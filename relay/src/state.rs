use anyhow::Result;
use anyhow::{anyhow, bail};
use std::collections::BTreeSet;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::thread::{JoinHandle, spawn};
use iced::time::Duration;
use crate::bichannel;
use crate::bichannel::ParentBiChannel;
use crate::message::FromIpcThreadMessage;
use crate::message::FromTcpThreadMessage;
use crate::message::ToIpcThreadMessage;
use crate::message::ToTcpThreadMessage;
use interprocess::local_socket::{traits::Listener, GenericNamespaced, ListenerOptions, ToNsName};
use std::time::{Instant, SystemTime, UNIX_EPOCH, Duration as StdDuration};
use std::sync::{OnceLock, Mutex};

// --- State definition and Default impl (replace existing block) ---
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
    // timestamp of last received baton packet (used by update logic)
    pub last_baton_instant: Option<Instant>,
    // simple metrics/UI helpers
    pub show_metrics: bool,
    pub packets_last_60s: usize,
    pub bps: f64,
    // Optional GUI error message
    pub error_message: Option<String>,
    // Is GUI pop-up card open
    pub card_open: bool,
    // GUI Toggle state elements
    pub altitude_toggle: bool,
    pub airspeed_toggle: bool,
    pub vertical_airspeed_toggle: bool,
    pub heading_toggle: bool,
    pub ipc_bichannel: Option<ParentBiChannel<ToIpcThreadMessage, FromIpcThreadMessage>>,
    pub tcp_bichannel: Option<ParentBiChannel<ToTcpThreadMessage, FromTcpThreadMessage>>,
    pub last_send_timestamp: Option<String>,
    pub saved_tcp_addrs: Vec<String>,
    pub selected_tcp_addr: Option<String>,
    pub tcp_addr_validation_error: Option<String>,
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
            saved_tcp_addrs: Vec::new(),
            selected_tcp_addr: None,
            tcp_addr_validation_error: None,

        }
    }
}
// --- helper functions -------------------------------------------------------
fn sanitize_field(s: &str) -> String {
    // remove CR/LF and replace any internal semicolons with commas,
    // trim whitespace
    s.replace('\r', "")
        .replace('\n', "")
        .replace(';', ",")
        .trim()
        .to_string()
}
fn normalize_baton_payload(raw: &str) -> Vec<String> {
    // trim whitespace, remove surrounding CR/LF
    let mut s = raw.trim().replace('\r', "").replace('\n', "");
    // remove leading semicolons that create empty first fields
    while s.starts_with(';') {
        s.remove(0);
    }
    // also remove trailing semicolons (avoid empty trailing field)
    while s.ends_with(';') {
        s.pop();
    }
    // split on semicolon and sanitize each field
    s.split(';')
        .map(|f| sanitize_field(f))
        .filter(|f| !f.is_empty())
        .collect()
}
fn build_imotions_packet(event_name: &str, fields: &[String]) -> String {
    // Header used in previous code: "E;1;PilotDataSync;;;;;{Event};{fields...}\r\n"
    let mut packet = String::from("E;1;PilotDataSync;;;;;");
    packet.push_str(event_name);
    if !fields.is_empty() {
        packet.push(';');
        packet.push_str(&fields.join(";"));
    }
    packet.push_str("\r\n");
    packet
}

/// Produce a compact, human-friendly timestamp (seconds since epoch + millis).
fn now_epoch_millis() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0));
    format!("{}.{:03}", now.as_secs(), now.subsec_millis())
}

// -- Buffered human logger --------------------------------------------------
//
// Behaviour:
//  - Messages are buffered instead of printed immediately.
//  - If a new message differs from the last logged message it is flushed
//    immediately (change detected).
//  - If messages are identical, they will be flushed at most once per
//    `LOG_FLUSH_INTERVAL_MS` milliseconds (periodic heartbeat).
//  - This reduces continuous identical output in PowerShell while keeping
//    readable, timestamped logs on change or periodically.
//
const LOG_FLUSH_INTERVAL_MS: u64 = 2000;

struct LoggerState {
    buffer: Vec<String>,
    last_flush: Instant,
    last_msg: Option<String>,
}

static LOGGER: OnceLock<Mutex<LoggerState>> = OnceLock::new();

fn init_logger() -> &'static Mutex<LoggerState> {
    LOGGER.get_or_init(|| {
        Mutex::new(LoggerState {
            buffer: Vec::new(),
            last_flush: Instant::now(),
            last_msg: None,
        })
    })
}

/// Flush buffered log lines to stderr (PowerShell) and update last_flush.
fn flush_logger() {
    let mutex = init_logger();
    let mut st = mutex.lock().expect("logger mutex poisoned");
    if st.buffer.is_empty() {
        return;
    }
    for line in st.buffer.drain(..) {
        eprintln!("{}", line);
    }
    st.last_flush = Instant::now();
}

/// Human-facing logger that buffers and flushes on change or periodically.
/// The formatted entry excludes the timestamp part used for deduplication,
/// so only the message content + level is compared.
fn human_log(level: &str, msg: &str) {
    let entry_body = format!("{} - {}", level, msg);
    let full_entry = format!("[{}] {}", now_epoch_millis(), entry_body);

    let mutex = init_logger();
    let now = Instant::now();

    {
        let mut st = mutex.lock().expect("logger mutex poisoned");

        // If content changed, push and mark for immediate flush.
        if st.last_msg.as_deref() != Some(&entry_body) {
            st.buffer.push(full_entry);
            st.last_msg = Some(entry_body);
            // drop lock before flushing to avoid double-lock
            drop(st);
            flush_logger();
            return;
        }

        // If identical and enough time passed since last flush, push & flush.
        if now.duration_since(st.last_flush) >= StdDuration::from_millis(LOG_FLUSH_INTERVAL_MS) {
            st.buffer.push(full_entry);
            // drop lock before flushing
            drop(st);
            flush_logger();
            return;
        }

        // Identical message and too soon to flush: skip pushing to avoid spam.
        // (We intentionally don't update last_flush or last_msg here.)
    }
}

/// Log and store event in in-memory event log (with timestamp).
fn push_event_log(state: &mut State, event: &str) {
    let entry = format!("[{}] {}", now_epoch_millis(), event);
    state.event_log.push(entry);
}
// Add this helper near your other helpers
fn send_packet_and_debug(stream: &mut std::net::TcpStream, packet: &str) -> Result<()> {
    // Print readable and hex views for debugging in a human-friendly format
    human_log("TX", &format!("packet len={} text={:?}", packet.len(), packet));
    let hex: String = packet
        .as_bytes()
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");
    human_log("TX", &format!("hex: {}", hex));
    // Write then flush -- report any error
    stream
        .write_all(packet.as_bytes())
        .map_err(|e| anyhow::anyhow!("write_all failed: {}", e))?;
    stream
        .flush()
        .map_err(|e| anyhow::anyhow!("flush failed: {}", e))?;
    Ok(())
}
// --- State impl -------------------------------------------------------------
impl State {
    fn saved_tcp_addrs_path() -> Result<PathBuf> {
        let mut path = dirs::config_dir()
            .ok_or_else(|| anyhow!("Could not find config directory"))?;
        path.push("PilotDataSynchronization");
        fs::create_dir_all(&path)?;
        path.push("relay_ips.json");
        Ok(path)
    }

    pub fn load_saved_tcp_addrs(&mut self) -> Result<()> {
        let path = Self::saved_tcp_addrs_path()?;
        let Ok(contents) = fs::read_to_string(path) else {
            return Ok(());
        };

        let mut uniq = BTreeSet::new();
        let trimmed = contents.trim();
        let body = trimmed
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .ok_or_else(|| anyhow!("InvalidTCP"))?;
        let addresses = if body.trim().is_empty() {
            Vec::new()
        } else {
            body.split(',')
                .map(|entry| entry.trim().trim_matches('"').to_string())
                .collect::<Vec<_>>()
        };
        for address in addresses {
            let candidate = address.trim();
            if Self::validate_tcp_addr(candidate).is_ok() {
                uniq.insert(candidate.to_string());
            }
        }

        self.saved_tcp_addrs = uniq.into_iter().collect();
        Ok(())
    }

    fn persist_saved_tcp_addrs(&self) -> Result<()> {
        let path = Self::saved_tcp_addrs_path()?;
        let contents = if self.saved_tcp_addrs.is_empty() {
            "[]\n".to_string()
        } else {
            format!(
                "[\n{}\n]\n",
                self.saved_tcp_addrs
                    .iter()
                    .map(|address| format!("  \"{}\"", address))
                    .collect::<Vec<_>>()
                    .join(",\n")
            )
        };
        fs::write(path, contents)?;
        Ok(())
    }

    pub fn validate_tcp_addr(address: &str) -> Result<()> {
        let _parsed = address
            .trim()
            .parse::<SocketAddr>()
            .map_err(|_| anyhow!("Invalid IP address. Use format: 127.0.0.1:9999"))?;
        Ok(())
    }

    pub fn save_tcp_addr_if_new(&mut self, address: &str) -> Result<()> {
        if !self.saved_tcp_addrs.iter().any(|saved| saved == address) {
            self.saved_tcp_addrs.push(address.to_string());
            self.saved_tcp_addrs.sort();
        }
        self.persist_saved_tcp_addrs()?;
        self.selected_tcp_addr = Some(address.to_string());
        Ok(())
    }

    // Simple metric helpers used by update/view code that expect them.
    pub fn refresh_metrics_now(&mut self) {
        // placeholder: in future compute accurate rates from history
        // Here we keep current values; could implement sliding window later.
        if self.packets_last_60s > 0 {
            // naive decay to avoid stale large counts (noop for now)
            self.packets_last_60s = self.packets_last_60s.saturating_sub(0);
        }
    }
    pub fn on_tcp_packet_sent(&mut self, bytes: usize) {
        // Update simple counters and log
        self.packets_last_60s = self.packets_last_60s.saturating_add(1);
        self.bps = bytes as f64;
        self.log_event(format!("Sent packet ({} bytes)", bytes));
    }
    pub fn ipc_connect(&mut self) -> Result<()> {
        if self.ipc_thread_handle.is_some() {
            bail!("IPC thread already exists.")
        }
        let (ipc_bichannel, mut child_bichannel) =
            bichannel::create_bichannels::<ToIpcThreadMessage, FromIpcThreadMessage>();
        let ipc_thread_handle = spawn(move || {
            let printname = "baton.sock";
            let name = printname.to_ns_name::<GenericNamespaced>().unwrap();
            let opts = ListenerOptions::new().name(name);
            let listener = match opts.create_sync() {
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    human_log("IPC", &format!(
                        "Could not start server because the socket file is occupied. Check if {} is in use.",
                        printname
                    ));
                    return Ok(());
                }
                x => x.unwrap(),
            };
            listener
                .set_nonblocking(interprocess::local_socket::ListenerNonblockingMode::Both)
                .expect("Error setting non-blocking mode on listener");
            human_log("IPC", &format!("Server running at {}", printname));
            let mut buffer = String::with_capacity(128);
            while !child_bichannel.is_killswitch_engaged() {
                let conn = listener.accept();
                let conn = match (child_bichannel.is_killswitch_engaged(), conn) {
                    (true, _) => return Ok(()),
                    (_, Ok(c)) => {
                        human_log("IPC", "Accepted incoming connection");
                        c
                    }
                    (_, Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    (_, Err(e)) => {
                        human_log("IPC", &format!("Incoming connection failed: {}", e));
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
                human_log("IPC", &format!("Client answered: {}", buffer.trim_end()));
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
                        Ok(_s) => {
                            let _ = buffer.pop(); // remove trailing newline
                            human_log("IPC", &format!("Got: {} ({} bytes read)", buffer, _s));
                            if buffer.starts_with("SHUTDOWN") {
                                let _ = child_bichannel
                                    .send_to_parent(FromIpcThreadMessage::BatonShutdown);
                                return Ok(());
                            } else {
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
    pub fn is_ipc_connected(&self) -> bool {
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
        let (tcp_bichannel, mut child_bichannel) = bichannel::create_bichannels();
        self.tcp_bichannel = Some(tcp_bichannel);
        let tcp_thread_handle = spawn(move || {
            let mut stream = match TcpStream::connect(address) {
                Ok(stream) => {
                    human_log("TCP", "Successfully connected to iMotions server.");
                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                    stream
                }
                Err(e) => {
                    human_log("TCP", &format!("Connection failed: {}", e));
                    bail!("Failed to connect to TCP");
                }
            };
            while !child_bichannel.is_killswitch_engaged() {
                for message in child_bichannel.received_messages() {
                    match message {
                        ToTcpThreadMessage::Send(data) => {
                            // Normalize baton payload
                            let fields = normalize_baton_payload(&data);
                            // --- Flexible mapping for iMOTIONS events ---
                            // The relay accepts two common payload shapes:
                            // 1) Paired fields for each sample (FM,Pilot) in sequence:
                            //    [Alt_FM, Alt_Pilot, Air_FM, Air_Pilot, Vert_FM, Vert_Pilot, Head_FM, Head_Pilot]
                            // 2) Single pilot-only values in order:
                            //    [Altitude, Airspeed, Heading, VerticalVelocity]
                            //
                            // Emit whatever events we can from the incoming payload.
                            if fields.len() < 2 {
                                human_log("TCP", &format!(
                                    "Dropping packet: not enough fields (need >=2) but baton sent {}: {:?}",
                                    fields.len(), fields
                                ));
                                continue;
                            }
                            // If payload is exactly 4 fields, assume pilot-only order
                            if fields.len() == 4 {
                                // plugin order: Altitude, Airspeed, Heading, VerticalVelocity
                                // For iMotions we need (FlightModel, Pilot) pairs. Use the pilot value for both slots.
                                let alt = fields.get(0).unwrap().clone();
                                let air = fields.get(1).unwrap().clone();
                                let head = fields.get(2).unwrap().clone();
                                let vv = fields.get(3).unwrap().clone();
                                // Altitude
                                let altitude_packet = build_imotions_packet("AltitudeSync", &[alt.clone(), alt.clone()]);
                                human_log("TCP", &format!("Sending AltitudeSync: {:?}", altitude_packet));
                                if let Err(e) = send_packet_and_debug(&mut stream, &altitude_packet) {
                                    human_log("TCP", &format!("Failed to send Altitude packet: {}", e));
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                    return Err(e);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                                // Airspeed
                                let airspeed_packet = build_imotions_packet("AirspeedSync", &[air.clone(), air.clone()]);
                                human_log("TCP", &format!("Sending AirspeedSync: {:?}", airspeed_packet));
                                if let Err(e) = send_packet_and_debug(&mut stream, &airspeed_packet) {
                                    human_log("TCP", &format!("Failed to send Airspeed packet: {}", e));
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                    return Err(e);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                                // Vertical velocity
                                let vv_packet = build_imotions_packet("VerticalVelocitySync", &[vv.clone(), vv.clone()]);
                                human_log("TCP", &format!("Sending VerticalVelocitySync: {:?}", vv_packet));
                                if let Err(e) = send_packet_and_debug(&mut stream, &vv_packet) {
                                    human_log("TCP", &format!("Failed to send Vertical Velocity packet: {}", e));
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                    return Err(e);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                                // Heading
                                let heading_packet = build_imotions_packet("HeadingSync", &[head.clone(), head.clone()]);
                                human_log("TCP", &format!("Sending HeadingSync: {:?}", heading_packet));
                                if let Err(e) = send_packet_and_debug(&mut stream, &heading_packet) {
                                    human_log("TCP", &format!("Failed to send Heading packet: {}", e));
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                    return Err(e);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                                // done with this message
                                continue;
                            }
                            // Otherwise attempt the paired-fields mapping (previous behavior)
                            // Send AltitudeSync if we have at least 2 fields
                            if fields.len() >= 2 {
                                let altitude_payload = vec![
                                    fields.get(0).unwrap().clone(),
                                    fields.get(1).unwrap().clone(),
                                ];
                                let altitude_packet = build_imotions_packet("AltitudeSync", &altitude_payload);
                                human_log("TCP", &format!("Sending AltitudeSync: {:?}", altitude_packet));
                                if let Err(e) = send_packet_and_debug(&mut stream, &altitude_packet) {
                                    human_log("TCP", &format!("Failed to send Altitude packet: {}", e));
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                    return Err(e);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                            }
                            // Send AirspeedSync if we have at least 4 fields
                            if fields.len() >= 4 {
                                let airspeed_payload = vec![
                                    fields.get(2).unwrap().clone(),
                                    fields.get(3).unwrap().clone(),
                                ];
                                let airspeed_packet = build_imotions_packet("AirspeedSync", &airspeed_payload);
                                human_log("TCP", &format!("Sending AirspeedSync: {:?}", airspeed_packet));
                                if let Err(e) = send_packet_and_debug(&mut stream, &airspeed_packet) {
                                    human_log("TCP", &format!("Failed to send Airspeed packet: {}", e));
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                    return Err(e);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                            } else {
                                human_log("TCP", &format!("Airspeed packet skipped: need >=4 fields, have {}", fields.len()));
                            }
                            // Send VerticalVelocitySync if we have at least 6 fields
                            if fields.len() >= 6 {
                                let vv_payload = vec![
                                    fields.get(4).unwrap().clone(),
                                    fields.get(5).unwrap().clone(),
                                ];
                                let vv_packet = build_imotions_packet("VerticalVelocitySync", &vv_payload);
                                human_log("TCP", &format!("Sending VerticalVelocitySync: {:?}", vv_packet));
                                if let Err(e) = send_packet_and_debug(&mut stream, &vv_packet) {
                                    human_log("TCP", &format!("Failed to send Vertical Velocity packet: {}", e));
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                    return Err(e);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                            } else {
                                human_log("TCP", &format!("VerticalVelocity packet skipped: need >=6 fields, have {}", fields.len()));
                            }
                            // Send HeadingSync if we have at least 8 fields
                            if fields.len() >= 8 {
                                let heading_payload = vec![
                                    fields.get(6).unwrap().clone(),
                                    fields.get(7).unwrap().clone(),
                                ];
                                let heading_packet = build_imotions_packet("HeadingSync", &heading_payload);
                                human_log("TCP", &format!("Sending HeadingSync: {:?}", heading_packet));
                                if let Err(e) = send_packet_and_debug(&mut stream, &heading_packet) {
                                    human_log("TCP", &format!("Failed to send Heading packet: {}", e));
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                    return Err(e);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                            } else {
                                human_log("TCP", &format!("Heading packet skipped: need >=8 fields, have {}", fields.len()));
                            }
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
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
        let was_connected = bichannel.is_conn_to_endpoint().unwrap_or(false);
        bichannel.killswitch_engage()?;
        if !was_connected {
            spawn(move || {
                let _ = handle.join();
            });
            return Ok(());
        }
        let res = handle
            .join()
            .map_err(|e| anyhow!("Join handle err: {e:?}"))?;
        Ok(res?)
    }
    pub fn is_tcp_connected(&self) -> bool {
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
        // store a timestamped copy for the UI/event history
        let entry = format!("[{}] {}", now_epoch_millis(), event);
        self.event_log.push(entry);
    }
}
