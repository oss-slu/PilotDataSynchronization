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
            
             let opts = ListenerOptions::new().name(name);
            let listener = match opts.create_sync() {
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    eprintln!("Error: socket file occupied: {}", printname);
                    return Ok(());
                }
                x => x.unwrap(),
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