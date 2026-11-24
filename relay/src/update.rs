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
    connect_tcp_validated(state, address, true)
}

/// Connect, optionally remembering the address.
///
/// The test receiver's port is new every time it starts, so saving it would
/// fill the saved list with ports that will never answer again. Nothing else
/// has a reason not to be saved.
fn connect_tcp_validated(state: &mut State, address: String, save: bool) {
    let trimmed = address.trim().to_string();
    match State::validate_tcp_addr(&trimmed, IMOTIONS_ADDR_HINT) {
        Ok(()) => {
            state.tcp_addr_validation_error = None;
            state.tcp_addr_field = trimmed.clone();
            if save {
                if let Err(e) = state.save_tcp_addr_if_new(&trimmed) {
                    state.log_event(format!("Saving TCP address failed: {}", e));
                }
            }
            if let Err(e) = state.tcp_connect(trimmed.clone()) {
                state.log_event(format!("TCP connect failed: {}", e));
            }
        }
        Err(e) => {
            state.tcp_addr_validation_error = Some(e.to_string());
        }
    }
}

pub(crate) fn update(state: &mut State, message: Message) -> Task<Message> {
    use Message as M;

    const LAST_SEEN_WINDOW: StdDuration = StdDuration::from_secs(2);

    match message {
        M::Update => {
            state.elapsed_time += Duration::from_millis(10);

            // Collect whatever the listener has printed, and notice if it has
            // exited. Uses the tick the window already runs on.
            let was_running = state.listener.status.is_running();
            state.listener.poll();
            if was_running && !state.listener.status.is_running() {
                state.log_event(state.listener.status.describe());
            }

            let was_setting_up = state.setup.status.is_running();
            state.setup.poll();
            if was_setting_up && !state.setup.status.is_running() {
                state.log_event(match state.setup.status {
                    crate::listener::ListenerStatus::Finished(Some(0)) => {
                        "The Python side is set up. Start flight will work now.".to_string()
                    }
                    _ => format!("Setting up the Python side: {}", state.setup.status.describe()),
                });
            }

            if let Some(receiver) = state.test_receiver.as_mut() {
                receiver.poll();
            }

            pump_replay(state);

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
                        // While the window is still connecting the mirror, a
                        // refusal is the expected answer rather than news: the
                        // listener has not finished starting Python. Logging
                        // each attempt would bury the event log.
                        if !state.mirror_arming() {
                            state.log_event(format!("Python mirror stopped: {}", reason));
                        }
                        state.mirror_connected = false;
                    }
                    FromTcpThreadMessage::MirrorLagging(reason) => {
                        state.log_event(reason);
                    }
                }
            }

            // After the TCP messages, so a MirrorConnected that arrived this
            // tick is already in hand and the arm can finish instead of
            // retrying a connection that has already landed.
            if state.mirror_arming() {
                if state.listener.status.is_running() {
                    if let Some(line) = state.poll_mirror_arm(Instant::now()) {
                        state.log_event(line);
                    }
                } else if let Some(line) = state.disarm_mirror() {
                    // What it was waiting for has gone, so waiting longer
                    // cannot help.
                    state.log_event(line);
                }
            }

            Task::none()
        }
        M::WindowCloseRequest(id) => {
            // The listener is a child process, and closing the window does not
            // end it by itself: it would keep running, keep holding the port,
            // and keep writing to the CSV with nothing left to show for it.
            // Measured on Windows, so this is not a precaution.
            state.listener.stop();
            state.setup.stop();

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

        M::PilotIdFieldUpdate(value) => {
            state.pilot_id_field = value;
            Task::none()
        }

        M::FlightIdFieldUpdate(value) => {
            state.flight_id_field = value;
            Task::none()
        }

        M::ListenerModeSelected(mode) => {
            // Changing it under a running listener would mislead: the panel
            // would describe one thing while another was recording.
            if state.listener.status.is_running() {
                state.error_message =
                    Some("Stop the listener before changing what it runs.".into());
            } else {
                state.listener_mode = mode;
                // The two modes report different things, so what the last one
                // said does not belong under the new one's heading.
                state.listener.live = Default::default();
            }
            Task::none()
        }

        M::BriefFieldUpdate(value) => {
            state.brief_field = value;
            Task::none()
        }

        M::GiveInstruction => {
            // live_groups reads one line per instruction, which is what
            // pressing Enter at its console does.
            match state.listener.send_line("") {
                Ok(()) => {
                    state.error_message = None;
                }
                Err(e) => {
                    let msg = e.to_string();
                    state.error_message = Some(msg.clone());
                    state.log_event(msg);
                }
            }
            Task::none()
        }

        M::StartFlight => {
            start_flight(state);
            Task::none()
        }

        M::StopFlight => {
            stop_flight(state);
            Task::none()
        }

        M::UseTestReceiver => {
            start_test_receiver(state);
            Task::none()
        }

        M::StopTestReceiver => {
            stop_test_receiver(state);
            Task::none()
        }

        M::RecordingSelected(label) => {
            select_recording(state, label);
            Task::none()
        }

        M::ReplayFlightSelected(flight) => {
            state.selected_replay_flight = Some(flight);
            Task::none()
        }

        M::StartReplay => {
            start_replay(state);
            Task::none()
        }

        M::StopReplay => {
            if let Some(replay) = state.replay.take() {
                state.log_event(format!(
                    "Replay stopped after {} of {} samples",
                    replay.sent(),
                    replay.total()
                ));
            }
            Task::none()
        }

        M::SetUpPython => {
            set_up_python(state);
            Task::none()
        }

        M::ToggleAdvanced => {
            state.show_advanced = !state.show_advanced;
            Task::none()
        }
    }
}

/// Build the Python environment from the window.
///
/// `uv sync` in a terminal was the one step the window could not do, and a
/// fresh clone cannot run a listener without it.
fn set_up_python(state: &mut State) {
    if state.setup.status.is_running() {
        state.error_message = Some("The Python side is already being set up.".into());
        return;
    }
    let Some(inference_dir) = state.inference_dir.clone() else {
        let msg = "Cannot find the inference folder next to the relay. Run the relay from the \
                   repository."
            .to_string();
        state.error_message = Some(msg.clone());
        state.log_event(msg);
        return;
    };

    let started = crate::listener::setup_command(&inference_dir)
        .and_then(|command| state.setup.start(command));
    match started {
        Ok(()) => {
            state.error_message = None;
            state.log_event("Setting up the Python side, this takes a few minutes".into());
        }
        Err(e) => {
            let msg = e.to_string();
            state.error_message = Some(msg.clone());
            state.log_event(msg);
        }
    }
}

/// Send whatever the replay owes, through the same door baton's samples use.
///
/// Paced here rather than on a thread of its own: the window already ticks at
/// 100 Hz, which is five times what a replay needs, and nothing can outlive
/// the window or race it this way.
fn pump_replay(state: &mut State) {
    let Some(replay) = state.replay.as_mut() else {
        return;
    };
    let batch = replay.due(Instant::now());
    if batch.is_empty() {
        return;
    }
    let finished = replay.finished();
    let description = replay.describe();

    match state.tcp_bichannel.as_mut() {
        Some(tcp_bi) => {
            for payload in batch {
                let _ = tcp_bi.send_to_child(ToTcpThreadMessage::Send(payload));
            }
        }
        None => {
            // Nothing to send to, and a replay that silently goes nowhere is
            // worse than one that stops and says so.
            state.replay = None;
            let msg = "The replay stopped: there is no connection to send it to.".to_string();
            state.error_message = Some(msg.clone());
            state.log_event(msg);
            return;
        }
    }

    if finished {
        state.log_event(description);
    }
}

/// Start a Flight: the listener, then the stream into it.
///
/// One button because it is one intention. Doing it by hand meant starting the
/// listener, typing the mirror address, and knowing that the order mattered
/// because the mirror is a client and the listener binds the port.
fn start_flight(state: &mut State) {
    if state.listener.status.is_running() {
        state.error_message =
            Some("A flight is already running. Stop it before starting another.".into());
        return;
    }

    // What was typed is checked before the connection is. A missing pilot id
    // has to be fixed either way, and naming it first means one error at a
    // time instead of being sent to connect something and told again.
    let command = match listener_command_for(state) {
        Ok(command) => command,
        Err(e) => {
            let msg = e.to_string();
            state.error_message = Some(msg.clone());
            state.log_event(msg);
            return;
        }
    };

    // The listener is sent a copy of the iMotions stream, so without one there
    // is nothing to copy. Said here, with the way out, rather than left to be
    // found later as a listener that recorded an empty file.
    if !state.is_tcp_connected() {
        let msg = "Connect iMotions first, or press Use a test receiver. The listener is sent a \
                   copy of the iMotions stream, so there has to be a stream."
            .to_string();
        state.error_message = Some(msg.clone());
        state.log_event(msg);
        return;
    }

    let described = command.display();
    if let Err(e) = state.listener.start(command) {
        let msg = e.to_string();
        state.listener.status = crate::listener::ListenerStatus::Failed(msg.clone());
        state.error_message = Some(msg.clone());
        state.log_event(msg);
        return;
    }
    state.error_message = None;
    state.log_event(format!("Listener started: {described}"));

    // The listener binds the port and the mirror connects to it, so this can
    // only succeed once Python is up. `arm_mirror` keeps trying, which is why
    // the order is no longer something to remember.
    state.arm_mirror(
        format!("127.0.0.1:{}", crate::listener::DEFAULT_LISTENER_PORT),
        Instant::now(),
    );
    state.log_event("Connecting the listener to the stream".into());
}

/// Stop a Flight: the listener, and the stream into it.
fn stop_flight(state: &mut State) {
    let was_running = state.listener.status.is_running();
    state.listener.stop();
    if let Err(e) = state.mirror_disconnect() {
        state.log_event(format!("Python mirror disconnect failed: {}", e));
    }
    if was_running {
        state.log_event("Flight stopped".into());
    }
}

/// Stand in for iMotions on a local port, and point the relay at it.
fn start_test_receiver(state: &mut State) {
    if state.test_receiver.is_some() {
        state.error_message = Some("The test receiver is already running.".into());
        return;
    }
    match crate::receiver::TestReceiver::start() {
        Ok(receiver) => {
            let address = receiver.addr();
            state.test_receiver = Some(receiver);
            state.log_event(format!("Test receiver listening on {address}"));
            state.error_message = None;

            // Pressing this means using the receiver instead of whatever the
            // relay is pointed at, and `tcp_connect` refuses a second thread.
            // Without this the receiver would start and nothing would connect
            // to it. The old address stays in Saved IPs, so going back is a
            // click.
            if state.tcp_thread_handle.is_some() {
                let previous = state.tcp_addr_field.clone();
                if let Err(e) = state.tcp_disconnect() {
                    state.log_event(format!("TCP disconnect failed: {}", e));
                }
                state.log_event(format!("Disconnected from {previous} to use the test receiver"));
            }

            // Its port is new each time, so this one is not remembered.
            connect_tcp_validated(state, address, false);
        }
        Err(e) => {
            let msg = e.to_string();
            state.error_message = Some(msg.clone());
            state.log_event(msg);
        }
    }
}

/// Take the test receiver down, and the connection to it with it.
fn stop_test_receiver(state: &mut State) {
    let Some(receiver) = state.test_receiver.take() else {
        return;
    };
    // Only if that is where the relay is pointed. Someone may have moved it to
    // the real iMotions in the meantime, and that connection is not ours.
    if state.tcp_addr_field == receiver.addr() && state.tcp_thread_handle.is_some() {
        if let Err(e) = state.tcp_disconnect() {
            state.log_event(format!("TCP disconnect failed: {}", e));
        }
    }
    state.log_event(format!("Test receiver on {} stopped", receiver.addr()));
    // Dropping it ends its thread and frees the port.
    drop(receiver);
}

/// Pick a recording, and read the flights inside it.
fn select_recording(state: &mut State, label: String) {
    let Some(index) = state.recordings.iter().position(|r| r.label() == label) else {
        return;
    };
    let path = state.recordings[index].path.clone();

    // The flights are read now rather than at startup, because this means
    // reading the whole file and these run to a hundred thousand rows.
    match crate::replay::read_recording(&path) {
        Ok(recording) => {
            state.selected_replay_flight = recording.flights.first().cloned();
            state.recordings[index] = recording;
            state.selected_recording = Some(label);
            state.error_message = None;
        }
        Err(e) => {
            let msg = e.to_string();
            state.error_message = Some(msg.clone());
            state.log_event(msg);
        }
    }
}

/// Replay the selected flight in place of X-Plane.
fn start_replay(state: &mut State) {
    if state.replay.as_ref().is_some_and(|r| !r.finished()) {
        state.error_message = Some("A replay is already running. Stop it first.".into());
        return;
    }
    if !state.is_tcp_connected() {
        let msg = "Connect iMotions or the test receiver first. A replay goes out the same way a \
                   real flight does."
            .to_string();
        state.error_message = Some(msg.clone());
        state.log_event(msg);
        return;
    }

    let Some(label) = state.selected_recording.clone() else {
        state.error_message = Some("Pick a recording to replay.".into());
        return;
    };
    let Some(recording) = state.recordings.iter().find(|r| r.label() == label).cloned() else {
        state.error_message = Some(format!("{label} is no longer in the data folder."));
        return;
    };
    if !recording.flights.is_empty() && state.selected_replay_flight.is_none() {
        state.error_message = Some("Pick which flight to replay.".into());
        return;
    }
    let flight = state.selected_replay_flight.clone();

    match crate::replay::payloads_for(&recording.path, flight.as_deref()) {
        Ok(payloads) => {
            let name = flight.unwrap_or_else(|| recording.label());
            state.log_event(format!(
                "Replaying {} samples of {name} at {} Hz, about {:.0}s",
                payloads.len(),
                crate::replay::DEFAULT_REPLAY_HZ,
                payloads.len() as f64 / crate::replay::DEFAULT_REPLAY_HZ,
            ));
            state.replay = Some(crate::replay::Replay::new(
                name,
                payloads,
                crate::replay::DEFAULT_REPLAY_HZ,
            ));
            state.error_message = None;
        }
        Err(e) => {
            let msg = e.to_string();
            state.error_message = Some(msg.clone());
            state.log_event(msg);
        }
    }
}

/// The command the panel's fields describe, or the reason there is none.
///
/// Every failure here is one the operator can act on, so each becomes the
/// error banner rather than a status the panel leaves them to interpret.
fn listener_command_for(state: &State) -> anyhow::Result<crate::listener::ListenerCommand> {
    let Some(inference_dir) = state.inference_dir.clone() else {
        anyhow::bail!(
            "Cannot find the inference folder next to the relay. Run the relay from the \
             repository, or start the listener from a terminal."
        );
    };

    match state.listener_mode {
        crate::listener::ListenerMode::Capture => crate::listener::capture_command(
            &inference_dir,
            &state.pilot_id_field,
            &state.flight_id_field,
            crate::listener::DEFAULT_LISTENER_PORT,
        ),
        crate::listener::ListenerMode::LiveGroups => crate::listener::live_groups_command(
            &inference_dir,
            std::path::Path::new(state.brief_field.trim()),
            &state.pilot_id_field,
            &state.flight_id_field,
            crate::listener::DEFAULT_LISTENER_PORT,
        ),
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
    use socket2::{Domain, Protocol, Socket, Type};
    use std::io::{BufRead, BufReader};
    use std::net::{SocketAddr, TcpListener, TcpStream};
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

    // How much the Python side is allowed to buffer in the kernel, for the
    // tests that are about the mirror falling behind.
    //
    // Linux rounds this up and enforces a floor, so the effective buffer is
    // larger than the number asked for; what matters is that it is kilobytes
    // rather than megabytes.
    const SMALL_RECV_BUFFER: usize = 2048;

    // A Python-side listener whose accepted connections have a deliberately
    // small receive buffer.
    //
    // The two tests below are about what the relay does when the mirror cannot
    // keep up, so they need a mirror that demonstrably cannot. Half of that is
    // MIRROR_SEND_BUFFER, which bounds our own side; the other half is the
    // reader's buffer, which in production belongs to the Python process and is
    // whatever the host decided. Leaving it at the default meant the two sides
    // together absorbed the whole run on a machine with generous tcp_rmem and
    // blocked on a machine without, which is issue #251: the tests passed or
    // failed according to the host rather than the code.
    //
    // Setting SO_RCVBUF explicitly also switches autotuning off for the socket,
    // so the window stays pinned small. It has to be set on the listener,
    // before listen, to be inherited by accepted connections.
    fn stalling_listener_on_free_port() -> (TcpListener, String) {
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("parse failed");
        let socket =
            Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).expect("socket failed");
        socket
            .set_recv_buffer_size(SMALL_RECV_BUFFER)
            .expect("set_recv_buffer_size failed");
        socket.bind(&addr.into()).expect("bind failed");
        socket.listen(16).expect("listen failed");

        let listener: TcpListener = socket.into();
        let bound = listener
            .local_addr()
            .expect("local_addr failed")
            .to_string();
        (listener, bound)
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
        // 4000 packets against the roughly 500 the bounded buffers hold, so the
        // mirror falls behind regardless of the host. This used to be 1500
        // samples chasing the kernel's autotuned buffers, and still lost.
        const SAMPLES: usize = 500;
        const PACKETS: usize = SAMPLES * 8;

        let (imotions, imotions_addr) = listener_on_free_port();
        let (python, python_addr) = stalling_listener_on_free_port();

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
        // second against 4000 queued. The window reopens as it reads, so some
        // writes keep succeeding and the mirror does not simply go away; it is
        // the case that used to pace iMotions.
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
        // Which one depends on how the reader's progress lines up with the
        // timeout, so both count.
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
        // The Python side never reads, so its pinned receive buffer fills and
        // stays full: writes block, the 100ms timeout fires and the mirror is
        // dropped. With the host's default buffers the whole run fits in the
        // kernel, nothing blocks, and the timeout never fires at all.
        const SAMPLES: usize = 500;

        let (imotions, imotions_addr) = listener_on_free_port();
        let (python, python_addr) = stalling_listener_on_free_port();

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
        for i in 0..SAMPLES {
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
    fn the_id_fields_hold_what_was_typed() {
        let mut state = State::default();
        let _ = update(&mut state, Message::PilotIdFieldUpdate("P004".into()));
        let _ = update(&mut state, Message::FlightIdFieldUpdate("P004-T1".into()));
        assert_eq!(state.pilot_id_field, "P004");
        assert_eq!(state.flight_id_field, "P004-T1");
    }

    #[test]
    fn starting_a_flight_without_a_pilot_id_says_so_and_starts_nothing() {
        let mut state = State::default();
        state.flight_id_field = "P004-T1".into();

        let _ = update(&mut state, Message::StartFlight);

        assert!(
            state
                .error_message
                .as_deref()
                .unwrap_or_default()
                .contains("Pilot id is required"),
            "{:?}",
            state.error_message
        );
        assert!(!state.listener.status.is_running());
    }

    #[test]
    fn a_flight_id_that_would_escape_the_data_folder_is_refused() {
        let mut state = State::default();
        state.pilot_id_field = "P004".into();
        state.flight_id_field = "../../etc/passwd".into();

        let _ = update(&mut state, Message::StartFlight);

        assert!(state.error_message.is_some());
        assert!(!state.listener.status.is_running());
    }

    #[test]
    fn stopping_a_flight_that_never_started_is_harmless() {
        let mut state = State::default();
        let _ = update(&mut state, Message::StopFlight);
        assert!(!state.listener.status.is_running());
    }

    /// The relay has to survive the listener, because the plugin cannot
    /// reconnect to a relay that restarted (docs/adr/0005, #255, #256).
    #[test]
    fn listener_errors_leave_the_relay_connections_alone() {
        let mut state = State::default();
        state.tcp_connected = true;
        state.pilot_id_field = String::new();

        let _ = update(&mut state, Message::StartFlight);
        let _ = update(&mut state, Message::StopFlight);

        assert!(state.tcp_connected);
    }

    #[test]
    fn the_mode_cannot_be_changed_under_a_running_listener() {
        use crate::listener::{ListenerMode, ListenerStatus};

        let mut state = State::default();
        state.listener.status = ListenerStatus::Running;

        let _ = update(&mut state, Message::ListenerModeSelected(ListenerMode::LiveGroups));

        assert_eq!(state.listener_mode, ListenerMode::Capture);
        assert!(state
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("Stop the listener"));
    }

    #[test]
    fn the_mode_changes_while_nothing_is_running() {
        use crate::listener::ListenerMode;

        let mut state = State::default();
        let _ = update(&mut state, Message::ListenerModeSelected(ListenerMode::LiveGroups));
        assert_eq!(state.listener_mode, ListenerMode::LiveGroups);
    }

    /// The two modes report different things, so the last flight's report must
    /// not appear under the other one's heading.
    #[test]
    fn changing_the_mode_clears_what_the_last_one_reported() {
        use crate::listener::ListenerMode;

        let mut state = State::default();
        state.listener.live.flight_id = "P004-T1".into();
        state.listener.live.csv = "data/session_flight_data.csv".into();

        let _ = update(&mut state, Message::ListenerModeSelected(ListenerMode::LiveGroups));

        assert!(state.listener.live.flight_id.is_empty());
        assert!(state.listener.live.csv.is_empty());
        assert!(state.listener.live.status.is_none());
    }

    #[test]
    fn live_groups_without_a_brief_says_which_file_is_missing() {
        use crate::listener::ListenerMode;

        let mut state = State::default();
        state.listener_mode = ListenerMode::LiveGroups;
        state.pilot_id_field = "P004".into();
        state.flight_id_field = "P004-T1".into();
        state.brief_field = "briefs/does_not_exist.csv".into();

        let _ = update(&mut state, Message::StartFlight);

        let error = state.error_message.unwrap_or_default();
        assert!(error.contains("No brief at"), "{error}");
        assert!(!state.listener.status.is_running());
    }

    #[test]
    fn giving_an_instruction_with_no_listener_says_so() {
        let mut state = State::default();
        let _ = update(&mut state, Message::GiveInstruction);
        assert!(state
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("nothing to tell"));
    }

    #[test]
    fn the_brief_field_holds_what_was_typed() {
        let mut state = State::default();
        let _ = update(&mut state, Message::BriefFieldUpdate("briefs/x.csv".into()));
        assert_eq!(state.brief_field, "briefs/x.csv");
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

    // --- Running a session without a terminal (#285) ----------------------

    #[test]
    fn a_flight_with_nowhere_to_send_says_what_to_connect() {
        let mut state = State::default();
        state.pilot_id_field = "P004".into();
        state.flight_id_field = "P004-T1".into();

        let _ = update(&mut state, Message::StartFlight);

        let error = state.error_message.clone().unwrap_or_default();
        assert!(error.contains("Connect iMotions first"), "{error}");
        assert!(error.contains("test receiver"), "it should name the way out: {error}");
        assert!(!state.listener.status.is_running(), "nothing should have started");
        assert!(!state.mirror_arming());
    }

    /// A missing pilot id is reported before the connection is checked. The
    /// other order sends the operator to connect something and then tells them
    /// about the id, which is two trips for one mistake.
    #[test]
    fn what_was_typed_is_checked_before_the_connection_is() {
        let mut state = State::default();
        state.flight_id_field = "P004-T1".into();

        let _ = update(&mut state, Message::StartFlight);

        let error = state.error_message.clone().unwrap_or_default();
        assert!(error.contains("Pilot id is required"), "{error}");
    }

    #[test]
    fn stopping_a_flight_disarms_the_mirror() {
        let mut state = State::default();
        state.arm_mirror("127.0.0.1:5001".into(), Instant::now());
        assert!(state.mirror_arming());

        let _ = update(&mut state, Message::StopFlight);

        assert!(!state.mirror_arming(), "the mirror was left trying to connect");
    }

    #[test]
    fn the_individual_controls_are_hidden_until_they_are_asked_for() {
        let mut state = State::default();
        assert!(!state.show_advanced, "the plumbing should not be the first thing shown");

        let _ = update(&mut state, Message::ToggleAdvanced);
        assert!(state.show_advanced);
        let _ = update(&mut state, Message::ToggleAdvanced);
        assert!(!state.show_advanced);
    }

    #[test]
    fn the_test_receiver_starts_and_the_relay_is_pointed_at_it() {
        let mut state = State::default();

        let _ = update(&mut state, Message::UseTestReceiver);

        let address = state
            .test_receiver
            .as_ref()
            .expect("no test receiver was started")
            .addr();
        assert_eq!(state.tcp_addr_field, address, "the address should be filled in");
        assert!(
            pump_until(&mut state, |s| s.is_tcp_connected()),
            "the relay never connected to its own receiver"
        );
        assert!(
            !state.saved_tcp_addrs.contains(&address),
            "a throwaway port should not join the saved addresses"
        );

        let _ = update(&mut state, Message::StopTestReceiver);
        assert!(state.test_receiver.is_none());
        assert!(!state.is_tcp_connected(), "the connection to it should have gone too");
    }

    /// Pressing it means using it instead, so the connection it replaces has
    /// to go first: `tcp_connect` refuses a second thread, and the receiver
    /// would otherwise start with nothing connected to it.
    #[test]
    fn using_the_test_receiver_takes_over_from_an_existing_connection() {
        let (listener, addr) = listener_on_free_port();
        let mut state = State::default();
        state.tcp_connect(addr.clone()).expect("tcp_connect failed");
        let _accepted = listener.accept().expect("accept failed");
        assert!(pump_until(&mut state, |s| s.is_tcp_connected()), "never connected");

        let _ = update(&mut state, Message::UseTestReceiver);

        let receiver_addr = state
            .test_receiver
            .as_ref()
            .expect("no test receiver was started")
            .addr();
        assert_ne!(receiver_addr, addr);
        assert_eq!(state.tcp_addr_field, receiver_addr);
        assert!(
            pump_until(&mut state, |s| s.is_tcp_connected()),
            "it started the receiver but connected nothing to it"
        );
        assert!(
            state
                .event_log
                .iter()
                .any(|line| line.contains("to use the test receiver")),
            "the takeover was not reported: {:?}",
            state.event_log
        );

        let _ = update(&mut state, Message::StopTestReceiver);
    }

    #[test]
    fn a_second_test_receiver_is_refused_rather_than_leaking_the_first() {
        let mut state = State::default();
        let _ = update(&mut state, Message::UseTestReceiver);
        let first = state.test_receiver.as_ref().map(|r| r.port);

        let _ = update(&mut state, Message::UseTestReceiver);

        assert_eq!(state.test_receiver.as_ref().map(|r| r.port), first);
        assert!(state
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("already running"));

        let _ = update(&mut state, Message::StopTestReceiver);
    }

    /// A one row recording, written where a test may write.
    fn one_row_recording() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("relay-replay-tests");
        std::fs::create_dir_all(&dir).expect("could not make a temp folder");
        let path = dir.join(format!("one_flight_{}.csv", std::process::id()));
        std::fs::write(
            &path,
            "flight_id,altitude,velocity,vertical_speed,heading\nF1,3000,120,500,270\n",
        )
        .expect("could not write the recording");
        path
    }

    /// The whole replay path: a CSV becomes the same iMotions packets a live
    /// sample does, in the same order, at the same destination.
    #[test]
    fn a_replayed_row_arrives_as_the_relay_s_own_packets() {
        let path = one_row_recording();
        let payloads = crate::replay::payloads_for(&path, Some("F1")).expect("no payloads");
        assert_eq!(payloads.len(), 1);

        let (listener, addr) = listener_on_free_port();
        let mut state = State::default();
        state.tcp_connect(addr).expect("tcp_connect failed");
        let (stream, _) = listener.accept().expect("accept failed");
        assert!(pump_until(&mut state, |s| s.is_tcp_connected()), "never connected");

        // 0 Hz so the single sample goes out on the first tick.
        state.replay = Some(crate::replay::Replay::new("F1".into(), payloads, 0.0));
        let _ = update(&mut state, Message::Update);

        stream
            .set_read_timeout(Some(StdDuration::from_secs(5)))
            .expect("set_read_timeout failed");
        let mut reader = BufReader::new(stream);
        let mut lines = Vec::new();
        for _ in 0..8 {
            let mut line = String::new();
            reader.read_line(&mut line).expect("read failed");
            lines.push(line);
        }

        assert_eq!(lines[0], "E;1;PilotDataSync;;;;;AltitudeSync;3000.000000;3000.000000\r\n");
        assert_eq!(lines[1], "E;1;PilotDataSync;;;;;AirspeedSync;120.000000;120.000000\r\n");
        assert_eq!(
            lines[2],
            "E;1;PilotDataSync;;;;;VerticalVelocitySync;500.000000;500.000000\r\n"
        );
        assert_eq!(lines[3], "E;1;PilotDataSync;;;;;HeadingSync;270.000000;270.000000\r\n");
        // The columns the recording lacks are sent as zero, not skipped, so
        // every sample is still eight packets.
        assert!(lines[4].contains("RollSync;0.000000;0.000000"), "{}", lines[4]);
        assert!(lines[7].contains("GForceSync;0.000000;0.000000"), "{}", lines[7]);

        let _ = state.tcp_disconnect();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_replay_stops_itself_when_there_is_nowhere_to_send() {
        let mut state = State::default();
        state.replay = Some(crate::replay::Replay::new(
            "F1".into(),
            vec!["1;1;2;2;3;3;4;4;5;5;6;6;7;7;8;8".to_string()],
            0.0,
        ));

        let _ = update(&mut state, Message::Update);

        assert!(state.replay.is_none(), "it should not go on sending into nothing");
        assert!(state
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("no connection"));
    }

    #[test]
    fn a_replay_without_a_connection_is_refused_with_the_reason() {
        let mut state = State::default();
        let _ = update(&mut state, Message::StartReplay);
        let error = state.error_message.clone().unwrap_or_default();
        assert!(error.contains("Connect iMotions or the test receiver"), "{error}");
        assert!(state.replay.is_none());
    }

    #[test]
    fn a_replay_with_no_recording_picked_says_to_pick_one() {
        let (listener, addr) = listener_on_free_port();
        let mut state = State::default();
        state.tcp_connect(addr).expect("tcp_connect failed");
        let _accepted = listener.accept().expect("accept failed");
        assert!(pump_until(&mut state, |s| s.is_tcp_connected()), "never connected");

        let _ = update(&mut state, Message::StartReplay);

        assert_eq!(state.error_message.as_deref(), Some("Pick a recording to replay."));
        let _ = state.tcp_disconnect();
    }

    #[test]
    fn picking_a_recording_reads_its_flights_and_offers_the_first() {
        let dir = std::env::temp_dir()
            .join("relay-update-tests")
            .join(format!("picker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("could not make a temp folder");
        std::fs::write(
            dir.join("two_flights.csv"),
            "flight_id,altitude,heading\nA,3000,270\nB,5000,90\n",
        )
        .expect("could not write the recording");

        let mut state = State::default();
        state.recordings = crate::replay::recordings_in(&dir);
        let label = "two_flights.csv".to_string();
        assert!(
            state.recordings.iter().any(|r| r.label() == label),
            "the recording should be listed: {:?}",
            state.recordings.iter().map(|r| r.label()).collect::<Vec<_>>()
        );
        // Listing does not read the flights, because that means reading the
        // whole file and the committed recordings are large.
        assert!(state.recordings[0].flights.is_empty());

        let _ = update(&mut state, Message::RecordingSelected(label.clone()));

        assert_eq!(state.selected_recording, Some(label.clone()));
        let recording = state
            .recordings
            .iter()
            .find(|r| r.label() == label)
            .expect("the recording went missing");
        assert_eq!(recording.flights, vec!["A", "B"], "its flights were not read");
        assert_eq!(
            state.selected_replay_flight.as_deref(),
            Some("A"),
            "the first flight should be offered rather than nothing"
        );
    }

    /// Pump the window's tick until `condition` holds or `limit` passes.
    fn pump_for(
        state: &mut State,
        limit: StdDuration,
        mut condition: impl FnMut(&State) -> bool,
    ) -> bool {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            let _ = update(state, Message::Update);
            if condition(state) {
                return true;
            }
            sleep(StdDuration::from_millis(5));
        }
        false
    }

    /// A whole session driven through the window's own messages, with no
    /// terminal, no simulator and no iMotions.
    ///
    /// This is the claim #285 makes, so it is worth a test that makes it
    /// rather than a list of things to click. It presses Use a test receiver,
    /// Start flight and Replay this flight, waits out a ten second flight, and
    /// then checks the rows reached the CSV.
    ///
    /// Ignored because it needs uv and spends fifteen seconds of wall clock
    /// doing it. Run it with `cargo test -- --ignored --nocapture` after
    /// changing anything in this path.
    #[test]
    #[ignore = "needs uv and binds port 5001; run manually"]
    fn a_whole_flight_runs_from_the_window_with_nothing_but_the_window() {
        use std::io::Write as _;

        let inference = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("relay/ has a parent")
            .join("inference");
        assert!(inference.join("pyproject.toml").is_file(), "test needs inference/");

        // A ten second flight, climbing, so the rows are telling apart.
        let dir = std::env::temp_dir()
            .join("relay-update-tests")
            .join(format!("e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("could not make a temp folder");
        let recording = dir.join("rehearsal.csv");
        {
            let mut file = std::fs::File::create(&recording).expect("create failed");
            writeln!(file, "flight_id,altitude,velocity,vertical_speed,heading").unwrap();
            for i in 0..200 {
                writeln!(file, "REHEARSAL,{},120,500,270", 3000 + i).unwrap();
            }
        }

        let flight_id = format!("WINDOWE2E{}", std::process::id());
        let mut state = State::default();
        state.recordings = crate::replay::recordings_in(&dir);
        state.pilot_id_field = "P004".into();
        state.flight_id_field = flight_id.clone();

        // Stand in for iMotions.
        let _ = update(&mut state, Message::UseTestReceiver);
        assert!(
            pump_for(&mut state, StdDuration::from_secs(10), |s| s.is_tcp_connected()),
            "the relay never connected to its own test receiver"
        );

        // Start the flight. The window starts the listener and connects it.
        let _ = update(&mut state, Message::StartFlight);
        assert!(
            state.listener.status.is_running(),
            "the listener did not start: {:?}",
            state.error_message
        );
        assert!(
            pump_for(&mut state, StdDuration::from_secs(60), |s| s.mirror_connected),
            "the window never connected the listener to the stream. Listener output: {:?}",
            state.listener.recent_output(10)
        );
        assert!(!state.mirror_arming(), "it should stop trying once connected");

        // Replay a flight into it.
        let _ = update(&mut state, Message::RecordingSelected("rehearsal.csv".into()));
        let _ = update(&mut state, Message::StartReplay);
        assert!(state.replay.is_some(), "the replay did not start: {:?}", state.error_message);

        assert!(
            pump_for(&mut state, StdDuration::from_secs(30), |s| s
                .replay
                .as_ref()
                .is_some_and(|r| r.finished())),
            "the replay never finished: {:?}",
            state.replay.as_ref().map(|r| r.describe())
        );

        // The test receiver should see eight packets per sample. Waited for
        // rather than asserted at once: a finished replay means the last
        // sample was handed to the TCP thread, not that it has reached the
        // far end of a socket.
        assert!(
            pump_for(&mut state, StdDuration::from_secs(10), |s| s
                .test_receiver
                .as_ref()
                .is_some_and(|r| r.tally.packets >= 200 * 8)),
            "the receiver saw only {} packets of 1600",
            state.test_receiver.as_ref().map(|r| r.tally.packets).unwrap_or(0)
        );

        // And the window knows what the logger is doing, as data rather than
        // as text it had to scrape.
        let status = state
            .listener
            .live
            .status
            .as_ref()
            .unwrap_or_else(|| panic!("no status event arrived: {:?}", state.listener.recent_output(10)));
        assert_eq!(status.state, "recording", "{}", status.headline());
        assert!(status.rows > 0, "the logger reported no rows: {}", status.headline());
        assert!(
            state.listener.live.csv.contains("session_flight_data.csv"),
            "the file it writes was not reported: {:?}",
            state.listener.live.csv
        );

        // Let the logger write the last of what it was sent, then stop.
        let _ = pump_for(&mut state, StdDuration::from_secs(2), |_| false);
        let _ = update(&mut state, Message::StopFlight);
        assert!(!state.listener.status.is_running());
        assert!(!state.mirror_connected);

        // And the rows are in the CSV, which is the point of the whole thing.
        let csv = inference.join("data").join("session_flight_data.csv");
        let text = std::fs::read_to_string(&csv)
            .unwrap_or_else(|e| panic!("could not read {}: {e}", csv.display()));
        let rows = text.lines().filter(|line| line.contains(&flight_id)).count();
        assert!(
            rows >= 150,
            "only {rows} of 200 samples reached {}",
            csv.display()
        );

        let _ = update(&mut state, Message::StopTestReceiver);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
