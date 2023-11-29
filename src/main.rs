#![allow(dead_code)]

use std::net::TcpListener;
use std::net::TcpStream;
use std::result;
use std::thread;
use std::fmt;
use std::io::{Write, Read};
use std::fmt::Display;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use std::collections::HashMap;

type Result<T> = result::Result<(), T>;

const SAFE_MODE: bool = true;

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
    messages.send(Message::ClientConnected{author: stream.clone()}).map_err(|err| {
        eprintln!("ERROR: could not send message to the server thread: {err}");
    })?;
    let mut buffer = Vec::new();
    buffer.resize(64, 0);
    loop {
        let n = stream.as_ref().read(&mut buffer).map_err(|err| {
            eprintln!("ERROR: could not read message from client: {err}");
            let _ =  messages.send(Message::ClientDisconnected{ author: stream.clone()});
        })?;

        let _ = messages.send(Message::NewMessage{bytes: buffer[0..n].to_vec(), author: stream.clone()}).map_err(|err| {
            eprintln!("ERROR: could not read message from client: {err}");
        });
    }
}

enum Message {
    ClientConnected{
        author: Arc<TcpStream>,
    },
    ClientDisconnected{
        author: Arc<TcpStream>,
    },
    NewMessage{
        author: Arc<TcpStream>,
        bytes: Vec<u8>,
    }
}

struct Client {
    conn: Arc<TcpStream>,
}

fn server(messages: Receiver<Message>) -> Result<()> {
    let mut clients = HashMap::new();
    loop {
        let msg = messages.recv().expect("The server receiver is not hung up");
        match msg {
            Message::ClientConnected{author} => {
                let addr = author.peer_addr().expect("TODO: cache the peer addr of the connection");
                clients.insert(addr.clone(), Client {
                    conn: author.clone(),
                });
            },
            Message::ClientDisconnected{author} => {
                let addr = author.peer_addr().expect("TODO: cache the peer addr of the connection");
                clients.remove(&addr);
            },
            Message::NewMessage{author, bytes }=> {
                let author_addr = author.peer_addr().expect("TODO: cache the peer addr of the connection");

                for (addr, client) in clients.iter() {
                    if *addr != author_addr {
                        let _ = client.conn.as_ref().write(&bytes);
                    }
                }
            },
        }
    }
}

fn main() -> Result<()>{
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
