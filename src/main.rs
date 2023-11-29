use std::net::IpAddr;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::net::{Shutdown, TcpStream};
use std::result;
use std::str;
use std::thread;
use std::fmt;
use std::io::{Write, Read};
use std::fmt::Display;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{SystemTime, Duration};

type Result<T> = result::Result<(), T>;

const SAFE_MODE: bool = true;
const BAN_LIMIT: Duration = Duration::from_secs(10*60);
const MESSAGE_RATE: Duration = Duration::from_secs(1);
const STRIKE_LIMIT: i32 = 10;

struct Sensitive<T> (T);

impl<T: Display> Display for Sensitive<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(inner) = self;
        if SAFE_MODE {
            writeln!(fmt, "[REDACTED]")
        }else {
            inner.fmt(fmt)
        }
    }
}

fn client(stream: Arc<TcpStream>, messages: Sender<Message>) -> Result<()>{
    let author_addr = stream.peer_addr().map_err(|err| {
        eprintln!("ERROR: could not get peer address {err}");
    })?;
    messages.send(Message::ClientConnected{author: stream.clone()}).map_err(|err| {
        eprintln!("ERROR: could not send message to the server thread: {err}");
    })?;
    let mut buffer = Vec::new();
    buffer.resize(64, 0);
    loop {
        let n = stream.as_ref().read(&mut buffer).map_err(|err| {
            eprintln!("ERROR: could not read message from client: {err}");
            let _ =  messages.send(Message::ClientDisconnected{author_addr});
        })?;
        if n > 0 {
            let _ = messages.send(Message::NewMessage{bytes: buffer[0..n].to_vec(), author_addr}).map_err(|err| {
                eprintln!("ERROR: could not read message from client: {err}");
            });
        } else {
            let _ = messages.send(Message::ClientDisconnected{author_addr}).map_err(|err| {
                eprintln!("ERROR: could not send message to the server thread: {err}");
            });
            break;
        }
    }
    Ok(())
}

enum Message {
    ClientConnected{
        author: Arc<TcpStream>,
    },
    ClientDisconnected{
        author_addr: SocketAddr
    },
    NewMessage{
        author_addr: SocketAddr,
        bytes: Vec<u8>,
    }
}

struct Client {
    conn: Arc<TcpStream>,
    last_message: SystemTime,
    strike_count: i32, 
}

fn server(messages: Receiver<Message>) -> Result<()> {
    let mut clients = HashMap::new();
    let mut banned_clients = HashMap::<IpAddr, SystemTime>::new();
    loop {
        let msg = messages.recv().expect("The server receiver is not hung up");
        match msg {
            Message::ClientConnected{author} => {
                let author_addr = author.peer_addr().expect("");
                let mut banned_at = banned_clients.remove(&author_addr.ip());
                let now = SystemTime::now();

                banned_at = banned_at.and_then(|banned_at| {
                    let duration = now.duration_since(banned_at).expect("TODO: don't crash if the clock went backwards");

                    if duration >= BAN_LIMIT {
                        None
                    }else {
                        Some(banned_at)
                    }

                });

                if let Some(banned_at) = banned_at  {
                    banned_clients.insert(author_addr.ip(), banned_at);
                    let mut author = author.as_ref();
                    let diff = now.duration_since(banned_at).expect("TODO: don't crash if the clock went backwards");
                    let secs = (BAN_LIMIT - diff).as_secs_f32();
                    println!("INFO: Client {author_addr} tried to connect but that client is bannedfor {secs} secs left");
                    let _ = writeln!(author, "you are banned!: {secs} secs left", secs = diff.as_secs_f32());
                    let _ = author.shutdown(Shutdown::Both);
                }else {
                    eprintln!("INFO: Client {author_addr} connected");
                    clients.insert(author_addr.clone(), Client {
                        conn: author.clone(),
                        last_message: now, 
                        strike_count: 0,
                    });
                }
            },
            Message::ClientDisconnected{author_addr} => {
                eprintln!("INFO: Client {author_addr} disconnected");
                clients.remove(&author_addr);
            },
            Message::NewMessage{author_addr, bytes }=> {

                if let Some(author) = clients.get_mut(&author_addr) {
                    let now = SystemTime::now();
                    let diff = now.duration_since(author.last_message).expect("TODO: don't crash is the clock went backwards");
                    if diff >= MESSAGE_RATE {
                        if let Ok(text) = str::from_utf8(&bytes) {
                            eprintln!("INFO: Client {author_addr} send message {text}");
                            for (addr, client) in clients.iter() {
                                if *addr != author_addr {
                                    let _ = client.conn.as_ref().write(&bytes);
                                }
                            }        
                        }else {
                            author.strike_count += 1;
                            if author.strike_count >= STRIKE_LIMIT {
                                eprintln!("INFO: Client {author_addr} is banned");
                                banned_clients.insert(author_addr.ip().clone(), now);
                                let _ = writeln!(author.conn.as_ref(), "You are banned!");
                                let _ = author.conn.shutdown(Shutdown::Both);
                            }
                        }
                    }else {
                        author.strike_count += 1;
                        if author.strike_count >= STRIKE_LIMIT {
                            eprintln!("INFO: Client {author_addr} is banned");
                            banned_clients.insert(author_addr.ip().clone(), now);
                            let _ = writeln!(author.conn.as_ref(), "You are banned!");
                            let _ = author.conn.shutdown(Shutdown::Both);
                        }
                    }
                }

            },
        }
    }
}

fn main() -> Result<()> {
    let address = "0.0.0.0:8000";
    let listener = TcpListener::bind(address).map_err(|err| {
        eprintln!("ERROR: could not bind {}: {}",Sensitive(address), Sensitive(err));
    })?;
    println!("INFO: Listening at address: {}", Sensitive(address));

    let (message_sender, message_receiver) = channel();

    thread::spawn(||server(message_receiver));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let message_sender = message_sender.clone();
                thread::spawn(|| {
                    let _  = client(stream.into(), message_sender);
                });
            },
            Err(err) => {
                eprintln!("ERROR: could not accept connection: {err}");
            },
        }
    }
    Ok(())
}
