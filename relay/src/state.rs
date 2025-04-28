use anyhow::Result;
use anyhow::{anyhow, bail};
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::net::TcpStream;
use std::thread::JoinHandle;

use iced::time::Duration;

use crate::bichannel;
use crate::bichannel::ParentBiChannel;
use crate::message::FromIpcThreadMessage;
use crate::message::FromTcpThreadMessage;
use crate::message::ToIpcThreadMessage;
use crate::message::ToTcpThreadMessage;

// use crate::ChannelMessage;

use interprocess::local_socket::{traits::Listener, GenericNamespaced, ListenerOptions, ToNsName};

#[derive(Default)]
#[allow(unused)]
pub(crate) struct State {
    pub elapsed_time: Duration,

    pub event_log: Vec<String>,

    pub ipc_thread_handle: Option<JoinHandle<Result<()>>>,
    pub tcp_thread_handle: Option<JoinHandle<Result<()>>>,

    pub tcp_connected: bool,
    pub tcp_addr_field: String,
    pub latest_baton_send: Option<String>,
    pub active_baton_connection: bool,
    // pub recv: Option<std::sync::mpsc::Receiver<ChannelMessage>>,
    pub ipc_bichannel: Option<ParentBiChannel<ToIpcThreadMessage, FromIpcThreadMessage>>,
    pub tcp_bichannel: Option<ParentBiChannel<ToTcpThreadMessage, FromTcpThreadMessage>>,
}

impl State {
    pub fn ipc_connect(&mut self) -> Result<()> {
        if self.ipc_thread_handle.is_some() {
            bail!("IPC thread already exists.")
        }

        // TODO
        let (ipc_bichannel, mut child_bichannel) =
            bichannel::create_bichannels::<ToIpcThreadMessage, FromIpcThreadMessage>();
        let ipc_thread_handle = std::thread::spawn(move || {
            // sample pulled directly from `interprocess` documentation

            let printname = "baton.sock";
            let name = printname.to_ns_name::<GenericNamespaced>().unwrap();

            let opts = ListenerOptions::new().name(name);

            let listener = match opts.create_sync() {
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    eprintln!(
                        "Error: could not start server because the socket file is occupied. Please check if 
                        {printname} is in use by another process and try again."
                    );
                    return Ok(());
                }
                x => x.unwrap(),
            };
            listener
                .set_nonblocking(interprocess::local_socket::ListenerNonblockingMode::Both)
                .expect("Error setting non-blocking mode on listener");

            println!("Server running at {printname}");

            let mut buffer = String::with_capacity(128);

            while !child_bichannel.is_killswitch_engaged() {
                let conn = listener.accept();
                let conn = match (child_bichannel.is_killswitch_engaged(), conn) {
                    (true, _) => return Ok(()),
                    (_, Ok(c)) => {
                        println!("success");
                        c
                    }
                    (_, Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    (_, Err(e)) => {
                        eprintln!("Incoming connection failed: {e}");
                        continue;
                    }
                };

                let mut conn = BufReader::new(conn);
                child_bichannel.set_is_conn_to_endpoint(true)?;

                match conn.read_line(&mut buffer) {
                    Ok(_) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    _ => panic!(),
                }

                let write_res = conn
                    .get_mut()
                    .write_all(b"Hello, from the relay prototype (Rust)!\n");

                match write_res {
                    Ok(_) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    _ => panic!(),
                }

                print!("Client answered: {buffer}");
                buffer.clear();

                // Continuously receive data from plugin
                while !child_bichannel.is_killswitch_engaged() {
                    // check for any new messages from parent and act accordingly
                    for message in child_bichannel.received_messages() {
                        match message {}
                    }

                    // read from connection input
                    match conn.read_line(&mut buffer) {
                        Ok(s) if s == 0 || buffer.len() == 0 => {
                            buffer.clear();
                            continue;
                        }
                        Ok(s) => {
                            let _ = buffer.pop(); // remove trailing newline
                            println!("Got: {buffer} ({s} bytes read)");

                            // txx is the sender half of channel from ipc_connection_handle -> main_gui_thread

                            // TODO: change baton to send strings not floats,
                            // ^ UNABLE TO TEST THIS LOGIC UNTIL THAT HAPPENS

                            // baton shutdown message received. Send shutdown message and break to next connection
                            // if the first 8 letters or so contains "SHUTDOWN",
                            if buffer.starts_with("SHUTDOWN") {
                                let _ = child_bichannel
                                    .send_to_parent(FromIpcThreadMessage::BatonShutdown);
                                return Ok(());
                            } else {
                                // actual baton data received
                                let _ = child_bichannel.send_to_parent(
                                    FromIpcThreadMessage::BatonData(buffer.clone()),
                                );
                            }

                            buffer.clear();
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                        Err(e) => panic!("Got err {e}"),
                    }
                }
            }

            Ok(())
        });

        self.ipc_bichannel = Some(ipc_bichannel);
        self.ipc_thread_handle = Some(ipc_thread_handle);

        Ok(())
    }

    pub fn ipc_disconnect(&mut self) -> Result<()> {
        if self.ipc_thread_handle.is_none() {
            bail!("IPC thread does not exist.")
        }

        let Some((bichannel, handle)) =
            self.ipc_bichannel.take().zip(self.ipc_thread_handle.take())
        else {
            bail!("IPC thread does not exist.")
        };

        bichannel.killswitch_engage()?;
        let res = handle
            .join()
            .map_err(|e| anyhow!("Join handle err: {e:?}"))?;

        Ok(res?)
    }

    pub fn _is_ipc_connected(&self) -> bool {
        if let Some(status) = self
            .ipc_bichannel
            .as_ref()
            .and_then(|bichannel| bichannel.is_conn_to_endpoint().ok())
        {
            status
        } else {
            false
        }
    }

    pub fn tcp_connect(&mut self, address: String) -> Result<()> {
        if self.tcp_thread_handle.is_some() {
            bail!("TCP thread already exists.")
        }

        // TODO
        let (tcp_bichannel, mut child_bichannel) = bichannel::create_bichannels();
        self.tcp_bichannel = Some(tcp_bichannel);

        let tcp_thread_handle = std::thread::spawn(move || {
            let mut stream = match TcpStream::connect(address) {
                Ok(stream) => {
                    println!("Successfully connected.");
                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                    stream
                }

                Err(e) => {
                    println!("Connection failed: {}", e);
                    bail!("Failed to connect to TCP");
                }
            };

            while !child_bichannel.is_killswitch_engaged() {
                // check messages from main thread
                for message in child_bichannel.received_messages() {
                    match message {
                        ToTcpThreadMessage::Send(data) => {
                            let _ = stream.write_all(data.as_bytes());
                        }
                    }
                }
            }

            Ok(())
        });

        self.tcp_thread_handle = Some(tcp_thread_handle);

        Ok(())
    }

    pub fn tcp_disconnect(&mut self) -> Result<()> {
        if self.tcp_thread_handle.is_none() {
            bail!("TCP thread does not exist.")
        }

        let Some((bichannel, handle)) =
            self.tcp_bichannel.take().zip(self.tcp_thread_handle.take())
        else {
            bail!("TCP thread does not exist.")
        };

        bichannel.killswitch_engage()?;
        let res = handle
            .join()
            .map_err(|e| anyhow!("Join handle err: {e:?}"))?;

        Ok(res?)
    }

    pub fn _is_tcp_connected(&self) -> bool {
        if let Some(status) = self
            .tcp_bichannel
            .as_ref()
            .and_then(|bichannel| bichannel.is_conn_to_endpoint().ok())
        {
            status
        } else {
            false
        }
    }

    pub fn log_event(&mut self, event: String) {
        self.event_log.push(event);
    }
}
