use anyhow::{anyhow, bail, Result};
use chrono::Local;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::net::TcpStream;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::thread::{JoinHandle, spawn};
use iced::time::Duration;
use crate::bichannel;
use crate::bichannel::ChildBiChannel;
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
    NameType,
    ToFsName,
    ToNsName,
};
use std::time::{Duration as StdDuration, Instant, SystemTime, UNIX_EPOCH};
use std::sync::{OnceLock, Mutex};

/// How many `event_log` entries to keep. The GUI renders only the newest few,
/// so this is scrollback headroom. The cap keeps the log O(1) in memory even if
/// a future `log_event` call lands on a hot path, as happened before #203.
pub(crate) const EVENT_LOG_CAPACITY: usize = 200;

/// What to type into the relay's address box. Deliberately not a working
/// address: iMotions runs on its own machine in the lab, so the IP has to come
/// from there. 5050 is the port it was listening on, read from Global Settings,
/// API tab, Event Receiving, and that is a setting rather than a fixed default.
/// The placeholder text and the validation message both come from here.
pub(crate) const IMOTIONS_ADDR_HINT: &str = "<iMotions IP>:5050";

/// Bounds on the mirror destination that feeds the Python side (#245).
///
/// Both are short on purpose. The mirror is a copy of the stream, so a Python
/// process that is slow, wedged or gone must never hold up the 20 Hz feed to
/// iMotions. On a timeout the mirror is dropped and the iMotions stream carries
/// on untouched.
const MIRROR_CONNECT_TIMEOUT: StdDuration = StdDuration::from_millis(500);
const MIRROR_WRITE_TIMEOUT: StdDuration = StdDuration::from_millis(100);

// --- State definition -------------------------------------------------------
#[allow(unused)]
pub(crate) struct State {
    pub elapsed_time: Duration,
    pub event_log: VecDeque<String>,
    pub ipc_thread_handle: Option<JoinHandle<Result<()>>>,
    pub tcp_thread_handle: Option<JoinHandle<Result<()>>>,
    pub tcp_connected: bool,
    pub tcp_addr_field: String,
    pub latest_baton_send: Option<String>,
    pub active_baton_connection: bool,
    pub last_baton_instant: Option<Instant>,

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

    // Optional GUI error message and card state
    pub error_message: Option<String>,
    pub card_open: bool,

    // metrics/UI helpers
    pub packets_last_60s: usize,
    pub bps: f64,
    pub show_metrics: bool,

    // tcp packet counters
    pub sent_packet_times: std::collections::VecDeque<Instant>,
    pub sent_samples: std::collections::VecDeque<(Instant, usize)>,
    
    pub saved_tcp_addrs: Vec<String>,
    pub selected_tcp_addr: Option<String>,
    pub tcp_addr_validation_error: Option<String>,

    // Mirror destination: the Python side reads the same packets iMotions gets
    pub mirror_addr_field: String,
    pub mirror_connected: bool,
}

impl Default for State {
    fn default() -> State {
        State {
            elapsed_time: Duration::ZERO,
            event_log: VecDeque::new(),
            ipc_thread_handle: None,
            tcp_thread_handle: None,
            tcp_connected: false,
            tcp_addr_field: String::new(),
            latest_baton_send: None,
            active_baton_connection: false,
            last_baton_instant: None,

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

            error_message: None,
            card_open: false,

            packets_last_60s: 0,
            bps: 0.0,
            show_metrics: false,

            sent_packet_times: std::collections::VecDeque::new(),
            sent_samples: std::collections::VecDeque::new(),

            mirror_addr_field: String::new(),
            mirror_connected: false,
        }
    }
}

// --- helper functions -------------------------------------------------------
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

/// The iMotions packets one baton sample turns into, in send order.
///
/// Pulled out of the TCP thread so the mapping can be tested without a socket,
/// and so both destinations are written the same bytes from one list rather
/// than each building its own.
fn packets_for_fields(fields: &[String]) -> Vec<String> {
    let pair = |value: &String| vec![value.clone(), value.clone()];

    if fields.len() == 4 {
        let (alt, air, head, vv) = (&fields[0], &fields[1], &fields[2], &fields[3]);
        return vec![
            build_imotions_packet("AltitudeSync", &pair(alt)),
            build_imotions_packet("AirspeedSync", &pair(air)),
            build_imotions_packet("VerticalVelocitySync", &pair(vv)),
            build_imotions_packet("HeadingSync", &pair(head)),
        ];
    }

    if fields.len() == 8 {
        let (alt, air, head, vv) = (&fields[0], &fields[1], &fields[2], &fields[3]);
        let (roll, pitch, yaw, gforce) = (&fields[4], &fields[5], &fields[6], &fields[7]);
        return vec![
            build_imotions_packet("AltitudeSync", &pair(alt)),
            build_imotions_packet("AirspeedSync", &pair(air)),
            build_imotions_packet("VerticalVelocitySync", &pair(vv)),
            build_imotions_packet("HeadingSync", &pair(head)),
            build_imotions_packet("RollSync", &pair(roll)),
            build_imotions_packet("PitchSync", &pair(pitch)),
            build_imotions_packet("YawSync", &pair(yaw)),
            build_imotions_packet("GForceSync", &pair(gforce)),
        ];
    }

    // Anything else: take the fields two at a time, in sample order, and stop
    // when they run out.
    const PAIRED_SAMPLES: [&str; 8] = [
        "AltitudeSync",
        "AirspeedSync",
        "VerticalVelocitySync",
        "HeadingSync",
        "RollSync",
        "PitchSync",
        "YawSync",
        "GForceSync",
    ];
    let mut packets = Vec::new();
    let mut idx = 0usize;
    for name in PAIRED_SAMPLES {
        if idx + 1 >= fields.len() {
            break;
        }
        let payload = vec![fields[idx].clone(), fields[idx + 1].clone()];
        packets.push(build_imotions_packet(name, &payload));
        idx += 2;
    }
    packets
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

// Dummy packet for the Send Packet button. iMotions only accepts sample ids
// that appear in the generated iMotions.xml, so the sample has to be one of
// the enabled toggles. Returns None when every toggle is off.
pub(crate) fn build_test_packet(state: &State) -> Option<String> {
    let sample = test_packet_sample(state)?;
    Some(build_imotions_packet(
        sample,
        &["0".to_string(), "0".to_string()],
    ))
}

// The first enabled toggle, in the order create_xml_file writes the samples.
fn test_packet_sample(state: &State) -> Option<&'static str> {
    [
        (state.altitude_toggle, "AltitudeSync"),
        (state.airspeed_toggle, "AirspeedSync"),
        (state.vertical_airspeed_toggle, "VerticalVelocitySync"),
        (state.heading_toggle, "HeadingSync"),
        (state.roll_toggle, "RollSync"),
        (state.pitch_toggle, "PitchSync"),
        (state.yaw_toggle, "YawSync"),
        (state.gforce_toggle, "GForceSync"),
    ]
    .into_iter()
    .find_map(|(enabled, sample)| enabled.then_some(sample))
}

fn now_epoch_millis() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| StdDuration::from_secs(0));
    format!("{}.{:03}", now.as_secs(), now.subsec_millis())
} 

/// Wall-clock timestamp for entries shown in the GUI.
///
/// `now_epoch_millis` is kept for `human_log`, where epoch values are fine for a
/// machine-read stderr log; on screen they are unreadable. Milliseconds are kept
/// because the relay streams at 20 Hz and events can arrive several per second.
fn now_local_time() -> String {
    Local::now().format("%H:%M:%S%.3f").to_string()
}

// -- Buffered human logger --------------------------------------------------
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

fn human_log(level: &str, msg: &str) {
    let entry_body = format!("{} - {}", level, msg);
    let full_entry = format!("[{}] {}", now_epoch_millis(), entry_body);

    let mutex = init_logger();
    let now = Instant::now();

    {
        let mut st = mutex.lock().expect("logger mutex poisoned");

        if st.last_msg.as_deref() != Some(&entry_body) {
            st.buffer.push(full_entry);
            st.last_msg = Some(entry_body);
            drop(st);
            flush_logger();
            return;
        }

        if now.duration_since(st.last_flush) >= StdDuration::from_millis(LOG_FLUSH_INTERVAL_MS) {
            st.buffer.push(full_entry);
            drop(st);
            flush_logger();
            return;
        }
    }
}

/// Open the mirror connection, with a bounded wait so a wrong address cannot
/// stall the TCP thread while iMotions is streaming.
fn connect_mirror(address: &str) -> Result<TcpStream> {
    let addr: SocketAddr = address
        .parse()
        .map_err(|_| anyhow!("Invalid IP address. Use format: 127.0.0.1:5001"))?;
    let stream = TcpStream::connect_timeout(&addr, MIRROR_CONNECT_TIMEOUT)?;
    stream.set_write_timeout(Some(MIRROR_WRITE_TIMEOUT))?;
    stream.set_nodelay(true)?;
    Ok(stream)
}

/// Write one packet to the mirror, if it is connected.
///
/// Failure drops the mirror and is reported, never propagated: the iMotions
/// stream is the one that matters and must not be affected by the copy.
fn mirror_send(
    mirror: &mut Option<TcpStream>,
    packet: &str,
    child_bichannel: &mut ChildBiChannel<ToTcpThreadMessage, FromTcpThreadMessage>,
) {
    let Some(stream) = mirror.as_mut() else {
        return;
    };
    let write = stream
        .write_all(packet.as_bytes())
        .and_then(|()| stream.flush());
    if let Err(e) = write {
        let reason = format!("Python mirror dropped: {}", e);
        human_log("TCP", &reason);
        let _ = child_bichannel.send_to_parent(FromTcpThreadMessage::MirrorDisconnected(reason));
        *mirror = None;
    }
}

fn send_packet_and_debug(stream: &mut TcpStream, packet: &str) -> Result<()> {
    human_log("TX", &format!("packet len={} text={:?}", packet.len(), packet));
    let hex: String = packet
        .as_bytes()
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");
    human_log("TX", &format!("hex: {}", hex));
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
    pub fn log_event(&mut self, event: String) {
        let entry = format!("[{}] {}", now_local_time(), event);
        self.event_log.push_back(entry);
        while self.event_log.len() > EVENT_LOG_CAPACITY {
            self.event_log.pop_front();
        }
    }

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
            .map_err(|_| {
                anyhow!("Invalid address. Enter a literal IP and port, for example {IMOTIONS_ADDR_HINT}")
            })?;
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

    pub fn refresh_metrics_now(&mut self) {
        let now = Instant::now();
        self.refresh_metrics(now);
    }

    pub fn on_tcp_packet_sent(&mut self, bytes: usize) {
        let now = Instant::now();
        self.sent_packet_times.push_back(now);
        self.sent_samples.push_back((now, bytes));
        self.refresh_metrics(now);
    }

    fn refresh_metrics(&mut self, now: Instant) {
        let window60 = StdDuration::from_secs(60);
        while let Some(&t) = self.sent_packet_times.front() {
            if now.duration_since(t) > window60 {
                self.sent_packet_times.pop_front();
            } else {
                break;
            }
        }
        self.packets_last_60s = self.sent_packet_times.len();

        let window1 = StdDuration::from_secs(1);
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

    pub fn ipc_connect(&mut self) -> Result<()> {
        if self.ipc_thread_handle.is_some() {
            bail!("IPC thread already exists.")
        }
        let (ipc_bichannel, mut child_bichannel) =
            bichannel::create_bichannels::<ToIpcThreadMessage, FromIpcThreadMessage>();
        let ipc_thread_handle = spawn(move || {
            let printname = "baton.sock";
            let name = if GenericNamespaced::is_supported() {
                printname.to_ns_name::<GenericNamespaced>().unwrap()
            } else {
                let mut path = std::env::temp_dir();
                path.push(printname);
                path.to_fs_name::<GenericFilePath>().unwrap()
            };

            println!("[RELAY] listening on socket: {:?}", name.borrow());
            
            let opts = ListenerOptions::new().name(name.clone());
            let listener = match opts.create_sync() {
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    human_log("IPC", &format!(
                        "Could not start server because the socket file is occupied. Check if {} is in use.",
                        printname
                    ));
                    return Ok(());
                }
                Ok(l) => {
                    println!("[OK] Successfully created named pipe listener");
                    l
                }
                Err(e) => {
                    eprintln!("[ERROR] Failed to create listener: {} (kind: {:?})", e, e.kind());
                    return Err(anyhow!("Failed to create listener: {}", e));
                }
            };
            
            listener
                .set_nonblocking(interprocess::local_socket::ListenerNonblockingMode::Both)
                .expect("Error setting non-blocking mode on listener");
            
            human_log("IPC", &format!("Server running at {}", printname));
            let mut buffer = String::with_capacity(256);
            
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
                
                // read initial/greeting line if present
                let mut greeting_attempts = 0;
                loop {
                    match conn.read_line(&mut buffer) {
                        Ok(n) if n > 0 => {
                            println!("[RELAY] Received greeting: {}", buffer.trim());
                            break;
                        }
                        Ok(_) => break,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            greeting_attempts += 1;
                            if greeting_attempts > 100 {
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
                
                // Send greeting response
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

                // Continuously receive data from plugin
                while !child_bichannel.is_killswitch_engaged() {
                    for message in child_bichannel.received_messages() {
                        match message {}
                    }
                    
                    match conn.read_line(&mut buffer) {
                        Ok(0) => {
                            std::thread::sleep(StdDuration::from_millis(1));
                            continue;
                        }
                        Ok(_s) => {
                            human_log("RX", &format!("baton line={:?}", buffer));
                            let _ = buffer.pop();
                            let _ = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonData(buffer.clone()));
                            buffer.clear();
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(StdDuration::from_millis(1));
                            continue;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
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
            let mut stream = match TcpStream::connect(address) {
                Ok(stream) => {
                    human_log("TCP", "Successfully connected to iMotions server.");
                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                    stream
                }
                Err(e) => {
                    let reason = format!("TCP connection failed: {}", e);
                    human_log("TCP", &reason);
                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                    // Report it so the GUI can show the failure and drop this
                    // thread, instead of the error only reaching stderr.
                    let _ = child_bichannel
                        .send_to_parent(FromTcpThreadMessage::Disconnected(reason.clone()));
                    bail!(reason);
                }
            };

            // Second destination for the same packets, set by SetMirror. Lives
            // in this thread so both sinks are written from one packet list.
            let mut mirror: Option<TcpStream> = None;

            while !child_bichannel.is_killswitch_engaged() {
                for message in child_bichannel.received_messages() {
                    match message {
                        ToTcpThreadMessage::Send(data) => {
                            let fields = normalize_baton_payload(&data);

                            if fields.len() < 2 {
                                human_log("TCP", &format!(
                                    "Dropping packet: not enough fields (need >=2) but baton sent {}: {:?}",
                                    fields.len(), fields
                                ));
                                continue;
                            }

                            for packet in packets_for_fields(&fields) {
                                let res = send_packet_and_debug(&mut stream, &packet);
                                let _ = child_bichannel.set_is_conn_to_endpoint(res.is_ok());
                                mirror_send(&mut mirror, &packet, &mut child_bichannel);
                            }
                        }
                        ToTcpThreadMessage::SendRaw(packet) => {
                            let res = send_packet_and_debug(&mut stream, &packet);
                            let _ = child_bichannel.set_is_conn_to_endpoint(res.is_ok());
                            if let Err(e) = res {
                                human_log("TCP", &format!("Test packet failed: {}", e));
                            }
                            // The mirror gets the test packet too, so the button
                            // checks both links at once.
                            mirror_send(&mut mirror, &packet, &mut child_bichannel);
                        }
                        ToTcpThreadMessage::SetMirror(target) => {
                            mirror = None;
                            match target {
                                None => {
                                    let _ = child_bichannel.send_to_parent(
                                        FromTcpThreadMessage::MirrorDisconnected(
                                            "closed".to_string(),
                                        ),
                                    );
                                }
                                Some(address) => match connect_mirror(&address) {
                                    Ok(stream) => {
                                        human_log(
                                            "TCP",
                                            &format!("Python mirror connected to {}", address),
                                        );
                                        mirror = Some(stream);
                                        let _ = child_bichannel.send_to_parent(
                                            FromTcpThreadMessage::MirrorConnected(address),
                                        );
                                    }
                                    Err(e) => {
                                        let reason =
                                            format!("Python mirror connection failed: {}", e);
                                        human_log("TCP", &reason);
                                        let _ = child_bichannel.send_to_parent(
                                            FromTcpThreadMessage::MirrorDisconnected(reason),
                                        );
                                    }
                                },
                            }
                        }
                    }
                }
                std::thread::sleep(StdDuration::from_millis(1));
            }
            Ok(())
        });
        
        self.tcp_thread_handle = Some(tcp_thread_handle);
        // Clear any banner left by an earlier failure, so a successful retry
        // does not keep showing the old error.
        self.error_message = None;
        Ok(())
    }

    /// Point the mirror at `address`, so the Python side receives a copy of
    /// every packet (#245).
    ///
    /// The mirror lives inside the TCP thread, so iMotions has to be connected
    /// first. With no iMotions to talk to, point Connect TCP straight at the
    /// Python listener instead and leave the mirror unused.
    pub fn mirror_connect(&mut self, address: String) -> Result<()> {
        Self::validate_tcp_addr(&address)?;
        // validate_tcp_addr trims before parsing, so trim here too. Otherwise a
        // pasted address with a stray space passes validation and then fails to
        // parse inside the thread.
        let address = address.trim().to_string();
        let Some(bichannel) = self.tcp_bichannel.as_mut() else {
            bail!("Connect TCP first. The mirror copies that stream.")
        };
        bichannel.send_to_child(ToTcpThreadMessage::SetMirror(Some(address)))
    }

    pub fn mirror_disconnect(&mut self) -> Result<()> {
        let Some(bichannel) = self.tcp_bichannel.as_mut() else {
            self.mirror_connected = false;
            return Ok(());
        };
        bichannel.send_to_child(ToTcpThreadMessage::SetMirror(None))
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
        // The mirror lives in that thread, so it goes down with it. Without
        // this the GUI keeps claiming the Python stream is connected.
        self.mirror_connected = false;
        
        if !was_connected {
            spawn(move || {
                let _ = handle.join();
            });
            return Ok(());
        }
        
        let res = handle.join().map_err(|e| anyhow!("Join handle err: {e:?}"))?;
        Ok(res?)
    }

    // Drops a TCP thread that has already exited, so tcp_connect is allowed
    // to start a new one. The join runs detached because the thread returned
    // an error and its Result is of no use here.
    pub fn reap_tcp_thread(&mut self) {
        if let Some(handle) = self.tcp_thread_handle.take() {
            spawn(move || {
                let _ = handle.join();
            });
        }
        self.tcp_bichannel = None;
        self.tcp_connected = false;
        // The mirror lived in that thread, so it is gone with it.
        self.mirror_connected = false;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_log_stops_growing_at_the_cap() {
        let mut state = State::default();
        for i in 0..(EVENT_LOG_CAPACITY + 50) {
            state.log_event(format!("event {i}"));
        }

        assert_eq!(state.event_log.len(), EVENT_LOG_CAPACITY);
        // The newest entries are the ones worth keeping.
        assert!(state.event_log.back().unwrap().contains(&format!(
            "event {}",
            EVENT_LOG_CAPACITY + 49
        )));
        assert!(state.event_log.front().unwrap().contains("event 50"));
        assert!(!state.event_log.iter().any(|e| e.ends_with("event 0")));
    }

    fn fields(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn eight_fields_become_one_packet_per_sample_in_order() {
        let packets = packets_for_fields(&fields(&[
            "1000", "120", "90", "500", "2", "3", "91", "1.0",
        ]));

        let names: Vec<&str> = packets
            .iter()
            .map(|p| p.split(';').nth(7).expect("no sample name"))
            .collect();
        assert_eq!(
            names,
            [
                "AltitudeSync",
                "AirspeedSync",
                "VerticalVelocitySync",
                "HeadingSync",
                "RollSync",
                "PitchSync",
                "YawSync",
                "GForceSync",
            ]
        );
        // Heading is field 2 and vertical velocity field 3, but they are sent
        // the other way round. Pin that, it is easy to "tidy" and break.
        assert_eq!(packets[2], "E;1;PilotDataSync;;;;;VerticalVelocitySync;500;500\r\n");
        assert_eq!(packets[3], "E;1;PilotDataSync;;;;;HeadingSync;90;90\r\n");
    }

    #[test]
    fn four_fields_send_only_the_flight_dynamics_factors() {
        let packets = packets_for_fields(&fields(&["1000", "120", "90", "500"]));
        assert_eq!(packets.len(), 4);
        assert_eq!(packets[0], "E;1;PilotDataSync;;;;;AltitudeSync;1000;1000\r\n");
    }

    #[test]
    fn an_odd_field_count_pairs_what_it_can_and_drops_the_remainder() {
        let packets = packets_for_fields(&fields(&["1000", "120", "90"]));
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0], "E;1;PilotDataSync;;;;;AltitudeSync;1000;120\r\n");
    }

    #[test]
    fn test_packet_matches_imotions_format() {
        let state = State::default();
        assert_eq!(
            build_test_packet(&state).as_deref(),
            Some("E;1;PilotDataSync;;;;;AltitudeSync;0;0\r\n")
        );
    }

    #[test]
    fn test_packet_uses_the_first_enabled_toggle() {
        let mut state = State::default();
        state.altitude_toggle = false;
        state.airspeed_toggle = false;

        assert_eq!(
            build_test_packet(&state).as_deref(),
            Some("E;1;PilotDataSync;;;;;VerticalVelocitySync;0;0\r\n")
        );
    }

    #[test]
    fn test_packet_is_none_when_every_toggle_is_off() {
        let mut state = State::default();
        state.altitude_toggle = false;
        state.airspeed_toggle = false;
        state.vertical_airspeed_toggle = false;
        state.heading_toggle = false;
        state.roll_toggle = false;
        state.pitch_toggle = false;
        state.yaw_toggle = false;
        state.gforce_toggle = false;

        assert_eq!(build_test_packet(&state), None);
    }
}
