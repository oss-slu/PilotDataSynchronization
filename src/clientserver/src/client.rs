use std::io::{Read, Write};
use std::net::TcpStream;
use std::str::from_utf8;

fn main() -> std::io::Result<()> {
    // Connect to the server
    match TcpStream::connect("127.0.0.1:7878") {
        Ok(mut stream) => {
            println!("Successfully connected to server at 127.0.0.1:7878");
            
            // Send a message to the server
            let message = "Hello from the Rust TCP client!";
            stream.write(message.as_bytes())?;
            println!("Sent: {}", message);
            
            // Buffer to store server response
            let mut buffer = [0; 1024];
            match stream.read(&mut buffer) {
                Ok(size) => {
                    // Convert the received bytes to a string
                    let response = from_utf8(&buffer[0..size]).unwrap();
                    println!("Received: {}", response);
                }
                Err(e) => {
                    eprintln!("Failed to receive response: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to connect: {}", e);
        }
    }
    
    Ok(())
}