use anyhow::{anyhow, bail, Result};
use std::collections::BTreeSet;
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

// --- State definition -------------------------------------------------------
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

fn now_epoch_millis() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| StdDuration::from_secs(0));
    format!("{}.{:03}", now.as_secs(), now.subsec_millis())
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
        let entry = format!("[{}] {}", now_epoch_millis(), event);
        self.event_log.push(entry);
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

    pub fn refresh_metrics_now(&mut self) {
        let now = Instant::now();
        self.refresh_metrics(now);
    }

    pub fn on_tcp_packet_sent(&mut self, bytes: usize) {
        let now = Instant::now();
        self.sent_packet_times.push_back(now);
        self.sent_samples.push_back((now, bytes));
        self.refresh_metrics(now);
        self.log_event(format!("Sent packet ({} bytes)", bytes));
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
                    human_log("TCP", &format!("Connection failed: {}", e));
                    bail!("Failed to connect to TCP");
                }
            };
            
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

                            let mut try_send = |pkt: String| -> Result<()> {
                                let res = send_packet_and_debug(&mut stream, &pkt);
                                if res.is_err() {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(false);
                                } else {
                                    let _ = child_bichannel.set_is_conn_to_endpoint(true);
                                }
                                res
                            };

                            if fields.len() == 4 {
                                let alt = fields[0].clone();
                                let air = fields[1].clone();
                                let head = fields[2].clone();
                                let vv = fields[3].clone();

                                let _ = try_send(build_imotions_packet("AltitudeSync", &[alt.clone(), alt.clone()]));
                                let _ = try_send(build_imotions_packet("AirspeedSync", &[air.clone(), air.clone()]));
                                let _ = try_send(build_imotions_packet("VerticalVelocitySync", &[vv.clone(), vv.clone()]));
                                let _ = try_send(build_imotions_packet("HeadingSync", &[head.clone(), head.clone()]));
                                continue;
                            }

                            if fields.len() == 8 {
                                let alt = fields[0].clone();
                                let air = fields[1].clone();
                                let head = fields[2].clone();
                                let vv = fields[3].clone();
                                let roll = fields[4].clone();
                                let pitch = fields[5].clone();
                                let yaw = fields[6].clone();
                                let gforce = fields[7].clone();

                                let _ = try_send(build_imotions_packet("AltitudeSync", &[alt.clone(), alt.clone()]));
                                let _ = try_send(build_imotions_packet("AirspeedSync", &[air.clone(), air.clone()]));
                                let _ = try_send(build_imotions_packet("VerticalVelocitySync", &[vv.clone(), vv.clone()]));
                                let _ = try_send(build_imotions_packet("HeadingSync", &[head.clone(), head.clone()]));
                                let _ = try_send(build_imotions_packet("RollSync", &[roll.clone(), roll.clone()]));
                                let _ = try_send(build_imotions_packet("PitchSync", &[pitch.clone(), pitch.clone()]));
                                let _ = try_send(build_imotions_packet("YawSync", &[yaw.clone(), yaw.clone()]));
                                let _ = try_send(build_imotions_packet("GForceSync", &[gforce.clone(), gforce.clone()]));
                                continue;
                            }

                            // Paired mapping
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
                        }
                    }
                }
                std::thread::sleep(StdDuration::from_millis(1));
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
        
        let res = handle.join().map_err(|e| anyhow!("Join handle err: {e:?}"))?;
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
}
