//! UI view definitions using iced.
//!
//! Each UI element is produced by a small focused function to improve readability,
//! testability and maintainability. The `view` function composes those elements.

use iced::widget::{
    button, column, container, pick_list, row, scrollable, text, text_input, toggler,
};
use iced::{Element, Length};
use iced_aw::{helpers::card, style};

use crate::listener::ListenerMode;
use crate::state::{IMOTIONS_ADDR_HINT, MIRROR_ADDR_HINT};
use crate::{Message, State};

type UIElement<'a> = Element<'a, Message>;

/// How many of the most recent `event_log` entries the GUI renders.
const EVENT_LOG_VISIBLE: usize = 5;

/// Compose the UI by collecting small, single-responsibility elements.
///
/// The order is the order a session happens in: whether the chain is whole,
/// where iMotions is, then the flight. The individual connect and disconnect
/// controls are recovery, so they sit behind Advanced rather than in front of
/// someone who only wants to record a flight (#285).
pub(crate) fn view(state: &State) -> UIElement {
    let mut elements: Vec<UIElement> = Vec::new();

    // Optional error banner
    if let Some(err) = spawn_error_message(state) {
        elements.push(err);
    }

    // The whole path, so one glance says where it is broken
    elements.push(chain_strip(state));

    // Where the telemetry is going
    elements.push(imotions_panel(state));

    // The listener the relay runs, so a Flight needs no terminal
    elements.push(listener_panel(state));

    // A recorded flight in place of the simulator
    elements.push(rehearse_panel(state));

    // The plumbing, for when something has to be put right by hand
    elements.push(advanced_toggle(state));
    if state.show_advanced {
        elements.push(advanced_panel(state));
    }

    // Recent event log
    elements.push(event_log_element(state));

    // XML download / card
    elements.push(xml_download_popup(state));

    // Scrollable, so a small screen or a long event log cannot hide the
    // controls underneath them.
    scrollable(column(elements).spacing(5).padding(10)).into()
}

fn spawn_error_message(state: &State) -> Option<UIElement> {
    state
        .error_message
        .as_ref()
        .map(|err| {
            container(text(format!("[WARN] {}", err)))
                .padding(10)
                .width(Length::Fill)
                .style(container::rounded_box)
                .center_x(Length::Fill)
                .into()
        })
}

fn elapsed_time_element(state: &State) -> UIElement {
    text(format!("Elapsed time: {:?}", state.elapsed_time)).into()
}

fn baton_connect_status_element(state: &State) -> UIElement {
    let connection_status = if state.active_baton_connection {
        ":) Baton Connected!".to_string()
    } else {
        ":( No Baton Connection".to_string()
    };
    text(connection_status).into()
}

fn baton_data_element(state: &State) -> UIElement {
    let content = match &state.latest_baton_send {
        Some(data) => format!("[BATON]: {}", data),
        None => "No data from baton.".into(),
    };
    text(content).into()
}

// Added this for tcp counter - Nyla Hughes
fn metrics_block(state: &State) -> UIElement {
    if !state.show_metrics {
        return text("").into();
    }

    let packets_60 = state.packets_last_60s;
    let bps_str = human_bps(state.bps);

    column![
        text(format!("Packets sent in the last 60's: {packets_60}")),
        text(format!("Throughput: {bps_str}")),
    ]
    .into()
}

/// Render the most recent `event_log` entries so connect/disconnect failures
/// are visible in the GUI instead of only being recorded in state.
fn event_log_element(state: &State) -> UIElement {
    if state.event_log.is_empty() {
        return text("").into();
    }

    // Newest first, so the latest failure is the line the user sees.
    let mut entries: Vec<UIElement> = vec![text("Recent events").size(14).into()];
    entries.extend(
        state
            .event_log
            .iter()
            .rev()
            .take(EVENT_LOG_VISIBLE)
            .map(|entry| -> UIElement { text(entry).size(12).into() }),
    );

    container(column(entries).spacing(2))
        .padding(10)
        .width(Length::Fill)
        .style(container::rounded_box)
        .into()
}

fn human_bps(bps: f64) -> String {
    const K: f64 = 1_000.0;
    if bps < K {
        return format!("{:.0} bps", bps);
    }
    let kbps = bps / K;
    if kbps < K {
        return format!("{:.1} Kbps", kbps);
    }
    let mbps = kbps / K;
    if mbps < K {
        return format!("{:.2} Mbps", mbps);
    }
    let gbps = mbps / K;
    format!("{:.2} Gbps", gbps)
}

fn tcp_connect_status_element(state: &State) -> UIElement {
    text(format!("TCP Connection Status: {}", state.tcp_connected)).into()
}

/// Simple helper: a button which triggers the app to verify the TCP connection state.
fn check_tcp_status_button() -> UIElement<'static> {
    button("Check TCP Connection Status")
        .on_press(Message::ConnectionMessage)
        .into()
}

/// IPC connect / disconnect buttons
fn ipc_connect_button() -> UIElement<'static> {
    button("Connect IPC").on_press(Message::ConnectIpc).into()
}

fn ipc_disconnect_button() -> UIElement<'static> {
    button("Disconnect IPC")
        .on_press(Message::DisconnectIpc)
        .into()
}

/// Build the TCP address input widget wired to `Message::TcpAddrFieldUpdate`.
fn tcp_addr_input(state: &State) -> UIElement {
    text_input(IMOTIONS_ADDR_HINT, &state.tcp_addr_field)
        .on_input(|addr| Message::TcpAddrFieldUpdate(addr))
        .into()
}

fn tcp_addr_dropdown(state: &State) -> UIElement {
    // Named rather than bare, so the list says which entry is this machine
    // and which is iMotions. The address is still shown: it is what someone
    // reads back to whoever set the lab up.
    let options: Vec<String> = state
        .saved_tcp_addrs
        .iter()
        .map(|address| State::label_for_tcp_addr(address))
        .collect();
    let selected = state
        .selected_tcp_addr
        .as_deref()
        .map(State::label_for_tcp_addr);

    pick_list(options, selected, Message::SavedTcpAddrSelected)
        .placeholder("Saved IPs")
        .into()
}

fn tcp_disconnect_button() -> UIElement<'static> {
    button("Disconnect iMotions")
        .on_press(Message::DisconnectTcp)
        .into()
}

/// Build the mirror address input, wired to `Message::MirrorAddrFieldUpdate`.
fn mirror_addr_input(state: &State) -> UIElement {
    text_input(MIRROR_ADDR_HINT, &state.mirror_addr_field)
        .on_input(Message::MirrorAddrFieldUpdate)
        .into()
}

fn mirror_status_element(state: &State) -> UIElement {
    let status = if state.mirror_connected {
        "Python mirror: connected"
    } else {
        "Python mirror: not connected"
    };
    text(status).into()
}

fn mirror_connect_button() -> UIElement<'static> {
    button("Connect mirror").on_press(Message::ConnectMirror).into()
}

fn mirror_disconnect_button() -> UIElement<'static> {
    button("Disconnect mirror")
        .on_press(Message::DisconnectMirror)
        .into()
}

/// Mirror controls: a second destination that receives a copy of every packet,
/// so the Python side can read the stream while iMotions keeps recording.
fn mirror_row(state: &State) -> UIElement {
    column![
        mirror_status_element(state),
        row![
            mirror_connect_button(),
            mirror_addr_input(state),
            mirror_disconnect_button(),
        ]
        .spacing(5),
    ]
    .spacing(5)
    .into()
}

/// How many lines of listener output the window shows.
const LISTENER_OUTPUT_VISIBLE: usize = 4;

/// One line per hop, so a broken Flight is read rather than deduced.
///
/// The order is the order the data travels, and each line says what is wrong
/// in terms of the thing to go and do, because the people running this are
/// researchers rather than developers.
fn chain_strip(state: &State) -> UIElement {
    let mut lines: Vec<UIElement> = vec![text("Flight path").size(14).into()];

    let source = match (&state.replay, state.active_baton_connection) {
        (Some(replay), _) if !replay.finished() => {
            format!("Flight data: {}", replay.describe())
        }
        (_, true) => "Flight data: the X-Plane plugin is connected".to_string(),
        _ => "Flight data: waiting for X-Plane. Start the simulator, or if it is already \
              running, re-enable the plugin in Plugin Admin. There is also a recorded flight \
              to replay below."
            .to_string(),
    };
    lines.push(text(source).size(12).into());

    let imotions = match (&state.test_receiver, state.tcp_connected) {
        (Some(receiver), true) if state.tcp_addr_field == receiver.addr() => {
            format!("iMotions: standing in with the test receiver on {}", receiver.addr())
        }
        (_, true) => format!("iMotions: connected to {}", state.tcp_addr_field),
        _ => "iMotions: not connected. Enter its address and press Connect iMotions, or use \
              the test receiver."
            .to_string(),
    };
    lines.push(text(imotions).size(12).into());

    let mode = state.listener_mode;
    let flight = if state.listener.live.flight_id.is_empty() {
        state.flight_id_field.trim().to_string()
    } else {
        state.listener.live.flight_id.clone()
    };
    let listener = if !state.listener.status.is_running() {
        "Listener: not running. Fill in the pilot and flight, then press Start flight."
            .to_string()
    } else if state.mirror_connected {
        format!("Listener: {mode} recording {flight}")
    } else if state.mirror_arming() {
        format!("Listener: {mode} starting, connecting it to the stream")
    } else {
        format!(
            "Listener: {mode} is running but is not being sent the stream. Stop the flight and \
             start it again."
        )
    };
    lines.push(text(listener).size(12).into());

    // Only when it is a problem. Said here because the listener cannot start
    // without it and the fix is an install, not a button.
    if state.uv_available == Some(false) {
        lines.push(
            text(
                "uv is not installed, so the relay cannot start a listener. Install uv, or run \
                 the listener from a terminal as before.",
            )
            .size(12)
            .into(),
        );
    }

    container(column(lines).spacing(2))
        .padding(10)
        .width(Length::Fill)
        .style(container::rounded_box)
        .into()
}

/// Where the relay sends, and a way to try it with nothing to send to.
fn imotions_panel(state: &State) -> UIElement {
    // One button rather than two. Connect is a dead end once it has worked,
    // and the way to close the socket deliberately was buried in the advanced
    // panel, where nobody running a session would look for it.
    let connect = if state.tcp_connected {
        button("Disconnect").on_press(Message::DisconnectTcp)
    } else {
        button("Connect iMotions").on_press(Message::ConnectTcp)
    };

    let mut rows: Vec<UIElement> = vec![
        text("iMotions").size(14).into(),
        row![connect, tcp_addr_input(state), tcp_addr_dropdown(state)]
        .spacing(5)
        .align_y(iced::Alignment::Center)
        .into(),
    ];

    if let Some(err) = &state.tcp_addr_validation_error {
        rows.push(text(err.clone()).size(12).into());
    }

    rows.push(match &state.test_receiver {
        Some(receiver) => row![
            button("Stop test receiver").on_press(Message::StopTestReceiver),
            text(receiver.describe()).size(12),
        ]
        .spacing(5)
        .align_y(iced::Alignment::Center)
        .into(),
        None => row![
            button("Use a test receiver").on_press(Message::UseTestReceiver),
            text("stands in for iMotions on this machine, for checking the chain").size(12),
        ]
        .spacing(5)
        .align_y(iced::Alignment::Center)
        .into(),
    });

    container(column(rows).spacing(5))
        .padding(10)
        .width(Length::Fill)
        .style(container::rounded_box)
        .into()
}

/// Replay a recorded flight in place of the simulator.
///
/// Here rather than in a script because rehearsing a session, or checking the
/// chain after moving a machine, is a thing the people running this do and a
/// terminal is what stopped them doing it.
fn rehearse_panel(state: &State) -> UIElement {
    let mut rows: Vec<UIElement> = vec![text("Without the simulator").size(14).into()];

    if state.recordings.is_empty() {
        rows.push(
            text("No recorded flights found in inference/data, so there is nothing to replay.")
                .size(12)
                .into(),
        );
        return container(column(rows).spacing(5))
            .padding(10)
            .width(Length::Fill)
            .style(container::rounded_box)
            .into();
    }

    let labels: Vec<String> = state.recordings.iter().map(|r| r.label()).collect();
    rows.push(
        row![
            text("Recording").size(12),
            pick_list(
                labels,
                state.selected_recording.clone(),
                Message::RecordingSelected,
            ),
        ]
        .spacing(5)
        .align_y(iced::Alignment::Center)
        .into(),
    );

    // Only once a recording has been picked, because that is when its flights
    // are read.
    let flights: Vec<String> = state
        .selected_recording
        .as_ref()
        .and_then(|label| state.recordings.iter().find(|r| &r.label() == label))
        .map(|recording| recording.flights.clone())
        .unwrap_or_default();

    let replaying = state.replay.as_ref().is_some_and(|r| !r.finished());
    let mut controls: Vec<UIElement> = Vec::new();
    if !flights.is_empty() {
        controls.push(text("Flight").size(12).into());
        controls.push(
            pick_list(
                flights,
                state.selected_replay_flight.clone(),
                Message::ReplayFlightSelected,
            )
            .into(),
        );
    }
    controls.push(if replaying {
        button("Stop replay").on_press(Message::StopReplay).into()
    } else {
        button("Replay this flight")
            .on_press(Message::StartReplay)
            .into()
    });
    rows.push(
        row(controls)
            .spacing(5)
            .align_y(iced::Alignment::Center)
            .into(),
    );

    if let Some(replay) = &state.replay {
        rows.push(text(replay.describe()).size(12).into());
    }

    container(column(rows).spacing(5))
        .padding(10)
        .width(Length::Fill)
        .style(container::rounded_box)
        .into()
}

fn advanced_toggle(state: &State) -> UIElement {
    let label = if state.show_advanced {
        "Hide the individual controls"
    } else {
        "Show the individual controls"
    };
    button(label).on_press(Message::ToggleAdvanced).into()
}

/// Every connection on its own, for putting something right by hand.
///
/// Hidden by default. These are the controls the window was made of before it
/// could run a Flight by itself, and they are still the way out when something
/// has to be reconnected without restarting anything.
fn advanced_panel(state: &State) -> UIElement {
    container(
        column![
            text("Individual controls").size(14),
            elapsed_time_element(state),
            baton_data_element(state),
            baton_connect_status_element(state),
            tcp_connect_status_element(state),
            send_packet_row(state),
            row![check_tcp_status_button(), tcp_disconnect_button()].spacing(5),
            row![ipc_connect_button(), ipc_disconnect_button()].spacing(5),
            mirror_row(state),
            metrics_block(state),
        ]
        .spacing(5),
    )
    .padding(10)
    .width(Length::Fill)
    .style(container::rounded_box)
    .into()
}

/// The live groups block: what the Instructor reads while flying.
fn live_groups_block(state: &State) -> Vec<UIElement> {
    let live = &state.listener.live;
    let mut rows: Vec<UIElement> = Vec::new();

    if let Some(agreement) = live.agreement {
        rows.push(
            text(format!(
                "Reference cohort fitted, agreement with the hidden tiers {agreement:.2}"
            ))
            .size(12)
            .into(),
        );
    }
    if let Some(note) = &live.note {
        rows.push(text(note.clone()).size(12).into());
    }

    if let Some(status) = &live.status {
        // The headline carries the group, so it is the biggest thing here.
        rows.push(text(status.headline()).size(16).into());
        if let Some(settling) = status.settling_text() {
            rows.push(text(settling).size(12).into());
        }
        for line in status.feature_lines() {
            rows.push(text(line).size(12).into());
        }
    }

    if let Some(next) = &live.next_instruction {
        rows.push(
            row![
                button("Give instruction").on_press(Message::GiveInstruction),
                text(format!("next: {next}")).size(12),
            ]
            .spacing(5)
            .align_y(iced::Alignment::Center)
            .into(),
        );
    } else if live.ready() {
        rows.push(text("No instructions left in the brief.").size(12).into());
    }

    rows
}

/// The listener panel: ids, a start/stop button, and the listener's own output.
///
/// It sits under the mirror controls because that is the order things happen
/// in: the listener reads the mirror port.
fn listener_panel(state: &State) -> UIElement {
    let running = state.listener.status.is_running();

    let mut rows: Vec<UIElement> = vec![
        text("Flight").size(14).into(),
        row![
            text(state.listener.status.describe()).size(12),
            pick_list(
                ListenerMode::ALL,
                Some(state.listener_mode),
                Message::ListenerModeSelected,
            ),
        ]
        .spacing(5)
        .align_y(iced::Alignment::Center)
        .into(),
        row![
            text("Pilot").size(12),
            text_input("P004", &state.pilot_id_field)
                .on_input(Message::PilotIdFieldUpdate),
            text("Flight").size(12),
            text_input("P004-T1", &state.flight_id_field)
                .on_input(Message::FlightIdFieldUpdate),
            listener_button(running),
        ]
        .spacing(5)
        .align_y(iced::Alignment::Center)
        .into(),
    ];

    if state.listener_mode == ListenerMode::LiveGroups {
        rows.push(
            row![
                text("Brief").size(12),
                text_input("briefs/example_level_holds.csv", &state.brief_field)
                    .on_input(Message::BriefFieldUpdate),
            ]
            .spacing(5)
            .align_y(iced::Alignment::Center)
            .into(),
        );
        rows.extend(live_groups_block(state));
    } else if let Some(status) = &state.listener.live.status {
        // The capture logger's own count. A connection being up says nothing
        // about rows reaching the file, and the count is what gets asked.
        rows.push(text(status.headline()).size(16).into());
        if !state.listener.live.csv.is_empty() {
            rows.push(text(format!("into {}", state.listener.live.csv)).size(12).into());
        }
    }

    // Say this before a start fails rather than after, since the fix is to
    // run the relay from the repository or start the listener by hand.
    if state.inference_dir.is_none() {
        rows.push(
            text("No inference folder found next to the relay, so the listener cannot start here.")
                .size(12)
                .into(),
        );
    } else if let Some(setup) = python_setup_row(state) {
        rows.push(setup);
    }

    let output = state.listener.recent_output(LISTENER_OUTPUT_VISIBLE);
    if !output.is_empty() {
        rows.extend(
            output
                .into_iter()
                .map(|line| -> UIElement { text(line.to_string()).size(12).into() }),
        );
    }

    container(column(rows).spacing(5))
        .padding(10)
        .width(Length::Fill)
        .style(container::rounded_box)
        .into()
}

/// The one-off setup, shown only while it is still needed.
///
/// A fresh clone has no Python environment, and `uv sync` in a terminal was
/// the last thing standing between the window and a working session. Once the
/// environment is there this row disappears, because it is not a thing to do
/// twice.
fn python_setup_row(state: &State) -> Option<UIElement> {
    let inference = state.inference_dir.as_ref()?;
    if crate::listener::venv_python(inference).is_some() {
        return None;
    }

    if state.setup.status.is_running() {
        let latest = state
            .setup
            .recent_output(1)
            .first()
            .map(|line| line.to_string())
            .unwrap_or_else(|| "working".to_string());
        return Some(
            column![
                text("Setting up the Python side. This happens once and takes a few minutes.")
                    .size(12),
                text(latest).size(12),
            ]
            .spacing(2)
            .into(),
        );
    }

    Some(
        column![
            text("The Python side has not been set up yet, so a flight cannot be recorded.")
                .size(12),
            row![
                button("Set up the Python side").on_press(Message::SetUpPython),
                text("downloads what the listener needs, once").size(12),
            ]
            .spacing(5)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(2)
        .into(),
    )
}

/// Start and stop are one button, because they are one decision and the panel
/// should never offer both at once.
///
/// It says flight rather than listener because starting one is two steps,
/// start the listener and connect it to the stream, and which of them is
/// which is not something the operator should have to hold in mind.
fn listener_button(running: bool) -> UIElement<'static> {
    if running {
        button("Stop flight").on_press(Message::StopFlight).into()
    } else {
        button("Start flight").on_press(Message::StartFlight).into()
    }
}

/// XML download card or opener button depending on `state.card_open`
fn xml_download_popup(state: &State) -> UIElement {
    if state.card_open {
        container(
            card(
                text("Download the XML File!"),
                column![
                    toggler(state.altitude_toggle)
                        .label("Altitude")
                        .on_toggle(Message::AltitudeToggle),
                    toggler(state.airspeed_toggle)
                        .label("Airspeed")
                        .on_toggle(Message::AirspeedToggle),
                    toggler(state.vertical_airspeed_toggle)
                        .label("Vertical Airspeed")
                        .on_toggle(Message::VerticalAirspeedToggle),
                    toggler(state.heading_toggle)
                        .label("Heading")
                        .on_toggle(Message::HeadingToggle),
                    toggler(state.roll_toggle)
                        .label("Roll")
                        .on_toggle(Message::RollToggle),
                    toggler(state.pitch_toggle)
                        .label("Pitch")
                        .on_toggle(Message::PitchToggle),
                    toggler(state.yaw_toggle)
                        .label("Yaw")
                        .on_toggle(Message::YawToggle),
                    toggler(state.gforce_toggle)
                        .label("G-Force")
                        .on_toggle(Message::GForceToggle),
                    button("Generate XML File").on_press(Message::CreateXMLFile),
                ],
            )
            .style(style::card::primary)
            .on_close(Message::CardClose),
        )
        .into()
    } else {
        button("Open XML Download Menu")
            .on_press(Message::CardOpen)
            .into()
    }
}

/// Send packet button: enabled variant wires the message, disabled variant is inert.
fn send_packet_button(state: &State) -> UIElement {
    if state.is_tcp_connected() {
        button("Send Packet").on_press(Message::SendPacket).into()
    } else {
        button("Send Packet (TCP Not Connected)").into()
    }
}

/// Timestamp of the last test packet, shown next to the button.
fn last_send_timestamp_element(state: &State) -> UIElement {
    let content = match &state.last_send_timestamp {
        Some(timestamp) => format!("Last test packet: {}", timestamp),
        None => "No test packet sent yet".to_string(),
    };
    text(content).into()
}

/// The Send Packet button with its timestamp beside it.
fn send_packet_row(state: &State) -> UIElement {
    row![
        send_packet_button(state),
        last_send_timestamp_element(state),
    ]
    .spacing(5)
    .align_y(iced::Alignment::Center)
    .into()
}
