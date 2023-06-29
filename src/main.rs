use std::collections::HashMap;

use mini_redis::Command::{Get, Set};
use mini_redis::{Command, Connection, Frame};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:6379").await.unwrap();
    loop {
        // The second item contains the IP and port of the new connections.
        let (socket, _) = listener.accept().await.unwrap();

        tokio::spawn(async move {
            // A new task is spawned for each inbound socket. The socket is moved to the new task
            // and processed there.
            // Tokio's Tasks are *green threads*.
            process(socket).await;
        });
    }
}

async fn process(socket: TcpStream) {
    let mut db = HashMap::new();

    // The `Connection` type, defined by mini-redis, abstracts byte streams into redis **frames**.
    let mut connection = Connection::new(socket);

    // Use `read_frame` to receive a command from the connection.
    while let Some(frame) = connection.read_frame().await.unwrap() {
        let response = match Command::from_frame(frame).unwrap() {
            Set(cmd) => {
                // The value is stored as `Vec<u8>`
                db.insert(cmd.key().to_string(), cmd.value().to_vec());
                // Sending an OK response
                Frame::Simple("OK".to_string())
            }
            Get(cmd) => {
                if let Some(value) = db.get(cmd.key()) {
                    // `Frame::Bulk` expects data to be of type `Bytes`.
                    // `&Vec<u8>` is converted to `Bytes` using `into()`.
                    Frame::Bulk(value.clone().into())
                } else {
                    Frame::Null
                }
            }
            cmd => panic!("Not implemented {:?}", cmd),
        };

        connection.write_frame(&response).await.unwrap();
    }
}
