use anyhow::{anyhow, bail, Result};
use iced::time::Duration;
use interprocess::local_socket::{traits::{Listener, Stream}, GenericNamespaced, GenericFilePath, ListenerOptions, NameType, ToFsName, ToNsName};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::thread::{spawn, JoinHandle};
use std::time::{Duration as StdDuration, Instant};

use crate::bichannel;
use crate::bichannel::ParentBiChannel;
use crate::message::{FromIpcThreadMessage, FromTcpThreadMessage, ToIpcThreadMessage, ToTcpThreadMessage};

/// Simple, consistent log helper used inside this module and spawned threads.
fn relay_log(msg: &str) {
    eprintln!("[relay] {}", msg);
}

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
    /// timestamp of last received baton packet (used by update logic)
    pub last_baton_instant: Option<Instant>,
    /// simple metrics/UI helpers
    pub show_metrics: bool,
    pub packets_last_60s: usize,
    pub bps: f64,
    /// Optional GUI error message
    pub error_message: Option<String>,
    /// Is GUI pop-up card open
    pub card_open: bool,
    /// GUI Toggle state elements
    pub altitude_toggle: bool,
    pub airspeed_toggle: bool,
    pub vertical_airspeed_toggle: bool,
    pub heading_toggle: bool,
    pub ipc_bichannel: Option<ParentBiChannel<ToIpcThreadMessage, FromIpcThreadMessage>>,
    pub tcp_bichannel: Option<ParentBiChannel<ToTcpThreadMessage, FromTcpThreadMessage>>,
    pub last_send_timestamp: Option<String>,
}

impl Default for State {
    fn default() -> State {
        State {
            elapsed_time: Duration::ZERO,
            event_log: Vec::new(),
            ipc_thread_handle: None,
            tcp_thread_handle: None,
            tcp_connected: false,
            tcp_addr_field: String::new(),
            latest_baton_send: None,
            active_baton_connection: false,
            last_baton_instant: None,
            show_metrics: false,
            packets_last_60s: 0,
            bps: 0.0,
            error_message: None,
            card_open: false,
            altitude_toggle: true,
            airspeed_toggle: true,
            vertical_airspeed_toggle: true,
            heading_toggle: true,
            ipc_bichannel: None,
            tcp_bichannel: None,
            last_send_timestamp: None,
        }
    }
}

// --- helpers ----------------------------------------------------------------

fn sanitize_field(s: &str) -> String {
    s.replace('\r', "")
        .replace('\n', "")
        .replace(';', ",")
        .trim()
        .to_string()
}

fn normalize_baton_payload(raw: &str) -> Vec<String> {
    let mut s = raw.trim().replace('\r', "").replace('\n', "");
    while s.starts_with(';') {
        s.remove(0);
    }
    while s.ends_with(';') {
        s.pop();
    }
    s.split(';')
        .map(|f| sanitize_field(f))
        .filter(|f| !f.is_empty())
        .collect()
}

fn build_imotions_packet(event_name: &str, fields: &[String]) -> String {
    let mut packet = String::from("E;1;PilotDataSync;;;;;");
    packet.push_str(event_name);
    if !fields.is_empty() {
        packet.push(';');
        packet.push_str(&fields.join(";"));
    }
    packet.push_str("\r\n");
    packet
}

fn send_packet(stream: &mut TcpStream, packet: &str) -> Result<()> {
    stream
        .write_all(packet.as_bytes())
        .map_err(|e| anyhow!("write_all failed: {}", e))?;
    stream
        .flush()
        .map_err(|e| anyhow!("flush failed: {}", e))?;
    Ok(())
}

// --- State impl -------------------------------------------------------------

impl State {
    pub fn refresh_metrics_now(&mut self) {
        if self.packets_last_60s > 0 {
            self.packets_last_60s = self.packets_last_60s.saturating_sub(0);
        }
    }

    pub fn on_tcp_packet_sent(&mut self, bytes: usize) {
        self.packets_last_60s = self.packets_last_60s.saturating_add(1);
        self.bps = bytes as f64;
        self.log_event(format!("Sent packet ({} bytes)", bytes));
    }

    pub fn is_ipc_connected(&self) -> bool {
        self.ipc_bichannel
            .as_ref()
            .and_then(|b| b.is_conn_to_endpoint().ok())
            .unwrap_or(false)
    }

    pub fn is_tcp_connected(&self) -> bool {
        self.tcp_bichannel
            .as_ref()
            .and_then(|b| b.is_conn_to_endpoint().ok())
            .unwrap_or(false)
    }

    pub fn ipc_connect(&mut self) -> Result<()> {
        if self.ipc_thread_handle.is_some() {
            bail!("IPC thread already exists.");
        }

        let (ipc_bichannel, mut child_bichannel) =
            bichannel::create_bichannels::<ToIpcThreadMessage, FromIpcThreadMessage>();

        let handle = spawn(move || {
            let printname = "baton.sock";

            // Pre-build an owned filesystem path string so any FsName conversion borrows an owned value
            // that lives for the duration of this closure (avoids temporary-borrow lifetime issues).
            let mut tmp_path_buf = std::env::temp_dir();
            tmp_path_buf.push(printname);
            let temp_path_string = tmp_path_buf.to_string_lossy().into_owned();

            // Prefer namespaced sockets when supported; otherwise fall back to filesystem-backed socket
            // in the system temp dir. Log which path form is selected and any conversion errors.
            let opts = if GenericNamespaced::is_supported() {
                // namespaced socket
                relay_log(&format!("IPC server using namespaced socket: {}", printname));
                let ns_name = printname.to_ns_name::<GenericNamespaced>().map_err(|e| {
                    relay_log(&format!("Failed to convert name to NsName: {}", e));
                    anyhow!("Name conversion failed")
                })?;
                ListenerOptions::new().name(ns_name)
            } else {
                // filesystem-backed socket in temp dir
                relay_log(&format!("IPC server using filesystem socket path: {}", temp_path_string));
                let fs_name = temp_path_string.as_str().to_fs_name::<GenericFilePath>().map_err(|e| {
                    relay_log(&format!("Failed to convert name to FsName: {}", e));
                    anyhow!("Name conversion failed")
                })?;
                ListenerOptions::new().name(fs_name)
            };

            let listener = opts.create_sync().map_err(|e| {
                relay_log(&format!("Failed to create IPC listener: {}", e));
                anyhow!("Listener create failed")
            })?;
            listener
                .set_nonblocking(interprocess::local_socket::ListenerNonblockingMode::Both)
                .map_err(|e| anyhow!("set_nonblocking failed: {}", e))?;
            relay_log("IPC server running");

            let mut buffer = String::with_capacity(128);
            while !child_bichannel.is_killswitch_engaged() {
                match listener.accept() {
                    Ok(conn) => {
                        relay_log("IPC incoming connection accepted");
                        let mut conn = BufReader::new(conn);

                        // Perform a blocking handshake with the client (baton).
                        // The baton writes a Hello line immediately after connecting and
                        // expects a reply before switching to non-blocking mode. Read
                        // that initial line, reply, then switch the underlying stream
                        // to non-blocking for normal operation.
                        match conn.read_line(&mut buffer) {
                            Ok(0) => {
                                relay_log("Handshake read returned 0 bytes; closing connection");
                                buffer.clear();
                                if let Err(e) = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonShutdown) {
                                    relay_log(&format!("Failed to send BatonShutdown during handshake: {}", e));
                                }
                                continue;
                            }
                            Ok(_) => {
                                let h = buffer.trim_end().to_string();
                                relay_log(&format!("IPC handshake received: {}", h));
                                buffer.clear();
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                // Unlikely for blocking socket, but be tolerant: treat as no handshake
                                relay_log("Handshake read would block; treating as failed handshake");
                                if let Err(e) = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonShutdown) {
                                    relay_log(&format!("Failed to send BatonShutdown after WouldBlock handshake: {}", e));
                                }
                                continue;
                            }
                            Err(e) => {
                                relay_log(&format!("Handshake read error: {}", e));
                                if let Err(e2) = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonShutdown) {
                                    relay_log(&format!("Failed to send BatonShutdown after handshake error: {}", e2));
                                }
                                continue;
                            }
                        }

                        // Send handshake reply
                        if let Err(e) = conn.get_mut().write_all(b"Hello, from the relay prototype (Rust)!\n") {
                            relay_log(&format!("Handshake reply failed: {}", e));
                            if let Err(e2) = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonShutdown) {
                                relay_log(&format!("Failed to send BatonShutdown after reply fail: {}", e2));
                            }
                            continue;
                        } else {
                            relay_log("Handshake reply sent");
                        }

                        // Switch connection to non-blocking mode for the main receive loop
                        if let Err(e) = conn.get_mut().set_nonblocking(true) {
                            relay_log(&format!("Failed to set connection non-blocking: {}", e));
                            // continue anyway; subsequent reads will attempt and handle WouldBlock
                        } else {
                            relay_log("Set accepted connection to non-blocking");
                        }

                        // mark connected and log result
                        match child_bichannel.set_is_conn_to_endpoint(true) {
                            Ok(_) => relay_log("Marked child_bichannel connected -> true"),
                            Err(e) => relay_log(&format!("Failed to mark child_bichannel connected: {}", e)),
                        }

                        loop {
                            if child_bichannel.is_killswitch_engaged() {
                                relay_log("Child killswitch engaged; breaking connection loop");
                                break;
                            }

                            // Check parent messages (none expected for now)
                            for _msg in child_bichannel.received_messages() {
                                // intentionally no-op; reserved for future commands
                            }

                            match conn.read_line(&mut buffer) {
                                Ok(0) => {
                                    // Connection closed by client — notify parent that baton disconnected
                                    relay_log("IPC read returned Ok(0) — connection closed by peer (EOF)");
                                    buffer.clear();
                                    if let Err(e) = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonShutdown) {
                                        relay_log(&format!("Failed to send BatonShutdown on EOF: {}", e));
                                    }
                                    break;
                                }
                                Ok(_) => {
                                    let _ = buffer.pop(); // remove trailing newline if present
                                    if buffer.starts_with("SHUTDOWN") {
                                        relay_log("IPC received SHUTDOWN message from client");
                                        if let Err(e) = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonShutdown) {
                                            relay_log(&format!("Failed to send BatonShutdown on SHUTDOWN: {}", e));
                                        }
                                        break;
                                    } else {
                                        // forward to parent and log on error
                                        if let Err(e) = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonData(buffer.clone())) {
                                            relay_log(&format!("Failed to forward BatonData to parent: {}", e));
                                        } else {
                                            // update last seen timestamp for diagnostics
                                            // (we cannot access State here, but logging helps)
                                            relay_log(&format!("Forwarded BatonData ({} bytes)", buffer.len()));
                                        }
                                    }
                                    buffer.clear();
                                }
                                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                    // normal when no data available
                                    std::thread::sleep(StdDuration::from_millis(1));
                                    continue;
                                }
                                Err(e) => {
                                    relay_log(&format!("IPC connection read error: {}", e));
                                    // Treat read error as a disconnect and notify parent
                                    if let Err(e2) = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonShutdown) {
                                        relay_log(&format!("Failed to send BatonShutdown after read error: {}", e2));
                                    }
                                    break;
                                }
                            }
                        }

                        // mark disconnected and log
                        match child_bichannel.set_is_conn_to_endpoint(false) {
                            Ok(_) => relay_log("Marked child_bichannel connected -> false"),
                            Err(e) => relay_log(&format!("Failed to mark child_bichannel disconnected: {}", e)),
                        };
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // no incoming connection right now
                        std::thread::sleep(StdDuration::from_millis(5));
                        continue;
                    }
                    Err(e) => {
                        relay_log(&format!("IPC accept failed: {}", e));
                        std::thread::sleep(StdDuration::from_millis(50));
                        continue;
                    }
                }
            }
            Ok(())
        });

        self.ipc_bichannel = Some(ipc_bichannel);
        self.ipc_thread_handle = Some(handle);
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

    pub fn tcp_connect(&mut self, address: String) -> Result<()> {
        if self.tcp_thread_handle.is_some() {
            bail!("TCP thread already exists.")
        }
        let (tcp_bichannel, mut child_bichannel) = bichannel::create_bichannels();
        // store parent side so UI/State can query/send to the TCP thread
        self.tcp_bichannel = Some(tcp_bichannel);

        let handle = spawn(move || {
            let mut stream = TcpStream::connect(address).map_err(|e| {
                relay_log(&format!("TCP connect failed: {}", e));
                anyhow!("Failed to connect to TCP")
            })?;
            relay_log("TCP connected");
            let _ = child_bichannel.set_is_conn_to_endpoint(true);

            while !child_bichannel.is_killswitch_engaged() {
                for message in child_bichannel.received_messages() {
                    match message {
                        ToTcpThreadMessage::Send(data) => {
                            let fields = normalize_baton_payload(&data);

                            if fields.len() < 2 {
                                relay_log(&format!("Dropping packet: not enough fields (need >=2) but baton sent {}: {:?}", fields.len(), fields));
                                continue;
                            }

                            // Pilot-only 4-field payload handling
                            if fields.len() == 4 {
                                let alt = fields[0].clone();
                                let air = fields[1].clone();
                                let head = fields[2].clone();
                                let vv = fields[3].clone();

                                let packets = [
                                    build_imotions_packet("AltitudeSync", &[alt.clone(), alt.clone()]),
                                    build_imotions_packet("AirspeedSync", &[air.clone(), air.clone()]),
                                    build_imotions_packet("VerticalVelocitySync", &[vv.clone(), vv.clone()]),
                                    build_imotions_packet("HeadingSync", &[head.clone(), head.clone()]),
                                ];

                                for pkt in &packets {
                                    if let Err(e) = send_packet(&mut stream, pkt) {
                                        relay_log(&format!("TCP send failed: {}", e));
                                        let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                        return Err(e);
                                    } else {
                                        let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                    }
                                }
                                continue;
                            }

                            // Paired-fields mapping (legacy behavior)
                            if fields.len() >= 2 {
                                let altitude_payload = vec![fields[0].clone(), fields[1].clone()];
                                let altitude_packet = build_imotions_packet("AltitudeSync", &altitude_payload);
                                if let Err(e) = send_packet(&mut stream, &altitude_packet) {
                                    relay_log(&format!("Failed to send Altitude packet: {}", e));
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                    return Err(e);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                            }

                            if fields.len() >= 4 {
                                let airspeed_payload = vec![fields[2].clone(), fields[3].clone()];
                                let airspeed_packet = build_imotions_packet("AirspeedSync", &airspeed_payload);
                                if let Err(e) = send_packet(&mut stream, &airspeed_packet) {
                                    relay_log(&format!("Failed to send Airspeed packet: {}", e));
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                    return Err(e);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                            }

                            if fields.len() >= 6 {
                                let vv_payload = vec![fields[4].clone(), fields[5].clone()];
                                let vv_packet = build_imotions_packet("VerticalVelocitySync", &vv_payload);
                                if let Err(e) = send_packet(&mut stream, &vv_packet) {
                                    relay_log(&format!("Failed to send Vertical Velocity packet: {}", e));
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                    return Err(e);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                            }

                            if fields.len() >= 8 {
                                let heading_payload = vec![fields[6].clone(), fields[7].clone()];
                                let heading_packet = build_imotions_packet("HeadingSync", &heading_payload);
                                if let Err(e) = send_packet(&mut stream, &heading_packet) {
                                    relay_log(&format!("Failed to send Heading packet: {}", e));
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                    return Err(e);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                            }
                        }
                    }
                }
                std::thread::sleep(StdDuration::from_millis(1));
            }
            Ok(())
        });

        self.tcp_thread_handle = Some(handle);
        Ok(())
    }

    pub fn tcp_disconnect(&mut self) -> Result<()> {
        if self.tcp_thread_handle.is_none() {
            bail!("TCP thread does not exist.")
        }
        let Some((biconductor, handle)) =
            self.tcp_bichannel.take().zip(self.tcp_thread_handle.take())
        else {
            bail!("TCP thread does not exist.")
        };
        biconductor.killswitch_engage()?;
        let res = handle
            .join()
            .map_err(|e| anyhow!("Join handle err: {e:?}"))?;
        Ok(res?)
    }

    pub fn log_event(&mut self, event: String) {
        self.event_log.push(event);
    }
}