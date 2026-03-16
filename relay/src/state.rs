use anyhow::Result;
use anyhow::{anyhow, bail};
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::net::TcpStream;
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

// --- State impl -------------------------------------------------------------
impl State {
    // Timestamped event logger used by the UI/event history
    pub fn log_event(&mut self, event: String) {
        let entry = format!("[{}] {}", chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true), event);
        self.event_log.push(entry);
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
            while !child_bichannel.is_killswitch_engaged() {
                let conn = listener.accept();
                let conn = match (child_bichannel.is_killswitch_engaged(), conn) {
                    (true, _) => return Ok(()),
                    (_, Ok(c)) => {
                        println!("[RELAY] Accepted incoming IPC connection");
                        c
                    }
                    (_, Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    (_, Err(e)) => {
                        eprintln!("Incoming connection failed: {e}");
                        continue;
                    }
                };
                let mut conn = BufReader::new(conn);
                child_bichannel.set_is_conn_to_endpoint(true)?;
                // read initial/greeting line if present
                match conn.read_line(&mut buffer) {
                    Ok(_) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                    Err(e) => eprintln!("Initial read error: {e}"),
                }
                let _ = conn.get_mut().write_all(b"Hello, from the relay (Rust)!\n");
                buffer.clear();

                while !child_bichannel.is_killswitch_engaged() {
                    for message in child_bichannel.received_messages() {
                        match message {}
                    }
                    match conn.read_line(&mut buffer) {
                        Ok(0) => {
                            // remote closed
                            let _ = child_bichannel.send_to_parent(FromIpcThreadMessage::BatonShutdown);
                            let _ = child_bichannel.set_is_conn_to_endpoint(false);
                            buffer.clear();
                            break;
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
        
        // Clone toggle states to pass to the thread
        let altitude_enabled = self.altitude_toggle;
        let airspeed_enabled = self.airspeed_toggle;
        let vertical_velocity_enabled = self.vertical_airspeed_toggle;
        let heading_enabled = self.heading_toggle;
        let roll_enabled = self.roll_toggle;
        let pitch_enabled = self.pitch_toggle;
        let yaw_enabled = self.yaw_toggle;
        let gforce_enabled = self.gforce_toggle;
        
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

                                    // Helper to send only if toggle is enabled
                                    let mut try_send_if_enabled = |enabled: bool, name: &str, data: &[String]| -> Result<()> {
                                        if enabled {
                                            let pkt = build_imotions_packet(name, data);
                                            try_send(pkt)?;
                                        }
                                        Ok(())
                                    };

                                    // If exactly 4 fields -> legacy pilot-only
                                    if fields.len() == 4 {
                                        let alt = fields[0].clone();
                                        let air = fields[1].clone();
                                        let head = fields[2].clone();
                                        let vv = fields[3].clone();

                                        let _ = try_send_if_enabled(altitude_enabled, "AltitudeSync", &[alt.clone(), alt.clone()]);
                                        let _ = try_send_if_enabled(airspeed_enabled, "AirspeedSync", &[air.clone(), air.clone()]);
                                        let _ = try_send_if_enabled(vertical_velocity_enabled, "VerticalVelocitySync", &[vv.clone(), vv.clone()]);
                                        let _ = try_send_if_enabled(heading_enabled, "HeadingSync", &[head.clone(), head.clone()]);
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

                                        let _ = try_send_if_enabled(altitude_enabled, "AltitudeSync", &[alt.clone(), alt.clone()]);
                                        let _ = try_send_if_enabled(airspeed_enabled, "AirspeedSync", &[air.clone(), air.clone()]);
                                        let _ = try_send_if_enabled(vertical_velocity_enabled, "VerticalVelocitySync", &[vv.clone(), vv.clone()]);
                                        let _ = try_send_if_enabled(heading_enabled, "HeadingSync", &[head.clone(), head.clone()]);
                                        let _ = try_send_if_enabled(roll_enabled, "RollSync", &[roll.clone(), roll.clone()]);
                                        let _ = try_send_if_enabled(pitch_enabled, "PitchSync", &[pitch.clone(), pitch.clone()]);
                                        let _ = try_send_if_enabled(yaw_enabled, "YawSync", &[yaw.clone(), yaw.clone()]);
                                        let _ = try_send_if_enabled(gforce_enabled, "GForceSync", &[gforce.clone(), gforce.clone()]);
                                        continue;
                                    }

                                    // Otherwise attempt paired mapping: assume sequence of pairs FM,Pilot
                                    let mut idx = 0usize;
                                    let mut send_pair_if_enabled = |enabled: bool, name: &str, idx: &mut usize| -> Result<()> {
                                        if *idx + 1 < fields.len() && enabled {
                                            let payload = vec![fields[*idx].clone(), fields[*idx + 1].clone()];
                                            let pkt = build_imotions_packet(name, &payload);
                                            *idx += 2;
                                            try_send(pkt)?;
                                        } else if *idx + 1 < fields.len() {
                                            // Skip the pair even if disabled
                                            *idx += 2;
                                        }
                                        Ok(())
                                    };

                                    let _ = send_pair_if_enabled(altitude_enabled, "AltitudeSync", &mut idx);
                                    let _ = send_pair_if_enabled(airspeed_enabled, "AirspeedSync", &mut idx);
                                    let _ = send_pair_if_enabled(vertical_velocity_enabled, "VerticalVelocitySync", &mut idx);
                                    let _ = send_pair_if_enabled(heading_enabled, "HeadingSync", &mut idx);
                                    let _ = send_pair_if_enabled(roll_enabled, "RollSync", &mut idx);
                                    let _ = send_pair_if_enabled(pitch_enabled, "PitchSync", &mut idx);
                                    let _ = send_pair_if_enabled(yaw_enabled, "YawSync", &mut idx);
                                    let _ = send_pair_if_enabled(gforce_enabled, "GForceSync", &mut idx);
                                }
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
            }
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