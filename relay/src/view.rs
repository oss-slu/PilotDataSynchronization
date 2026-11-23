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
pub(crate) fn view(state: &State) -> UIElement {
    let mut elements: Vec<UIElement> = Vec::new();

    // Optional error banner
    if let Some(err) = spawn_error_message(state) {
        elements.push(err);
    }

    // The whole path, so one glance says where it is broken
    elements.push(chain_strip(state));

    // Informational text
    elements.push(elapsed_time_element(state));
    elements.push(baton_data_element(state));
    elements.push(baton_connect_status_element(state));

    // Action buttons
    elements.push(send_packet_row(state));

    // TCP controls and status
    elements.push(tcp_connect_status_element(state));
    elements.push(check_tcp_status_button());

    // IPC controls
    elements.push(ipc_connect_button());
    elements.push(ipc_disconnect_button());

    // TCP connect/disconnect row
    elements.push(tcp_connect_button(state));
    elements.push(tcp_disconnect_button());

    // Second destination, read by the Python side
    elements.push(mirror_row(state));

    // The listener the relay runs, so a Flight needs no terminal
    elements.push(listener_panel(state));

    // Metrics (if enabled)
    elements.push(metrics_block(state));

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
    pick_list(
        state.saved_tcp_addrs.clone(),
        state.selected_tcp_addr.clone(),
        Message::SavedTcpAddrSelected,
    )
    .placeholder("Saved IPs")
    .into()
}

fn tcp_validation_text(state: &State) -> UIElement {
    if let Some(err) = &state.tcp_addr_validation_error {
        text(err).into()
    } else {
        text(" ").into()
    }
}

fn tcp_connect_button(state: &State) -> UIElement {
    column![
        row![
            button("Connect TCP").on_press(Message::ConnectTcp),
            tcp_addr_input(state),
            tcp_addr_dropdown(state),
        ]
        .spacing(5),
        tcp_validation_text(state),
    ]
    .spacing(5)
    .into()
}

fn tcp_disconnect_button() -> UIElement<'static> {
    button("Disconnect TCP")
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
    let plugin = if state.active_baton_connection {
        "X-Plane plugin: connected".to_string()
    } else {
        "X-Plane plugin: waiting. Start X-Plane, or if it is already running,          re-enable the plugin in Plugin Admin"
            .to_string()
    };

    let imotions = if state.tcp_connected {
        format!("iMotions: connected to {}", state.tcp_addr_field)
    } else {
        "iMotions: not connected. Type the address and press Connect TCP".to_string()
    };

    let listener = if state.listener.status.is_running() {
        match (state.listener.live.ready(), state.listener_mode) {
            (true, _) => format!(
                "Listener: {} recording {}",
                state.listener_mode, state.listener.live.flight_id
            ),
            (false, mode) => format!("Listener: {mode} starting"),
        }
    } else if state.mirror_connected {
        "Listener: a mirror is connected, but the relay did not start it".to_string()
    } else {
        "Listener: not running".to_string()
    };

    container(
        column![
            text("Flight path").size(14),
            text(plugin).size(12),
            text(imotions).size(12),
            text(listener).size(12),
        ]
        .spacing(2),
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
        row![
            text(state.listener.status.describe()),
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
    }

    // Say this before a start fails rather than after, since the fix is to
    // run the relay from the repository or start the listener by hand.
    if state.inference_dir.is_none() {
        rows.push(
            text("No inference folder found next to the relay, so the listener cannot start here.")
                .size(12)
                .into(),
        );
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

/// Start and stop are one button, because they are one decision and the panel
/// should never offer both at once.
fn listener_button(running: bool) -> UIElement<'static> {
    if running {
        button("Stop listener").on_press(Message::StopListener).into()
    } else {
        button("Start listener").on_press(Message::StartListener).into()
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
