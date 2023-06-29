use mini_redis::{Connection, Frame};
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
    // The `Connection` type, defined by mini-redis, abstracts byte streams into redis **frames**.
    let mut connection = Connection::new(socket);

    if let Some(frame) = connection.read_frame().await.unwrap() {
        println!("GOT: {:?}", frame);

        // Respond with a mini-redis `Error`
        let response = Frame::Error("not implemented".to_string());
        connection.write_frame(&response).await.unwrap();
    }
}
