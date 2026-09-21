use std::env;
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::process;
use std::thread;

// Matches the relay's placeholder address, 127.0.0.1:9999
const DEFAULT_PORT: u16 = 9999;

// Reads newline-terminated packets until the peer closes the connection,
// printing each one. Returns the number of packets read.
fn read_packets<R: BufRead>(mut reader: R, peer: &str) -> u64 {
    let mut line = Vec::new();
    let mut count = 0;
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            // Zero bytes means the peer closed the connection
            Ok(0) => break,
            Ok(_) => {
                count += 1;
                let packet = String::from_utf8_lossy(&line);
                println!("[{peer}] #{count}: {}", packet.trim_end());
            }
            Err(e) => {
                eprintln!("[{peer}] Read failed: {e}");
                break;
            }
        }
    }
    count
}

fn handle_client(stream: TcpStream, peer: String) {
    let count = read_packets(BufReader::new(stream), &peer);
    println!("[{peer}] Disconnected. Packets received: {count}");
}

// Uses the default port when no argument is given
fn parse_port(arg: Option<String>) -> Result<u16, String> {
    match arg {
        None => Ok(DEFAULT_PORT),
        Some(s) => s.parse::<u16>().map_err(|_| format!("Invalid port: {s}")),
    }
}

fn main() -> std::io::Result<()> {
    let port = match parse_port(env::args().nth(1)) {
        Ok(port) => port,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("Usage: cargo run -- [port]  (default {DEFAULT_PORT})");
            process::exit(2);
        }
    };

    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!("Server listening on {}", listener.local_addr()?);

    // Listen for incoming connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // Fails if the client disconnected before we got here
                let peer = match stream.peer_addr() {
                    Ok(addr) => addr.to_string(),
                    Err(e) => {
                        eprintln!("Could not read peer address: {e}");
                        "unknown peer".to_string()
                    }
                };
                println!("New connection: {peer}");

                // Spawn a new thread for each connection
                thread::spawn(move || {
                    handle_client(stream, peer);
                });
            }
            Err(e) => {
                eprintln!("Connection failed: {e}");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn counts_each_packet_including_unterminated_last_one() {
        let data = b"E;1;PilotDataSync;;;;;AltitudeSync;1.0;1.0\r\n\
                     E;1;PilotDataSync;;;;;AirspeedSync;2.0;2.0\r\n\
                     E;1;PilotDataSync;;;;;HeadingSync;3.0;3.0";
        assert_eq!(read_packets(Cursor::new(&data[..]), "test"), 3);
    }

    #[test]
    fn empty_stream_reads_no_packets() {
        assert_eq!(read_packets(Cursor::new(&b""[..]), "test"), 0);
    }

    #[test]
    fn parse_port_defaults_when_no_argument() {
        assert_eq!(parse_port(None), Ok(DEFAULT_PORT));
    }

    #[test]
    fn parse_port_accepts_valid_port() {
        assert_eq!(parse_port(Some("7878".to_string())), Ok(7878));
    }

    #[test]
    fn parse_port_rejects_invalid_input() {
        assert!(parse_port(Some("abc".to_string())).is_err());
        assert!(parse_port(Some("70000".to_string())).is_err());
        assert!(parse_port(Some("-1".to_string())).is_err());
    }
}
