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

/// What to type into the mirror's address box. Unlike the iMotions hint this is
/// a working example, because the Python listener usually runs on the same
/// machine as the relay, and 5001 is the capture logger's default port.
pub(crate) const MIRROR_ADDR_HINT: &str = "127.0.0.1:5001";

/// Bounds on the mirror destination that feeds the Python side (#245).
///
/// Connecting and writing both run on the mirror's own thread, so these never
/// cost the iMotions stream anything. They exist so a Python process that is
/// gone or wedged is given up on rather than held open forever.
const MIRROR_CONNECT_TIMEOUT: StdDuration = StdDuration::from_millis(500);
const MIRROR_WRITE_TIMEOUT: StdDuration = StdDuration::from_millis(100);

/// How many packets may wait for the mirror's writer thread.
///
/// The stream is 8 packets per sample at 20 Hz, so 160 a second. This holds
/// about a second and a half, enough to ride out a clustering refit without
/// losing anything. Past that the oldest behaviour is to drop new packets
/// rather than make iMotions wait.
const MIRROR_QUEUE_CAPACITY: usize = 256;

/// How much the kernel may hold on the mirror socket on our behalf.
///
/// `MIRROR_QUEUE_CAPACITY` bounds only the queue in front of the writer
/// thread. Behind it the socket's own send buffer is a second queue, and the
/// Linux default is about 2.5 MB. Measured on loopback, that and the reader's
/// buffer together swallow some 37,000 packets -- close to four minutes of
/// stream -- before a write blocks, so a mirror that stopped reading minutes
/// ago still takes writes, `MIRROR_WRITE_TIMEOUT` never fires, nothing is
/// reported, and the bound in front of the writer is decoration.
///
/// Asking for 32 KiB brings that down to about 3,200 packets, or twenty
/// seconds. Linux doubles the request, so the buffer is really 64 KiB; what
/// matters is that it is tens of kilobytes rather than megabytes. It costs no
/// throughput: 50,000 packets to a reader that keeps up take the same 45 ms
/// either way.
const MIRROR_SEND_BUFFER: usize = 32 * 1024;

/// How often an armed mirror tries again.
///
/// The mirror is a TCP client and the listener binds the port, so the mirror
/// can only connect once the listener is up. Longer than
/// `MIRROR_CONNECT_TIMEOUT`, so each attempt gets its full chance before the
/// next one replaces it.
const MIRROR_ARM_RETRY: StdDuration = StdDuration::from_millis(600);

/// When an armed mirror says it is taking a while.
///
/// It keeps trying afterwards. There is no deadline, because the thing it is
/// waiting for is the listener, and whether that has given up is already known
/// from whether it is still running. A first `uv run` may spend minutes
/// building the Python environment, and a window that stopped trying at some
/// arbitrary point would leave a listener running with nothing being sent to
/// it, which is the worst of the three outcomes.
const MIRROR_ARM_SLOW: StdDuration = StdDuration::from_secs(20);

/// A mirror connection the window is still trying to make on its own.
///
/// Connecting the mirror by hand meant typing an address and knowing it had to
/// happen after the listener started and before the flight did. None of that
/// is the operator's problem, so the window carries it (#285).
pub(crate) struct MirrorArm {
    pub(crate) address: String,
    armed_at: Instant,
    last_try: Option<Instant>,
    said_it_was_slow: bool,
}

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
    /// Set while the window is connecting the mirror by itself.
    pub mirror_arm: Option<MirrorArm>,

    // The pilot_inference listener the relay runs for the operator (#285), so
    // a Flight needs no terminal. The relay outlives it (docs/adr/0005).
    pub listener: crate::listener::Listener,
    pub listener_mode: crate::listener::ListenerMode,
    pub pilot_id_field: String,
    pub flight_id_field: String,
    pub brief_field: String,
    pub inference_dir: Option<std::path::PathBuf>,

    /// `uv sync`, run from the window, for a tree whose Python environment has
    /// not been built yet. The same machinery as a listener, so its output
    /// shows in the same place.
    pub setup: crate::listener::Listener,

    /// Whether `uv` is on PATH. `None` until something has looked.
    ///
    /// Checked once at startup rather than discovered when a listener fails to
    /// spawn, because the fix is an install and that is not a thing to find out
    /// with a pilot already in the seat.
    pub uv_available: Option<bool>,

    /// A recorded flight being sent in place of X-Plane (#285).
    pub replay: Option<crate::replay::Replay>,
    pub recordings: Vec<crate::replay::Recording>,
    pub selected_recording: Option<String>,
    pub selected_replay_flight: Option<String>,

    /// A local stand-in for iMotions, so the chain can be tried at a desk.
    pub test_receiver: Option<crate::receiver::TestReceiver>,

    /// Whether the window shows the individual connect and disconnect
    /// controls. Off by default: they are recovery, and the people running a
    /// session should see the flight, not the plumbing.
    pub show_advanced: bool,
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
            mirror_arm: None,

            listener: crate::listener::Listener::default(),
            listener_mode: crate::listener::ListenerMode::Capture,
            pilot_id_field: String::new(),
            flight_id_field: String::new(),
            // A brief that exists in the repo, so Live groups can be tried
            // without hunting for a file first.
            brief_field: "briefs/example_level_holds.csv".to_string(),
            inference_dir: crate::listener::find_inference_dir(),

            setup: crate::listener::Listener::default(),

            // Left unchecked here. `Default` is what the tests build, and they
            // should not each spawn a process to ask about PATH.
            uv_available: None,

            replay: None,
            recordings: Vec::new(),
            selected_recording: None,
            selected_replay_flight: None,
            test_receiver: None,
            show_advanced: false,
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

/// One iMotions entry per flight value, in the order the relay sends them.
///
/// "Event" here is one iMotions entry, not a Sample in the GLOSSARY sense,
/// which is a whole set of telemetry values.
const IMOTIONS_EVENTS: [&str; 8] = [
    "AltitudeSync",
    "AirspeedSync",
    "VerticalVelocitySync",
    "HeadingSync",
    "RollSync",
    "PitchSync",
    "YawSync",
    "GForceSync",
];

/// The iMotions packets one baton sample turns into, in send order.
///
/// Pulled out of the TCP thread so the mapping can be tested without a socket,
/// and so both destinations are written the same bytes from one list rather
/// than each building its own.
///
/// Every iMotions entry carries two values, `FlightModel*` then `Pilot*`. A
/// 16-field sample has both, paired. The 4- and 8-field shapes are what older
/// plugins send: they carry the pilot half only, so the flightmodel half is
/// filled with the same number and the two cannot be compared (#184).
fn packets_for_fields(fields: &[String]) -> Vec<String> {
    // Legacy duplication: one value standing in for both halves. Only for the
    // field counts that predate the paired payload.
    let pair = |value: &String| vec![value.clone(), value.clone()];

    // The paired payload: both halves of every value, flightmodel first, in
    // iMotions event order. Spelled out rather than left to the generic branch
    // at the end so a test pins the order and the halves cannot quietly swap.
    if fields.len() == IMOTIONS_EVENTS.len() * 2 {
        return IMOTIONS_EVENTS
            .iter()
            .enumerate()
            .map(|(i, name)| {
                build_imotions_packet(name, &[fields[i * 2].clone(), fields[i * 2 + 1].clone()])
            })
            .collect();
    }

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

    // Anything else: take the fields two at a time, in event order, and stop
    // when they run out.
    let mut packets = Vec::new();
    let mut idx = 0usize;
    for name in IMOTIONS_EVENTS {
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

/// What the mirror's writer thread reports back to the TCP thread.
enum MirrorEvent {
    Connected(String),
    Disconnected(String),
}

/// The TCP thread's handle on the mirror: a queue into the writer thread, and
/// the writer thread's reports coming back.
struct Mirror {
    packets: crossbeam::channel::Sender<String>,
    events: crossbeam::channel::Receiver<MirrorEvent>,
    dropped: usize,
    /// Whether we are in a run of drops, so the log gets one line when the
    /// mirror falls behind and one when it recovers, not one per packet.
    lagging: bool,
}

/// Start the mirror's writer thread and hand back a queue into it.
///
/// Connecting and writing both happen on that thread, so neither can delay the
/// iMotions stream. The TCP thread only ever pushes into a bounded queue, which
/// either takes the packet immediately or refuses it.
fn spawn_mirror(address: String) -> Mirror {
    let (packets_tx, packets_rx) = crossbeam::channel::bounded::<String>(MIRROR_QUEUE_CAPACITY);
    let (events_tx, events_rx) = crossbeam::channel::unbounded();

    spawn(move || {
        let mut stream = match connect_mirror(&address) {
            Ok(stream) => {
                human_log("TCP", &format!("Python mirror connected to {}", address));
                let _ = events_tx.send(MirrorEvent::Connected(address.clone()));
                stream
            }
            Err(e) => {
                let reason = format!("Python mirror connection failed: {}", e);
                human_log("TCP", &reason);
                let _ = events_tx.send(MirrorEvent::Disconnected(reason));
                return;
            }
        };

        // Ends when the TCP thread drops its sender, which is how the mirror is
        // closed and how it goes away with the connection it belongs to.
        for packet in packets_rx {
            let write = stream
                .write_all(packet.as_bytes())
                .and_then(|()| stream.flush());
            if let Err(e) = write {
                let reason = format!("Python mirror dropped: {}", e);
                human_log("TCP", &reason);
                let _ = events_tx.send(MirrorEvent::Disconnected(reason));
                return;
            }
        }
    });

    Mirror {
        packets: packets_tx,
        events: events_rx,
        dropped: 0,
        lagging: false,
    }
}

/// Open the mirror connection. Runs on the writer thread, so the timeout here
/// costs the iMotions stream nothing.
fn connect_mirror(address: &str) -> Result<TcpStream> {
    let addr: SocketAddr = address
        .parse()
        .map_err(|_| anyhow!("Mirror address is not a literal IP and port: {address}"))?;
    let stream = TcpStream::connect_timeout(&addr, MIRROR_CONNECT_TIMEOUT)?;
    stream.set_write_timeout(Some(MIRROR_WRITE_TIMEOUT))?;
    stream.set_nodelay(true)?;
    // Without this the write timeout above is unreachable in practice: see
    // MIRROR_SEND_BUFFER.
    socket2::SockRef::from(&stream).set_send_buffer_size(MIRROR_SEND_BUFFER)?;
    Ok(stream)
}

/// Hand one packet to the mirror's writer thread, if there is one.
///
/// Never waits. A full queue means the reader is behind, so the packet is
/// dropped and counted. Losing copies is the right trade: the mirror is for
/// analysis that can tolerate gaps, and the iMotions recording cannot be redone.
fn mirror_send(
    mirror: &mut Option<Mirror>,
    packet: &str,
    child_bichannel: &mut ChildBiChannel<ToTcpThreadMessage, FromTcpThreadMessage>,
) {
    let Some(active) = mirror.as_mut() else {
        return;
    };
    match active.packets.try_send(packet.to_string()) {
        Ok(()) => {
            if active.lagging {
                let note = format!(
                    "Python mirror caught up, {} copies dropped while it was behind",
                    active.dropped
                );
                human_log("TCP", &note);
                let _ = child_bichannel.send_to_parent(FromTcpThreadMessage::MirrorLagging(note));
                active.lagging = false;
                active.dropped = 0;
            }
        }
        Err(_) => {
            active.dropped += 1;
            // One line per episode. At 160 packets a second, reporting each
            // drop would push everything else out of the capped event log.
            if !active.lagging {
                active.lagging = true;
                let note =
                    "Python mirror is behind, dropping copies. iMotions is unaffected".to_string();
                human_log("TCP", &note);
                let _ = child_bichannel.send_to_parent(FromTcpThreadMessage::MirrorLagging(note));
            }
        }
    }
}

/// Forward anything the writer thread has reported, and clear the mirror if it
/// has gone away.
fn drain_mirror_events(
    mirror: &mut Option<Mirror>,
    child_bichannel: &mut ChildBiChannel<ToTcpThreadMessage, FromTcpThreadMessage>,
) {
    let events: Vec<MirrorEvent> = match mirror.as_ref() {
        Some(active) => active.events.try_iter().collect(),
        None => return,
    };
    for event in events {
        match event {
            MirrorEvent::Connected(address) => {
                let _ = child_bichannel.send_to_parent(FromTcpThreadMessage::MirrorConnected(address));
            }
            MirrorEvent::Disconnected(reason) => {
                let _ = child_bichannel
                    .send_to_parent(FromTcpThreadMessage::MirrorDisconnected(reason));
                *mirror = None;
                return;
            }
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
            if Self::validate_tcp_addr(candidate, IMOTIONS_ADDR_HINT).is_ok() {
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

    /// `example` is the address hint for whichever box is being validated, so
    /// the mirror does not tell people to type an iMotions address.
    pub fn validate_tcp_addr(address: &str, example: &str) -> Result<()> {
        let _parsed = address
            .trim()
            .parse::<SocketAddr>()
            .map_err(|_| {
                anyhow!("Invalid address. Enter a literal IP and port, for example {example}")
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
        // Bind the listener on the calling thread. Doing it inside the spawned
        // thread meant a bind failure had no way back to the caller: the thread
        // reported `Ok(())`, `ipc_thread_handle` was set for a thread that owned
        // no listener, and the guard above then rejected every later retry.
        let printname = "baton.sock";
        let name = if GenericNamespaced::is_supported() {
            printname.to_ns_name::<GenericNamespaced>()?
        } else {
            let mut path = std::env::temp_dir();
            path.push(printname);
            path.to_fs_name::<GenericFilePath>()?
        };

        println!("[RELAY] listening on socket: {:?}", name.borrow());

        let listener = ListenerOptions::new()
            .name(name)
            .create_sync()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::AddrInUse => anyhow!(
                    "{} is already in use. Another relay instance may still be running.",
                    printname
                ),
                _ => anyhow!("Failed to create listener: {}", e),
            })?;

        listener
            .set_nonblocking(interprocess::local_socket::ListenerNonblockingMode::Both)
            .map_err(|e| anyhow!("Error setting non-blocking mode on listener: {}", e))?;

        human_log("IPC", &format!("Server running at {}", printname));

        let (ipc_bichannel, mut child_bichannel) =
            bichannel::create_bichannels::<ToIpcThreadMessage, FromIpcThreadMessage>();
        let ipc_thread_handle = spawn(move || {
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

            // Second destination for the same packets, set by SetMirror. Its
            // socket is owned by a writer thread; this is only the queue into
            // it, so nothing here can be slowed by the Python side.
            let mut mirror: Option<Mirror> = None;

            while !child_bichannel.is_killswitch_engaged() {
                drain_mirror_events(&mut mirror, &mut child_bichannel);
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
                            // Dropping the old handle closes its queue, which
                            // ends that writer thread and its socket with it.
                            mirror = None;
                            match target {
                                None => {
                                    let _ = child_bichannel.send_to_parent(
                                        FromTcpThreadMessage::MirrorDisconnected(
                                            "closed".to_string(),
                                        ),
                                    );
                                }
                                // Connecting happens on the writer thread, so
                                // this returns immediately and the result
                                // arrives as a MirrorEvent.
                                Some(address) => mirror = Some(spawn_mirror(address)),
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
        Self::validate_tcp_addr(&address, MIRROR_ADDR_HINT)?;
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
        self.mirror_arm = None;
        // Cleared here rather than when the thread confirms. Otherwise the
        // window goes on saying the listener is recording for a tick after the
        // flight was stopped, which is the one moment someone is looking at it
        // to check that it has.
        self.mirror_connected = false;
        let Some(bichannel) = self.tcp_bichannel.as_mut() else {
            return Ok(());
        };
        bichannel.send_to_child(ToTcpThreadMessage::SetMirror(None))
    }

    /// Start connecting the mirror, and keep trying until it lands.
    ///
    /// The first attempt usually fails, and that is expected rather than an
    /// error: the listener is still starting Python and has not bound the port.
    /// Treating the first refusal as a failure is exactly what made this a
    /// manual step with an order to remember.
    pub fn arm_mirror(&mut self, address: String, now: Instant) {
        self.mirror_addr_field = address.clone();
        self.mirror_arm = Some(MirrorArm {
            address,
            armed_at: now,
            last_try: None,
            said_it_was_slow: false,
        });
    }

    /// Carry on connecting an armed mirror. Returns a line for the event log
    /// when there is something worth saying, which is not on every attempt.
    pub fn poll_mirror_arm(&mut self, now: Instant) -> Option<String> {
        let mut arm = self.mirror_arm.take()?;

        if self.mirror_connected {
            return Some(format!("The listener is receiving the stream on {}", arm.address));
        }

        // Said once, and then it carries on trying. The listener is still
        // starting, and saying nothing for a minute looks like a window that
        // has stopped working.
        let mut note = None;
        if !arm.said_it_was_slow && now.saturating_duration_since(arm.armed_at) >= MIRROR_ARM_SLOW {
            arm.said_it_was_slow = true;
            note = Some(format!(
                "Still connecting the listener to {}. A first run builds the Python \
                 environment, which can take a few minutes.",
                arm.address
            ));
        }

        let due = arm
            .last_try
            .map(|last| now.saturating_duration_since(last) >= MIRROR_ARM_RETRY)
            .unwrap_or(true);
        if !due {
            self.mirror_arm = Some(arm);
            return note;
        }

        arm.last_try = Some(now);
        let address = arm.address.clone();
        match self.mirror_connect(address) {
            Ok(()) => {
                // Sending the target succeeded, which is not the same as the
                // connection succeeding. Stay armed until the thread reports
                // MirrorConnected.
                self.mirror_arm = Some(arm);
                note
            }
            Err(e) => {
                // A missing TCP thread cannot fix itself by waiting, so this
                // one is reported rather than retried.
                Some(format!("Could not point the listener at the stream: {e}"))
            }
        }
    }

    /// Stop trying, because what it was waiting for is gone.
    pub fn disarm_mirror(&mut self) -> Option<String> {
        let arm = self.mirror_arm.take()?;
        Some(format!(
            "Stopped connecting the listener to {}: the listener is no longer running.",
            arm.address
        ))
    }

    /// Whether the window is still trying to connect the mirror.
    pub fn mirror_arming(&self) -> bool {
        self.mirror_arm.is_some()
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
        // this the GUI keeps claiming the mirror is connected.
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
    fn eight_fields_become_one_packet_per_event_in_order() {
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

    // The point of #184: both halves reach iMotions, so the instrument reading
    // and the aircraft's true state can be compared.
    #[test]
    fn sixteen_fields_pair_flightmodel_with_pilot() {
        let packets = packets_for_fields(&fields(&[
            "1000", "1001", // altitude: flightmodel, pilot
            "120", "121", // airspeed
            "500", "501", // vertical velocity
            "90", "91", // heading
            "2", "3", // roll
            "4", "5", // pitch
            "91", "91", // yaw: no pilot DataRef exists, so both are flightmodel
            "1.0", "1.1", // g force
        ]));

        assert_eq!(packets.len(), 8);
        assert_eq!(
            packets[0],
            "E;1;PilotDataSync;;;;;AltitudeSync;1000;1001\r\n"
        );
        assert_eq!(
            packets[1],
            "E;1;PilotDataSync;;;;;AirspeedSync;120;121\r\n"
        );
        // Vertical velocity before heading, as in the 8-field shape. The pairs
        // are in event order, which is not baton's old field order.
        assert_eq!(
            packets[2],
            "E;1;PilotDataSync;;;;;VerticalVelocitySync;500;501\r\n"
        );
        assert_eq!(packets[3], "E;1;PilotDataSync;;;;;HeadingSync;90;91\r\n");
        assert_eq!(
            packets[7],
            "E;1;PilotDataSync;;;;;GForceSync;1.0;1.1\r\n"
        );
    }

    // The regression #184 describes: with only the pilot half, the flightmodel
    // field is a copy, so nothing can be compared. Pinned so the legacy shape
    // is not mistaken for correct behaviour.
    #[test]
    fn eight_fields_still_duplicate_because_the_flightmodel_half_is_absent() {
        let packets = packets_for_fields(&fields(&[
            "1000", "120", "90", "500", "2", "3", "91", "1.0",
        ]));

        for packet in &packets {
            let values: Vec<&str> = packet
                .trim_end()
                .split(';')
                .skip(8)
                .collect();
            assert_eq!(
                values.len(),
                2,
                "every entry carries two values: {packet:?}"
            );
            assert_eq!(
                values[0], values[1],
                "the 8-field shape has no flightmodel half to send: {packet:?}"
            );
        }
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

    // --- The mirror the window connects by itself (#285) ------------------

    #[test]
    fn arming_the_mirror_fills_the_address_in() {
        let mut state = State::default();
        state.arm_mirror("127.0.0.1:5001".into(), Instant::now());
        assert!(state.mirror_arming());
        assert_eq!(
            state.mirror_addr_field, "127.0.0.1:5001",
            "the address should show in the field it would have been typed into"
        );
    }

    /// Nothing to connect through cannot come right by waiting, so it is
    /// reported once instead of retried for half a minute.
    #[test]
    fn arming_without_a_tcp_thread_reports_rather_than_retrying() {
        let mut state = State::default();
        let now = Instant::now();
        state.arm_mirror("127.0.0.1:5001".into(), now);

        let line = state.poll_mirror_arm(now).expect("no report");

        assert!(line.contains("Connect TCP first"), "{line}");
        assert!(!state.mirror_arming(), "it should have given up");
    }

    #[test]
    fn an_armed_mirror_waits_between_attempts() {
        let mut state = State::default();
        let now = Instant::now();
        // A thread it can send to, so the attempt succeeds and it stays armed.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind failed");
        let addr = listener.local_addr().expect("local_addr failed").to_string();
        state.tcp_connect(addr).expect("tcp_connect failed");

        state.arm_mirror("127.0.0.1:5001".into(), now);
        assert!(state.poll_mirror_arm(now).is_none(), "the first attempt reports nothing");
        assert!(state.mirror_arming());

        // Too soon: no second attempt, and still armed.
        assert!(state
            .poll_mirror_arm(now + StdDuration::from_millis(10))
            .is_none());
        assert!(state.mirror_arming());

        let _ = state.tcp_disconnect();
    }

    /// It says so and keeps trying, because the thing it is waiting for is a
    /// listener that may still be building its Python environment.
    #[test]
    fn an_armed_mirror_says_when_it_is_taking_a_while_and_carries_on() {
        let mut state = State::default();
        let now = Instant::now();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind failed");
        let addr = listener.local_addr().expect("local_addr failed").to_string();
        state.tcp_connect(addr).expect("tcp_connect failed");
        state.arm_mirror("127.0.0.1:5001".into(), now);

        let late = now + MIRROR_ARM_SLOW + StdDuration::from_secs(1);
        let line = state.poll_mirror_arm(late).expect("no report once it was slow");

        assert!(line.contains("Still connecting"), "{line}");
        assert!(state.mirror_arming(), "it must not give up on the listener");

        // And it says it only once.
        let later = late + MIRROR_ARM_RETRY + StdDuration::from_secs(1);
        assert!(state.poll_mirror_arm(later).is_none());
        assert!(state.mirror_arming());

        let _ = state.tcp_disconnect();
    }

    #[test]
    fn disarming_says_why_it_stopped() {
        let mut state = State::default();
        state.arm_mirror("127.0.0.1:5001".into(), Instant::now());
        let line = state.disarm_mirror().expect("no reason given");
        assert!(line.contains("no longer running"), "{line}");
        assert!(!state.mirror_arming());
        assert!(state.disarm_mirror().is_none(), "nothing to say twice");
    }

    #[test]
    fn an_armed_mirror_stops_once_it_is_connected() {
        let mut state = State::default();
        let now = Instant::now();
        state.arm_mirror("127.0.0.1:5001".into(), now);
        state.mirror_connected = true;

        let line = state.poll_mirror_arm(now).expect("no report");

        assert!(line.contains("receiving the stream"), "{line}");
        assert!(!state.mirror_arming());
    }

    #[test]
    fn disconnecting_the_mirror_stops_the_window_reconnecting_it() {
        let mut state = State::default();
        state.arm_mirror("127.0.0.1:5001".into(), Instant::now());
        state.mirror_disconnect().expect("mirror_disconnect failed");
        assert!(
            !state.mirror_arming(),
            "pressing disconnect must not be undone a moment later"
        );
    }
}
