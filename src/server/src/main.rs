use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn handle_client(mut stream: TcpStream) {
    // Buffer to store incoming data
    let mut buffer = [0; 1024];
    
    match stream.read(&mut buffer) {
        Ok(size) => {
            // Convert the received bytes to a string
            let received = String::from_utf8_lossy(&buffer[0..size]);
            println!("Received: {}", received);
            
            // Echo the message back with a prefix
            let response = format!("Server received: {}", received);
            match stream.write(response.as_bytes()) {
                Ok(_) => println!("Response sent"),
                Err(e) => eprintln!("Failed to send response: {}", e),
            }
        }
        Err(e) => eprintln!("Failed to receive data: {}", e),
    }
}

fn main() -> std::io::Result<()> {
    // Create a listener bound to address 127.0.0.1:7878
    let listener = TcpListener::bind("127.0.0.1:7878")?;
    println!("Server listening on port 7878");

    // Listen for incoming connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("New connection: {}", stream.peer_addr().unwrap());
                
                // Spawn a new thread for each connection
                thread::spawn(move || {
                    handle_client(stream);
                });
            }
            Err(e) => {
                eprintln!("Connection failed: {}", e);
            }
        }
    }
    
    Ok(())
}