use anyhow::Result;
use anyhow::{anyhow, bail};
use std::collections::BTreeSet;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::net::TcpStream;
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
use interprocess::local_socket::{
    traits::Listener,
    GenericNamespaced,
    GenericFilePath,
    ListenerOptions,
    ToFsName,
    ToNsName,
};
use interprocess::local_socket::NameType;
use std::time::{Duration as StdDuration, Instant};

// --- Helpers for parsing/serialization and logging --------------------------
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

fn send_packet_and_debug(stream: &mut TcpStream, packet: &str) -> Result<()> {
    // minimal debug output; actual logging elsewhere
    eprintln!("TX: {}", packet.trim_end());
    stream.write_all(packet.as_bytes())
        .map_err(|e| anyhow!("write_all failed: {}", e))?;
    stream.flush()
        .map_err(|e| anyhow!("flush failed: {}", e))?;
    Ok(())
}

// --- State definition -------------------------------------------------------
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
    pub last_baton_instant: Option<Instant>,

    // toggles (added roll/pitch/yaw/g_force)
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
    pub roll_toggle: bool,
    pub pitch_toggle: bool,
    pub yaw_toggle: bool,
    pub gforce_toggle: bool,

    pub ipc_bichannel: Option<ParentBiChannel<ToIpcThreadMessage, FromIpcThreadMessage>>,
    pub tcp_bichannel: Option<ParentBiChannel<ToTcpThreadMessage, FromTcpThreadMessage>>,
    pub last_send_timestamp: Option<String>,

    // Optional GUI error message and card state (required by update/view)
    pub error_message: Option<String>,
    pub card_open: bool,

    // metrics/UI helpers
    pub packets_last_60s: usize,
    pub bps: f64,
    pub show_metrics: bool,

    // tcp packet counters
    pub sent_packet_times: std::collections::VecDeque<Instant>,
    pub sent_samples: std::collections::VecDeque<(Instant, usize)>,
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
            roll_toggle: true,
            pitch_toggle: true,
            yaw_toggle: true,
            gforce_toggle: true,

            ipc_bichannel: None,
            tcp_bichannel: None,
            last_send_timestamp: None,
            saved_tcp_addrs: Vec::new(),
            selected_tcp_addr: None,
            tcp_addr_validation_error: None,

            // new UI fields
            error_message: None,
            card_open: false,

            packets_last_60s: 0,
            bps: 0.0,
            show_metrics: false,

            sent_packet_times: std::collections::VecDeque::new(),
            sent_samples: std::collections::VecDeque::new(),
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

// --- State impl -------------------------------------------------------------
impl State {
    // Timestamped event logger used by the UI/event history
    pub fn log_event(&mut self, event: String) {
        let entry = format!("[{}] {}", chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true), event);
        self.event_log.push(entry);
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
            // Choose namespace consistently with the Baton client:
            // - If GenericNamespaced is supported on this OS, use it.
            // - Otherwise fall back to a filesystem-backed name in the temp dir.
            let name = if GenericNamespaced::is_supported() {
                printname.to_ns_name::<GenericNamespaced>().unwrap()
            } else {
                let mut path = std::env::temp_dir();
                path.push(printname);
                path.to_fs_name::<GenericFilePath>().unwrap()
            };

            // Debug: show the concrete socket name/identifier the relay will listen on
            println!("[RELAY] listening on socket: {:?}", name.borrow());
            
            let opts = ListenerOptions::new().name(name.clone());
            let listener = match opts.create_sync() {
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    eprintln!("Error: socket file occupied: {}", printname);
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
                Ok(l) => {
                    println!("✓ Successfully created named pipe listener");
                    l
                }
                Err(e) => {
                    eprintln!("✗ Failed to create listener: {} (kind: {:?})", e, e.kind());
                    return Err(anyhow!("Failed to create listener: {}", e));
                }
            };
            listener
                .set_nonblocking(interprocess::local_socket::ListenerNonblockingMode::Both)
                .expect("Error setting non-blocking mode on listener");
            println!("Server running at {printname}");
            let mut buffer = String::with_capacity(256);
            human_log("IPC", &format!("Server running at {}", printname));
            let mut buffer = String::with_capacity(128);
            while !child_bichannel.is_killswitch_engaged() {
                let conn = listener.accept();
                let conn = match (child_bichannel.is_killswitch_engaged(), conn) {
                    (true, _) => return Ok(()),
                    (_, Ok(c)) => {
                        println!("[RELAY] Accepted incoming IPC connection");
                        human_log("IPC", "Accepted incoming connection");
                        c
                    }
                    (_, Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    (_, Err(e)) => {
                        human_log("IPC", &format!("Incoming connection failed: {}", e));
                        continue;
                    }
                };
                let mut conn = BufReader::new(conn);
                child_bichannel.set_is_conn_to_endpoint(true)?;
                
                // read initial/greeting line if present - give it time for non-blocking socket
                let mut greeting_attempts = 0;
                loop {
                    match conn.read_line(&mut buffer) {
                        Ok(n) if n > 0 => {
                            println!("[RELAY] Received greeting: {}", buffer.trim());
                            break;
                        }
                        Ok(_) => {
                            // Empty read, connection might be closed
                            break;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            greeting_attempts += 1;
                            if greeting_attempts > 100 {
                                // Give up waiting for greeting after 1 second
                                break;
                            }
                            std::thread::sleep(StdDuration::from_millis(10));
                            continue;
                        }
                        Err(e) => {
                            eprintln!("Initial read error: {e}");
                            break;
                        }
                    }
                }
                
                // Send greeting response and ensure it's flushed
                if let Err(e) = conn.get_mut().write_all(b"Hello, from the relay (Rust)!\n") {
                    eprintln!("Failed to send greeting: {e}");
                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                    continue;
                }
                if let Err(e) = conn.get_mut().flush() {
                    eprintln!("Failed to flush greeting: {e}");
                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                    continue;
                }
                println!("[RELAY] Sent greeting response");
                buffer.clear();

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
                    for message in child_bichannel.received_messages() {
                        match message {}
                    }
                    match conn.read_line(&mut buffer) {
                        Ok(0) => {
                            // For non-blocking sockets, Ok(0) could just mean no data available yet
                            // We need to check if the connection is actually closed
                            // Try a small read to verify
                            std::thread::sleep(StdDuration::from_millis(1));
                            continue;
                        }
                        Ok(_s) => {
                            // debug: show exactly what we got from IPC before forwarding
                            println!("[RELAY] IPC raw line: {:?}", buffer);
                            let _ = buffer.pop();
                            let _ = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonData(buffer.clone()));
                            buffer.clear();
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(StdDuration::from_millis(1));
                            continue;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                            // Actual broken pipe - connection closed
                            eprintln!("IPC connection closed (broken pipe)");
                            let _ = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonShutdown);
                            let _ = child_bichannel.set_is_conn_to_endpoint(false);
                            break;
                        }
                        Err(e) => {
                            eprintln!("IPC read error: {e}");
                            let _ = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonShutdown);
                            let _ = child_bichannel.set_is_conn_to_endpoint(false);
                            break;
                        }
                    }
                }
                let _ = child_bichannel.set_is_conn_to_endpoint(false);
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
        let res = handle.join().map_err(|e| anyhow!("Join handle err: {e:?}"))?;
        Ok(res?)
    }

        let res = handle
            .join()
            .map_err(|e| anyhow!("Join handle err: {e:?}"))?;
        Ok(res?)
    }
    pub fn is_ipc_connected(&self) -> bool {
        if let Some(status) = self
            .ipc_bichannel
            .as_ref()
            .and_then(|b| b.is_conn_to_endpoint().ok())
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
            // try to connect once, then loop handling messages while connected
            match TcpStream::connect(address) {
                Ok(mut stream) => {
                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                    // main connected loop
                    while !child_bichannel.is_killswitch_engaged() {
                        for message in child_bichannel.received_messages() {
                            match message {
                                ToTcpThreadMessage::Send(data) => {
                                    // parse normalized baton payload into fields
                                    let fields = normalize_baton_payload(&data);
                                    // Keep the original behavior: plugin sends pilot-only values
                                    // New behavior: support up to 8 pilot values:
                                    // [Altitude, Airspeed, Heading, VerticalVelocity, Roll, Pitch, Yaw, GForce]
                                    // Also support paired FM/Pilot sequences (FM,Pilot,FM,Pilot,...)
                                    // Strategy:
                                    //  - If fields.len() == 4: treat as original pilot-only (duplicate into FM,Pilot)
                                    //  - If fields.len() == 8: treat as pilot-only extended (generate 8 events duplicating FM)
                                    //  - Otherwise, attempt paired mapping: consume pairs sequentially and emit packets for known samples
                                    if fields.len() < 2 {
                                        eprintln!("Dropping packet: not enough fields: {:?}", fields);
                                        continue;
                                    }

                                    // helper to try send and handle error by marking endpoint disconnected
                                    let mut try_send = |pkt: String| -> Result<()> {
                                        let res = send_packet_and_debug(&mut stream, &pkt);
                                        if res.is_err() {
                                            let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                        } else {
                                            let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                        }
                                        res
                                    };

                                    // If exactly 4 fields -> legacy pilot-only
                                    if fields.len() == 4 {
                                        let alt = fields[0].clone();
                                        let air = fields[1].clone();
                                        let head = fields[2].clone();
                                        let vv = fields[3].clone();

                                        let altitude_packet = build_imotions_packet("AltitudeSync", &[alt.clone(), alt.clone()]);
                                        try_send(altitude_packet)?;

                                        let airspeed_packet = build_imotions_packet("AirspeedSync", &[air.clone(), air.clone()]);
                                        try_send(airspeed_packet)?;

                                        let vv_packet = build_imotions_packet("VerticalVelocitySync", &[vv.clone(), vv.clone()]);
                                        try_send(vv_packet)?;

                                        let heading_packet = build_imotions_packet("HeadingSync", &[head.clone(), head.clone()]);
                                        try_send(heading_packet)?;
                                        continue;
                                    }

                                    // If exactly 8 fields -> extended pilot-only order
                                    if fields.len() == 8 {
                                        let alt = fields[0].clone();
                                        let air = fields[1].clone();
                                        let head = fields[2].clone();
                                        let vv = fields[3].clone();
                                        let roll = fields[4].clone();
                                        let pitch = fields[5].clone();
                                        let yaw = fields[6].clone();
                                        let gforce = fields[7].clone();

                                        try_send(build_imotions_packet("AltitudeSync", &[alt.clone(), alt.clone()]))?;
                                        try_send(build_imotions_packet("AirspeedSync", &[air.clone(), air.clone()]))?;
                                        try_send(build_imotions_packet("VerticalVelocitySync", &[vv.clone(), vv.clone()]))?;
                                        try_send(build_imotions_packet("HeadingSync", &[head.clone(), head.clone()]))?;
                                        try_send(build_imotions_packet("RollSync", &[roll.clone(), roll.clone()]))?;
                                        try_send(build_imotions_packet("PitchSync", &[pitch.clone(), pitch.clone()]))?;
                                        try_send(build_imotions_packet("YawSync", &[yaw.clone(), yaw.clone()]))?;
                                        try_send(build_imotions_packet("GForceSync", &[gforce.clone(), gforce.clone()]))?;
                                        continue;
                                    }

                                    // Otherwise attempt paired mapping: assume sequence of pairs FM,Pilot
                                    // Map in the known order if present: Altitude, Airspeed, VerticalVelocity, Heading, Roll, Pitch, Yaw, GForce
                                    let mut idx = 0usize;
                                    let mut send_pair_if_present = |name: &str, idx: &mut usize| -> Result<()> {
                                        if *idx + 1 < fields.len() {
                                            let payload = vec![fields[*idx].clone(), fields[*idx + 1].clone()];
                                            let pkt = build_imotions_packet(name, &payload);
                                            *idx += 2;
                                            try_send(pkt)?;
                                        }
                                        Ok(())
                                    };

                                    let _ = send_pair_if_present("AltitudeSync", &mut idx);
                                    let _ = send_pair_if_present("AirspeedSync", &mut idx);
                                    let _ = send_pair_if_present("VerticalVelocitySync", &mut idx);
                                    let _ = send_pair_if_present("HeadingSync", &mut idx);
                                    let _ = send_pair_if_present("RollSync", &mut idx);
                                    let _ = send_pair_if_present("PitchSync", &mut idx);
                                    let _ = send_pair_if_present("YawSync", &mut idx);
                                    let _ = send_pair_if_present("GForceSync", &mut idx);
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
                        // small sleep to avoid busy loop
                        std::thread::sleep(StdDuration::from_millis(1));
                    }
                    Ok(())
                }
                Err(e) => {
                    eprintln!("TCP connect failed: {}", e);
                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                    return Err(anyhow!("Failed to connect to TCP: {}", e));
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
        bichannel.killswitch_engage()?;
        let res = handle.join().map_err(|e| anyhow!("Join handle err: {e:?}"))?;
        Ok(res?)
    }

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

    // Added this for tcp packet count -Nyla Hughes
    pub fn on_tcp_packet_sent(&mut self, bytes: usize) {
        let now = Instant::now();
        self.sent_packet_times.push_back(now);
        self.sent_samples.push_back((now, bytes));
        self.refresh_metrics(now);
    }

    pub fn refresh_metrics_now(&mut self) {
        let now = Instant::now();
        self.refresh_metrics(now);
    }

    fn refresh_metrics(&mut self, now: Instant) {
        // last 60 seconds -> packet count
        let window60 = std::time::Duration::from_secs(60);
        while let Some(&t) = self.sent_packet_times.front() {
            if now.duration_since(t) > window60 {
                self.sent_packet_times.pop_front();
            } else {
                break;
            }
        }
        self.packets_last_60s = self.sent_packet_times.len();

        // last 1 second -> throughput
        let window1 = std::time::Duration::from_secs(1);
        while let Some(&(t, _)) = self.sent_samples.front() {
            if now.duration_since(t) > window1 {
                self.sent_samples.pop_front();
            } else {
                break;
            }
        }
        let bytes_last_1s: usize = self.sent_samples.iter().map(|&(_, b)| b).sum();
        self.bps = (bytes_last_1s as f64) * 8.0;
        self.show_metrics = self.packets_last_60s > 0 || self.bps >= 1.0;
    }
}
    pub fn log_event(&mut self, event: String) {
        // store a timestamped copy for the UI/event history
        let entry = format!("[{}] {}", now_epoch_millis(), event);
        self.event_log.push(entry);
    }
}
