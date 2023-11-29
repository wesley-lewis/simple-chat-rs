#![allow(dead_code)]

use std::net::TcpListener;
use std::net::TcpStream;
use std::result;
use std::thread;
use std::fmt;
use std::ops::Deref;
use std::io::Read;
use std::fmt::Display;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;

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
    messages.send(Message::ClientConnected(stream.clone())).map_err(|err| {
        eprintln!("ERROR: could not send message to the server thread: {err}");
    })?;
    let mut buffer = Vec::new();
    buffer.resize(64, 0);
    loop {
        let n = stream.deref().read(&mut buffer).map_err(|err| {
            eprintln!("ERROR: could not read message from client: {err}");
            let _ =  messages.send(Message::ClientDisconnected);
        })?;

        let _ = messages.send(Message::NewMessage(buffer[0..n].to_vec())).map_err(|err| {
            eprintln!("ERROR: could not read message from client: {err}");
        });
    }
}

enum Message {
    ClientConnected(Arc<TcpStream>),
    ClientDisconnected,
    NewMessage(Vec<u8>),
}

fn server(_messages: Receiver<Message>) -> Result<()> {
    todo!()
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
