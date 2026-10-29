use iced::{time::Duration, Task};
use std::fs::File;
use std::io::Write;
use std::time::{Duration as StdDuration, Instant};

use chrono::Local;

use crate::{
    message::FromTcpThreadMessage, message::FromIpcThreadMessage, message::ToTcpThreadMessage,
    state::build_test_packet, state::IMOTIONS_ADDR_HINT, Message, State,
};

fn connect_tcp_with_validation_and_save(state: &mut State, address: String) {
    let trimmed = address.trim().to_string();
    match State::validate_tcp_addr(&trimmed, IMOTIONS_ADDR_HINT) {
        Ok(()) => {
            state.tcp_addr_validation_error = None;
            state.tcp_addr_field = trimmed.clone();
            if let Err(e) = state.save_tcp_addr_if_new(&trimmed) {
                state.log_event(format!("Saving TCP address failed: {}", e));
            }
            if let Err(e) = state.tcp_connect(trimmed.clone()) {
                state.log_event(format!("TCP connect failed: {}", e));
            }
        }
        Err(e) => {
            let msg = e.to_string();
            // Record it like every other connection failure. This path returns
            // before tcp_connect, so without this the event log never hears that
            // the press did anything, and the inline hint under the field is easy
            // to miss.
            state.log_event(format!("TCP connect failed: {}", msg));
            state.tcp_addr_validation_error = Some(msg);
        }
    }
}

pub(crate) fn update(state: &mut State, message: Message) -> Task<Message> {
    use Message as M;

    const LAST_SEEN_WINDOW: StdDuration = StdDuration::from_secs(2);

    match message {
        M::Update => {
            state.elapsed_time += Duration::from_millis(10);

            // compute active baton connection using centralized helpers
            let ipc_conn_flag = state.is_ipc_connected();
            let recent_packet = state
                .last_baton_instant
                .map(|t| t.elapsed() <= LAST_SEEN_WINDOW)
                .unwrap_or(false);
            state.active_baton_connection = ipc_conn_flag || recent_packet;

            // process IPC messages
            if let Some(ipc_bichannel) = &state.ipc_bichannel {
                for msg in ipc_bichannel.received_messages() {
                    match msg {
                        FromIpcThreadMessage::BatonData(data) => {
                            if let Some(tcp_bi) = state.tcp_bichannel.as_mut() {
                                let _ = tcp_bi.send_to_child(ToTcpThreadMessage::Send(data.clone()));
                            }
                            state.latest_baton_send = Some(data);
                            state.last_baton_instant = Some(Instant::now());
                            state.active_baton_connection = true;
                        }
                        FromIpcThreadMessage::BatonShutdown => {
                            let _ = state.tcp_disconnect();
                            state.active_baton_connection = false;
                        }
                    }
                }
            }

            // process TCP messages
            let tcp_messages = state
                .tcp_bichannel
                .as_ref()
                .map(|bichannel| bichannel.received_messages())
                .unwrap_or_default();
            for msg in tcp_messages {
                match msg {
                    FromTcpThreadMessage::Connected => {
                        state.log_event("TCP connected to iMotions".into());
                        state.tcp_connected = true;
                    }
                    FromTcpThreadMessage::Disconnected(reason) => {
                        state.log_event(format!("TCP disconnected: {}", reason));
                        state.error_message = Some(reason);
                        state.tcp_connected = false;
                        // The thread has exited. Clear it so Connect TCP works
                        // again without pressing Disconnect TCP first.
                        state.reap_tcp_thread();
                    }
                    FromTcpThreadMessage::Sent(bytes) => {
                        state.on_tcp_packet_sent(bytes);
                    }
                    FromTcpThreadMessage::SendError(err) => {
                        state.log_event(format!("TCP send error: {}", err));
                    }
                    FromTcpThreadMessage::MirrorConnected(address) => {
                        state.log_event(format!("Python mirror connected to {}", address));
                        state.mirror_connected = true;
                    }
                    FromTcpThreadMessage::MirrorDisconnected(reason) => {
                        state.log_event(format!("Python mirror stopped: {}", reason));
                        state.mirror_connected = false;
                    }
                    FromTcpThreadMessage::MirrorLagging(reason) => {
                        state.log_event(reason);
                    }
                }
            }

            Task::none()
        }
        M::WindowCloseRequest(id) => {
            if let Some(ref bichannel) = state.ipc_bichannel {
                let _ = bichannel.killswitch_engage();
            }
            if let Some(ref bichannel) = state.tcp_bichannel {
                let _ = bichannel.killswitch_engage();
            }

            // remove unix socket file on macos test/dev path only
            if cfg!(target_os = "macos") {
                let _ = std::fs::remove_file("/tmp/baton.sock");
            }

            iced::window::close(id)
        }
        M::ConnectionMessage => {
            state.tcp_connected = state.is_tcp_connected();
            Task::none()
        }
        M::ConnectIpc => {
            if let Err(e) = state.ipc_connect() {
                state.log_event(format!("IPC connect failed: {}", e));
            }
            Task::none()
        }
        M::DisconnectIpc => {
            if let Err(e) = state.ipc_disconnect() {
                state.log_event(format!("IPC disconnect failed: {}", e));
            }
            Task::none()
        }
        M::ConnectTcp => {
            let address = state.tcp_addr_field.clone();
            connect_tcp_with_validation_and_save(state, address);
            Task::none()
        }
        M::SavedTcpAddrSelected(address) => {
            connect_tcp_with_validation_and_save(state, address);
            Task::none()
        }
        M::DisconnectTcp => {
            if let Err(e) = state.tcp_disconnect() {
                state.log_event(format!("TCP disconnect failed: {}", e));
            }
            Task::none()
        }
        M::AltitudeToggle(value) => {
            state.altitude_toggle = value;
            Task::none()
        }
        M::AirspeedToggle(value) => {
            state.airspeed_toggle = value;
            Task::none()
        }
        M::VerticalAirspeedToggle(value) => {
            state.vertical_airspeed_toggle = value;
            Task::none()
        }
        M::HeadingToggle(value) => {
            state.heading_toggle = value;
            Task::none()
        }
        M::RollToggle(value) => {
            state.roll_toggle = value;
            Task::none()
        }
        M::PitchToggle(value) => {
            state.pitch_toggle = value;
            Task::none()
        }
        M::YawToggle(value) => {
            state.yaw_toggle = value;
            Task::none()
        }
        M::GForceToggle(value) => {
            state.gforce_toggle = value;
            Task::none()
        }
        M::CreateXMLFile => create_xml_file(state),
        M::CardOpen => {
            state.card_open = true;
            Task::none()
        }
        M::CardClose => {
            state.card_open = false;
            Task::none()
        }
        M::TcpAddrFieldUpdate(addr) => {
            state.tcp_addr_field = addr;
            state.tcp_addr_validation_error = None;
            Task::none()
        }
        M::MirrorAddrFieldUpdate(addr) => {
            state.mirror_addr_field = addr;
            Task::none()
        }
        M::ConnectMirror => {
            let address = state.mirror_addr_field.clone();
            if let Err(e) = state.mirror_connect(address) {
                let msg = format!("Python mirror: {}", e);
                state.error_message = Some(msg.clone());
                state.log_event(msg);
            }
            Task::none()
        }
        M::DisconnectMirror => {
            if let Err(e) = state.mirror_disconnect() {
                state.log_event(format!("Python mirror disconnect failed: {}", e));
            }
            Task::none()
        }
        M::SendPacket => {
            let connected = state.is_tcp_connected();
            let packet = build_test_packet(state);
            match (packet, state.tcp_bichannel.as_mut()) {
                (Some(packet), Some(tcp_bi)) if connected => {
                    match tcp_bi.send_to_child(ToTcpThreadMessage::SendRaw(packet)) {
                        Ok(()) => {
                            state.error_message = None;
                            state.last_send_timestamp =
                                Some(Local::now().format("%H:%M:%S").to_string());
                            state.log_event("Test packet queued".into());
                        }
                        Err(e) => {
                            let msg = format!("Sending the test packet failed: {}", e);
                            state.error_message = Some(msg.clone());
                            state.log_event(msg);
                        }
                    }
                }
                (None, _) => {
                    state.error_message =
                        Some("Enable at least one dataref toggle to send a test packet".into());
                }
                _ => {
                    state.error_message = Some("Connect TCP before sending a test packet".into());
                }
            }
            Task::none()
        }
    }
}

fn create_xml_file(state: &mut State) -> Task<Message> {
    let mut downloads_path = match dirs::download_dir() {
        Some(p) => p,
        None => {
            let msg = "Could not determine Downloads directory".to_string();
            state.error_message = Some(msg.clone());
            state.log_event(msg);
            return Task::none();
        }
    };
    downloads_path.push("iMotions.xml");

    // Validate toggles - check if all dataref toggles are false
    if !state.altitude_toggle
        && !state.airspeed_toggle
        && !state.vertical_airspeed_toggle
        && !state.heading_toggle
        && !state.roll_toggle
        && !state.pitch_toggle
        && !state.yaw_toggle
        && !state.gforce_toggle
    {
        state.error_message = Some("Please select at least one dataref toggle".into());
        return Task::none();
    }
    state.error_message = None;

    let mut contents = String::from("<EventSource Version=\"1\" Id=\"PilotDataSync\" Name=\"Positional Flight Data\">\n");
    if state.altitude_toggle {
        contents.push_str("\t<Sample Id=\"AltitudeSync\" Name=\"Altitude Synchronization\">\n");
        contents.push_str("\t\t<Field Id=\"FlightModelAltitude\" Range=\"Variable\" Min=\"0\" Max=\"50000\" />\n");
        contents.push_str("\t\t<Field Id=\"PilotAltitude\" Range=\"Variable\" Min=\"0\" Max=\"50000\" />\n");
        contents.push_str("\t</Sample>\n");
    }
    if state.airspeed_toggle {
        contents.push_str("\t<Sample Id=\"AirspeedSync\" Name=\"Airspeed Synchronization\">\n");
        contents.push_str("\t\t<Field Id=\"FlightModelAirspeed\" Range=\"Variable\" Min=\"0\" Max=\"600\" />\n");
        contents.push_str("\t\t<Field Id=\"PilotAirspeed\" Range=\"Variable\" Min=\"0\" Max=\"600\" />\n");
        contents.push_str("\t</Sample>\n");
    }
    if state.vertical_airspeed_toggle {
        contents.push_str("\t<Sample Id=\"VerticalVelocitySync\" Name=\"Vertical Velocity Synchronization\">\n");
        contents.push_str("\t\t<Field Id=\"FlightModelVerticalVelocity\" Range=\"Variable\" Min=\"-5000\" Max=\"5000\" />\n");
        contents.push_str("\t\t<Field Id=\"PilotVerticalVelocity\" Range=\"Variable\" Min=\"-5000\" Max=\"5000\" />\n");
        contents.push_str("\t</Sample>\n");
    }
    if state.heading_toggle {
        contents.push_str("\t<Sample Id=\"HeadingSync\" Name=\"Heading Synchronization\">\n");
        contents.push_str("\t\t<Field Id=\"FlightModelHeading\" Range=\"Variable\" Min=\"0\" Max=\"360\" />\n");
        contents.push_str("\t\t<Field Id=\"PilotHeading\" Range=\"Variable\" Min=\"0\" Max=\"360\" />\n");
        contents.push_str("\t</Sample>\n");
    }

    // New samples for Roll, Pitch, Yaw, G-Force
    if state.roll_toggle {
        contents.push_str("\t<Sample Id=\"RollSync\" Name=\"Roll Synchronization\">\n");
        contents.push_str("\t\t<Field Id=\"FlightModelRoll\" Range=\"Variable\" Min=\"-180\" Max=\"180\" />\n");
        contents.push_str("\t\t<Field Id=\"PilotRoll\" Range=\"Variable\" Min=\"-180\" Max=\"180\" />\n");
        contents.push_str("\t</Sample>\n");
    }

    if state.pitch_toggle {
        contents.push_str("\t<Sample Id=\"PitchSync\" Name=\"Pitch Synchronization\">\n");
        contents.push_str("\t\t<Field Id=\"FlightModelPitch\" Range=\"Variable\" Min=\"-180\" Max=\"180\" />\n");
        contents.push_str("\t\t<Field Id=\"PilotPitch\" Range=\"Variable\" Min=\"-180\" Max=\"180\" />\n");
        contents.push_str("\t</Sample>\n");
    }

    if state.yaw_toggle {
        contents.push_str("\t<Sample Id=\"YawSync\" Name=\"Yaw Synchronization\">\n");
        contents.push_str("\t\t<Field Id=\"FlightModelYaw\" Range=\"Variable\" Min=\"0\" Max=\"360\" />\n");
        contents.push_str("\t\t<Field Id=\"PilotYaw\" Range=\"Variable\" Min=\"0\" Max=\"360\" />\n");
        contents.push_str("\t</Sample>\n");
    }

    if state.gforce_toggle {
        contents.push_str("\t<Sample Id=\"GForceSync\" Name=\"G-Force Synchronization\">\n");
        contents.push_str("\t\t<Field Id=\"FlightModelGForce\" Range=\"Variable\" Min=\"-10\" Max=\"10\" />\n");
        contents.push_str("\t\t<Field Id=\"PilotGForce\" Range=\"Variable\" Min=\"-10\" Max=\"10\" />\n");
        contents.push_str("\t</Sample>\n");
    }

    contents.push_str("</EventSource>");

    match File::create(&downloads_path) {
        Ok(mut file) => {
            if let Err(e) = file.write_all(contents.as_bytes()) {
                let msg = format!("Writing XML file failed: {}", e);
                state.error_message = Some(msg.clone());
                state.log_event(msg);
            } else {
                state.log_event(format!("XML file written to {}", downloads_path.display()));
            }
        }
        Err(e) => {
            let msg = format!("Creating XML file failed: {}", e);
            state.error_message = Some(msg.clone());
            state.log_event(msg);
        }
    }

    Task::none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::thread::sleep;

    // Waits for a condition, polling every 10ms, so the test does not hang.
    fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
        for _ in 0..200 {
            if condition() {
                return true;
            }
            sleep(StdDuration::from_millis(10));
        }
        false
    }

    #[test]
    fn send_packet_writes_one_test_packet_to_the_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind failed");
        let addr = listener.local_addr().expect("local_addr failed").to_string();

        let mut state = State::default();
        state.tcp_connect(addr).expect("tcp_connect failed");

        let (stream, _) = listener.accept().expect("accept failed");
        assert!(wait_until(|| state.is_tcp_connected()), "TCP never connected");

        let _ = update(&mut state, Message::SendPacket);

        // Bounded so a delivery regression fails the test instead of hanging
        stream
            .set_read_timeout(Some(StdDuration::from_secs(5)))
            .expect("set_read_timeout failed");
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).expect("read failed");

        assert_eq!(line, "E;1;PilotDataSync;;;;;AltitudeSync;0;0\r\n");
        assert!(state.last_send_timestamp.is_some());
        assert_eq!(state.error_message, None);

        let _ = state.tcp_disconnect();
    }

    // Pumps the Update message the GUI subscription would send, so queued
    // thread messages are processed.
    fn pump_update(state: &mut State, ticks: usize) {
        for _ in 0..ticks {
            let _ = update(state, Message::Update);
            sleep(StdDuration::from_millis(10));
        }
    }

    // Pumps until the state satisfies `condition`, returning as soon as it
    // does. A fixed tick count turns a busy machine into a failing test, since
    // these are all waits on a background thread rather than on elapsed time.
    fn pump_until(state: &mut State, mut condition: impl FnMut(&State) -> bool) -> bool {
        for _ in 0..1000 {
            let _ = update(state, Message::Update);
            if condition(state) {
                return true;
            }
            sleep(StdDuration::from_millis(10));
        }
        false
    }

    // A port nothing listens on, obtained by binding and then dropping.
    fn closed_port_addr() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind failed");
        let addr = listener.local_addr().expect("local_addr failed").to_string();
        drop(listener);
        addr
    }

    #[test]
    fn failed_connect_surfaces_an_error_and_allows_a_retry() {
        let mut state = State::default();
        state
            .tcp_connect(closed_port_addr())
            .expect("tcp_connect should spawn a thread");

        assert!(
            pump_until(&mut state, |s| s.error_message.is_some()),
            "no error surfaced"
        );

        let error = state.error_message.clone().expect("no error surfaced");
        assert!(
            error.starts_with("TCP connection failed:"),
            "unexpected error: {error}"
        );
        assert!(!state.tcp_connected);
        assert!(state.tcp_thread_handle.is_none(), "dead thread was kept");
        assert!(
            state.event_log.iter().any(|e| e.contains("TCP disconnected")),
            "failure missing from the event log: {:?}",
            state.event_log
        );

        // The retry is the point: this used to fail with "TCP thread already
        // exists" until Disconnect TCP was pressed.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind failed");
        let addr = listener.local_addr().expect("local_addr failed").to_string();
        state.tcp_connect(addr).expect("retry was rejected");
        let _ = listener.accept().expect("accept failed");
        assert!(wait_until(|| state.is_tcp_connected()), "retry never connected");
        assert!(
            state.error_message.is_none(),
            "stale failure banner after a successful retry: {:?}",
            state.error_message
        );

        let _ = state.tcp_disconnect();
    }

    // Binds a listener and returns it with its address.
    fn listener_on_free_port() -> (TcpListener, String) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind failed");
        let addr = listener.local_addr().expect("local_addr failed").to_string();
        (listener, addr)
    }

    fn read_lines(stream: &TcpStream, count: usize) -> Vec<String> {
        stream
            .set_read_timeout(Some(StdDuration::from_secs(5)))
            .expect("set_read_timeout failed");
        let mut reader = BufReader::new(stream);
        (0..count)
            .map(|_| {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read failed");
                line
            })
            .collect()
    }

    // The point of #245: Python sees the stream without taking it away from
    // iMotions.
    #[test]
    fn the_python_mirror_receives_the_same_packets_as_imotions() {
        let (imotions, imotions_addr) = listener_on_free_port();
        let (python, python_addr) = listener_on_free_port();

        let mut state = State::default();
        state.tcp_connect(imotions_addr).expect("tcp_connect failed");
        let (imotions_stream, _) = imotions.accept().expect("accept failed");
        assert!(wait_until(|| state.is_tcp_connected()), "TCP never connected");

        state
            .mirror_connect(python_addr)
            .expect("mirror_connect failed");
        let (python_stream, _) = python.accept().expect("mirror accept failed");
        assert!(
            pump_until(&mut state, |s| s.mirror_connected),
            "mirror never reported connected"
        );

        // One eight-value sample, the shape the plugin sends.
        state
            .tcp_bichannel
            .as_mut()
            .expect("no bichannel")
            .send_to_child(ToTcpThreadMessage::Send(
                "1000;120;90;500;2;3;91;1.0".to_string(),
            ))
            .expect("send failed");

        let to_imotions = read_lines(&imotions_stream, 8);
        let to_python = read_lines(&python_stream, 8);
        assert_eq!(to_imotions, to_python, "the two sinks disagree");
        assert_eq!(
            to_imotions[0],
            "E;1;PilotDataSync;;;;;AltitudeSync;1000;1000\r\n"
        );

        let _ = state.tcp_disconnect();
    }

    // A missing or dead Python process must not disturb the recording.
    #[test]
    fn imotions_keeps_streaming_when_the_mirror_cannot_connect() {
        let (imotions, imotions_addr) = listener_on_free_port();

        let mut state = State::default();
        state.tcp_connect(imotions_addr).expect("tcp_connect failed");
        let (imotions_stream, _) = imotions.accept().expect("accept failed");
        assert!(wait_until(|| state.is_tcp_connected()), "TCP never connected");

        state
            .mirror_connect(closed_port_addr())
            .expect("the request itself should be accepted");
        assert!(
            pump_until(&mut state, |s| !s.mirror_connected),
            "mirror claimed to be connected"
        );

        let _ = update(&mut state, Message::SendPacket);
        let lines = read_lines(&imotions_stream, 1);
        assert_eq!(lines[0], "E;1;PilotDataSync;;;;;AltitudeSync;0;0\r\n");
        assert!(state.is_tcp_connected(), "iMotions was dropped with the mirror");

        let _ = state.tcp_disconnect();
    }

    // Sends one eight-value sample the way baton would.
    fn send_sample(state: &mut State, altitude: &str) {
        state
            .tcp_bichannel
            .as_mut()
            .expect("no bichannel")
            .send_to_child(ToTcpThreadMessage::Send(format!(
                "{altitude};120;90;500;2;3;91;1.0"
            )))
            .expect("send failed");
    }

    // The case that actually happens in a lab: Python is killed or crashes
    // while the flight is still being recorded.
    #[test]
    fn imotions_survives_the_python_side_dying_mid_stream() {
        let (imotions, imotions_addr) = listener_on_free_port();
        let (python, python_addr) = listener_on_free_port();

        let mut state = State::default();
        state.tcp_connect(imotions_addr).expect("tcp_connect failed");
        let (imotions_stream, _) = imotions.accept().expect("accept failed");
        assert!(wait_until(|| state.is_tcp_connected()), "TCP never connected");

        state.mirror_connect(python_addr).expect("mirror_connect failed");
        let (python_stream, _) = python.accept().expect("mirror accept failed");
        assert!(
            pump_until(&mut state, |s| s.mirror_connected),
            "mirror never reported connected"
        );

        send_sample(&mut state, "1000");
        assert_eq!(read_lines(&python_stream, 8).len(), 8);

        // Python goes away mid-flight.
        drop(python_stream);
        drop(python);

        // Keep streaming. The write fails once the peer is gone, which may take
        // a sample or two to surface.
        for i in 0..20 {
            send_sample(&mut state, &format!("{}", 2000 + i));
            pump_update(&mut state, 2);
        }

        assert!(
            wait_until(|| {
                let _ = update(&mut state, Message::Update);
                !state.mirror_connected
            }),
            "mirror never noticed Python was gone"
        );
        assert!(state.is_tcp_connected(), "iMotions went down with the mirror");
        assert!(
            state.event_log.iter().any(|e| e.contains("Python mirror stopped")),
            "nothing in the event log: {:?}",
            state.event_log
        );

        // iMotions still receives everything sent after the mirror died.
        send_sample(&mut state, "9999");
        let lines = read_lines(&imotions_stream, 8);
        assert!(
            lines[0].contains("AltitudeSync"),
            "unexpected line: {}",
            lines[0]
        );

        let _ = state.tcp_disconnect();
    }

    // Counts lines off a socket on its own thread, the way a real recorder
    // would. The counter is readable while it runs, so a test can wait for
    // delivery instead of closing the socket to find out.
    fn drain_in_background(stream: TcpStream) -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = counter.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                line.clear();
                seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        });
        counter
    }

    // Connecting the mirror used to happen on the TCP thread, where a 500ms
    // connect timeout stalled the iMotions stream. It now runs on the mirror's
    // own thread, so streaming continues while the attempt is in flight.
    #[test]
    fn connecting_the_mirror_does_not_pause_the_imotions_stream() {
        // RFC 5737 TEST-NET-1: routed nowhere, so the connect attempt runs to
        // its timeout rather than being refused.
        const BLACKHOLE: &str = "192.0.2.1:5001";

        let (imotions, imotions_addr) = listener_on_free_port();
        let mut state = State::default();
        state.tcp_connect(imotions_addr).expect("tcp_connect failed");
        let (imotions_stream, _) = imotions.accept().expect("accept failed");
        assert!(wait_until(|| state.is_tcp_connected()), "TCP never connected");
        let delivered = drain_in_background(imotions_stream);

        state
            .mirror_connect(BLACKHOLE.to_string())
            .expect("mirror_connect failed");

        // Immediately, while the connect is still being attempted.
        let started = std::time::Instant::now();
        for i in 0..20 {
            send_sample(&mut state, &format!("{}", 1000 + i));
        }
        assert!(
            wait_until(|| delivered.load(std::sync::atomic::Ordering::Relaxed) >= 160),
            "iMotions got {} of 160 packets while the mirror was connecting",
            delivered.load(std::sync::atomic::Ordering::Relaxed)
        );
        let elapsed = started.elapsed();
        assert!(
            elapsed < StdDuration::from_millis(400),
            "iMotions waited {elapsed:?} for the mirror's connect attempt"
        );

        let _ = state.tcp_disconnect();
    }

    // A Python process that reads, but slower than the relay writes. iMotions
    // must still receive every packet, and the relay must say the copy is
    // falling behind rather than quietly coping.
    #[test]
    fn a_slow_python_reader_does_not_pace_imotions() {
        // Enough to exceed the socket buffers, which absorb a surprising amount:
        // at 500 samples the slow reader never creates any backpressure at all.
        const SAMPLES: usize = 1500;
        const PACKETS: usize = SAMPLES * 8;

        let (imotions, imotions_addr) = listener_on_free_port();
        let (python, python_addr) = listener_on_free_port();

        let mut state = State::default();
        state.tcp_connect(imotions_addr).expect("tcp_connect failed");
        let (imotions_stream, _) = imotions.accept().expect("accept failed");
        assert!(wait_until(|| state.is_tcp_connected()), "TCP never connected");
        let imotions_count = drain_in_background(imotions_stream);

        state.mirror_connect(python_addr).expect("mirror_connect failed");
        let (slow, _) = python.accept().expect("accept failed");
        assert!(
            pump_until(&mut state, |s| s.mirror_connected),
            "mirror never reported connected"
        );

        // Reads steadily but slower than the relay writes: about 1000 lines a
        // second against 12000 queued. Fast enough that writes keep succeeding,
        // so the mirror stays up and the queue is what fills, which is the case
        // that used to pace iMotions.
        let slow_reader = std::thread::spawn(move || {
            let mut reader = BufReader::new(slow);
            let mut line = String::new();
            let mut count = 0usize;
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                line.clear();
                count += 1;
                sleep(StdDuration::from_millis(1));
            }
            count
        });

        let started = std::time::Instant::now();
        for i in 0..SAMPLES {
            send_sample(&mut state, &format!("{}", 1000 + i % 100));
        }
        let queued = started.elapsed();
        assert!(
            queued < StdDuration::from_secs(3),
            "queueing {SAMPLES} samples took {queued:?}, the slow reader is holding the thread"
        );

        // Everything reaches iMotions long before the slow reader could have
        // consumed it: pacing to 20ms a line would take about 80 seconds.
        let delivered = |count: &std::sync::atomic::AtomicUsize| {
            count.load(std::sync::atomic::Ordering::Relaxed)
        };
        assert!(
            wait_until(|| delivered(&imotions_count) >= PACKETS),
            "iMotions only got {} of {PACKETS} packets",
            delivered(&imotions_count)
        );
        let full_delivery = started.elapsed();
        assert!(
            full_delivery < StdDuration::from_secs(10),
            "iMotions took {full_delivery:?}, it is being paced by the mirror"
        );

        // The relay says something about it rather than quietly coping: either
        // copies are being dropped, or the writer gave up on the write timeout.
        // Which one depends on how much the socket buffers absorb.
        assert!(
            wait_until(|| {
                let _ = update(&mut state, Message::Update);
                !state.mirror_connected
                    || state.event_log.iter().any(|e| e.contains("behind"))
            }),
            "a slow mirror was never reported: {:?}",
            state.event_log
        );
        assert!(
            state.is_tcp_connected(),
            "iMotions was dropped along with the slow mirror"
        );

        drop(slow_reader);
        let _ = state.tcp_disconnect();
    }

    // The ADR 0003 claim: a Python process that accepts the connection and then
    // stops reading gets dropped on the write timeout instead of stalling the
    // feed to iMotions.
    #[test]
    fn a_python_process_that_stops_reading_is_dropped_not_waited_on() {
        let (imotions, imotions_addr) = listener_on_free_port();
        let (python, python_addr) = listener_on_free_port();

        let mut state = State::default();
        state.tcp_connect(imotions_addr).expect("tcp_connect failed");
        let (imotions_stream, _) = imotions.accept().expect("accept failed");
        assert!(wait_until(|| state.is_tcp_connected()), "TCP never connected");

        // iMotions keeps draining, as a real recorder would.
        let drain = std::thread::spawn(move || {
            let mut reader = BufReader::new(imotions_stream);
            let mut sink = String::new();
            let mut lines = 0usize;
            while reader.read_line(&mut sink).unwrap_or(0) > 0 {
                sink.clear();
                lines += 1;
            }
            lines
        });

        // Python accepts, then never reads a byte.
        state.mirror_connect(python_addr).expect("mirror_connect failed");
        let (stalled, _) = python.accept().expect("accept failed");
        assert!(
            pump_until(&mut state, |s| s.mirror_connected),
            "mirror never reported connected"
        );

        let started = std::time::Instant::now();
        for i in 0..3000 {
            send_sample(&mut state, &format!("{}", 1000 + i % 100));
        }
        assert!(
            wait_until(|| {
                let _ = update(&mut state, Message::Update);
                !state.mirror_connected
            }),
            "a stalled reader was never dropped"
        );
        let elapsed = started.elapsed();

        assert!(state.is_tcp_connected(), "iMotions was taken down with it");
        // One write timeout, not one per packet.
        assert!(
            elapsed < StdDuration::from_secs(20),
            "took {elapsed:?}, the stalled mirror is holding up the stream"
        );

        drop(stalled);
        let _ = state.tcp_disconnect();
        let delivered = drain.join().unwrap_or(0);
        assert!(
            delivered > 1000,
            "iMotions only got {delivered} lines while the mirror was stalled"
        );
    }

    #[test]
    fn the_mirror_can_be_reconnected_after_python_restarts() {
        let (imotions, imotions_addr) = listener_on_free_port();

        let mut state = State::default();
        state.tcp_connect(imotions_addr).expect("tcp_connect failed");
        let (_imotions_stream, _) = imotions.accept().expect("accept failed");
        assert!(wait_until(|| state.is_tcp_connected()), "TCP never connected");

        // First Python process.
        let (python, python_addr) = listener_on_free_port();
        state
            .mirror_connect(python_addr.clone())
            .expect("mirror_connect failed");
        let (first, _) = python.accept().expect("accept failed");
        assert!(
            pump_until(&mut state, |s| s.mirror_connected),
            "mirror never reported connected"
        );
        drop(first);
        drop(python);

        // Second Python process on the same address.
        let listener = TcpListener::bind(&python_addr).expect("rebind failed");
        state.mirror_connect(python_addr).expect("reconnect rejected");
        let (second, _) = listener.accept().expect("accept failed");
        assert!(
            pump_until(&mut state, |s| s.mirror_connected),
            "mirror did not come back"
        );

        send_sample(&mut state, "1234");
        let lines = read_lines(&second, 8);
        assert_eq!(lines[0], "E;1;PilotDataSync;;;;;AltitudeSync;1234;1234\r\n");

        let _ = state.tcp_disconnect();
    }

    #[test]
    fn disconnecting_tcp_also_clears_the_mirror_status() {
        let (imotions, imotions_addr) = listener_on_free_port();
        let (python, python_addr) = listener_on_free_port();

        let mut state = State::default();
        state.tcp_connect(imotions_addr).expect("tcp_connect failed");
        let (_imotions_stream, _) = imotions.accept().expect("accept failed");
        assert!(wait_until(|| state.is_tcp_connected()), "TCP never connected");
        state.mirror_connect(python_addr).expect("mirror_connect failed");
        let (_python_stream, _) = python.accept().expect("accept failed");
        assert!(
            pump_until(&mut state, |s| s.mirror_connected),
            "mirror never reported connected"
        );

        let _ = state.tcp_disconnect();

        assert!(
            !state.mirror_connected,
            "GUI still claims the mirror is up after Disconnect TCP"
        );
    }

    // 100 samples is 800 packets to each sink. Checks nothing is dropped,
    // reordered or interleaved between the two.
    #[test]
    fn a_long_run_reaches_both_sinks_in_order() {
        let (imotions, imotions_addr) = listener_on_free_port();
        let (python, python_addr) = listener_on_free_port();

        let mut state = State::default();
        state.tcp_connect(imotions_addr).expect("tcp_connect failed");
        let (imotions_stream, _) = imotions.accept().expect("accept failed");
        assert!(wait_until(|| state.is_tcp_connected()), "TCP never connected");
        state.mirror_connect(python_addr).expect("mirror_connect failed");
        let (python_stream, _) = python.accept().expect("accept failed");
        assert!(
            pump_until(&mut state, |s| s.mirror_connected),
            "mirror never reported connected"
        );

        const SAMPLES: usize = 100;
        for i in 0..SAMPLES {
            send_sample(&mut state, &format!("{}", 1000 + i));
        }

        let to_imotions = read_lines(&imotions_stream, SAMPLES * 8);
        let to_python = read_lines(&python_stream, SAMPLES * 8);
        assert_eq!(to_imotions, to_python, "the sinks diverged over a long run");

        // Altitudes arrive in the order they were sent.
        let altitudes: Vec<&String> = to_python
            .iter()
            .filter(|line| line.contains("AltitudeSync"))
            .collect();
        assert_eq!(altitudes.len(), SAMPLES);
        assert!(altitudes[0].contains(";1000;1000"), "{}", altitudes[0]);
        assert!(
            altitudes[SAMPLES - 1].contains(&format!(";{};{}", 1000 + SAMPLES - 1, 1000 + SAMPLES - 1)),
            "{}",
            altitudes[SAMPLES - 1]
        );
        assert!(state.mirror_connected, "mirror dropped during a normal run");

        let _ = state.tcp_disconnect();
    }

    #[test]
    fn a_mirror_address_with_stray_whitespace_still_connects() {
        let (imotions, imotions_addr) = listener_on_free_port();
        let (python, python_addr) = listener_on_free_port();

        let mut state = State::default();
        state.tcp_connect(imotions_addr).expect("tcp_connect failed");
        let (_imotions_stream, _) = imotions.accept().expect("accept failed");
        assert!(wait_until(|| state.is_tcp_connected()), "TCP never connected");

        state
            .mirror_connect(format!("  {}  ", python_addr))
            .expect("padded address was rejected");
        let (_python_stream, _) = python.accept().expect("accept failed");
        assert!(
            pump_until(&mut state, |s| s.mirror_connected),
            "padded address never connected"
        );

        let _ = state.tcp_disconnect();
    }

    #[test]
    fn a_malformed_mirror_address_is_refused_before_the_thread_sees_it() {
        let (imotions, imotions_addr) = listener_on_free_port();
        let mut state = State::default();
        state.tcp_connect(imotions_addr).expect("tcp_connect failed");
        let (_imotions_stream, _) = imotions.accept().expect("accept failed");

        let error = state
            .mirror_connect("localhost:5001".to_string())
            .expect_err("host names are not supported");
        let message = error.to_string();
        assert!(
            message.contains("literal IP and port"),
            "unexpected error: {message}"
        );
        // The mirror box must offer its own example. Borrowing the iMotions one
        // would tell people to point Python at the recorder.
        assert!(
            message.contains("127.0.0.1:5001"),
            "mirror error should name the mirror's example: {message}"
        );
        assert!(
            !message.contains("iMotions"),
            "mirror error leaked the iMotions hint: {message}"
        );
        assert!(!state.mirror_connected);

        let _ = state.tcp_disconnect();
    }

    #[test]
    fn the_mirror_needs_tcp_connected_first() {
        let mut state = State::default();
        let (_python, python_addr) = listener_on_free_port();

        let error = state
            .mirror_connect(python_addr)
            .expect_err("mirror without TCP should fail");
        assert!(
            error.to_string().contains("Connect TCP first"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn empty_address_is_reported_in_the_event_log() {
        let mut state = State::default();
        assert!(state.tcp_addr_field.is_empty(), "field should start empty");

        let _ = update(&mut state, Message::ConnectTcp);

        // The inline hint alone is easy to miss, so the panel has to show it too.
        assert!(
            state
                .event_log
                .iter()
                .any(|e| e.contains("TCP connect failed") && e.contains("No address entered")),
            "empty address missing from the event log: {:?}",
            state.event_log
        );
        // An empty field is not the same mistake as a typo.
        let inline = state
            .tcp_addr_validation_error
            .clone()
            .expect("no inline validation error");
        assert!(inline.starts_with("No address entered"), "unexpected: {inline}");
        assert!(state.tcp_thread_handle.is_none(), "should not have connected");
    }

    #[test]
    fn malformed_address_is_reported_in_the_event_log() {
        let mut state = State::default();
        // Deliberately not a host name: #240 may start accepting those, and this
        // test is about reporting, not about which forms are valid.
        state.tcp_addr_field = "127.0.0.1".to_string();

        let _ = update(&mut state, Message::ConnectTcp);

        assert!(
            state
                .event_log
                .iter()
                .any(|e| e.contains("TCP connect failed") && e.contains("Invalid address")),
            "malformed address missing from the event log: {:?}",
            state.event_log
        );
        assert!(state.tcp_thread_handle.is_none(), "should not have connected");
    }

    #[test]
    fn send_packet_with_every_toggle_off_reports_an_error() {
        let mut state = State::default();
        state.altitude_toggle = false;
        state.airspeed_toggle = false;
        state.vertical_airspeed_toggle = false;
        state.heading_toggle = false;
        state.roll_toggle = false;
        state.pitch_toggle = false;
        state.yaw_toggle = false;
        state.gforce_toggle = false;

        let _ = update(&mut state, Message::SendPacket);

        assert_eq!(
            state.error_message.as_deref(),
            Some("Enable at least one dataref toggle to send a test packet")
        );
        assert!(state.last_send_timestamp.is_none());
    }

    #[test]
    fn send_packet_without_tcp_reports_an_error() {
        let mut state = State::default();

        let _ = update(&mut state, Message::SendPacket);

        assert_eq!(
            state.error_message.as_deref(),
            Some("Connect TCP before sending a test packet")
        );
        assert!(state.last_send_timestamp.is_none());
    }
}
