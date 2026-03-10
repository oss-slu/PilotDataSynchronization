//! UI view definitions using iced.
//!
//! Each UI element is produced by a small focused function to improve readability,
//! testability and maintainability. The `view` function composes those elements.

use iced::widget::{button, column, container, pick_list, row, text, text_input, toggler};
use iced::{Element, Length};
use iced_aw::{helpers::card, style};

use crate::{Message, State};

type UIElement<'a> = Element<'a, Message>;

const DEFAULT_TCP_PLACEHOLDER: &str = "127.0.0.1:9999";

/// Compose the UI by collecting small, single-responsibility elements.
pub(crate) fn view(state: &State) -> UIElement {
    let mut elements: Vec<UIElement> = Vec::new();

    // Optional error banner
    if let Some(err) = spawn_error_message(state) {
        elements.push(err);
    }

    // Informational text
    elements.push(elapsed_time_element(state));
    elements.push(baton_data_element(state));
    elements.push(baton_connect_status_element(state));

    // Action buttons
    if let Some(btn) = send_packet_button(state) {
        elements.push(btn);
    }

    // TCP controls and status
    elements.push(tcp_connect_status_element(state));
    elements.push(check_tcp_status_button());

    // IPC controls
    elements.push(ipc_connect_button());
    elements.push(ipc_disconnect_button());

    // TCP connect/disconnect row
    elements.push(tcp_connect_button(state));
    elements.push(tcp_disconnect_button());

    // XML download / card
    elements.push(xml_download_popup(state));

    column(elements).into()
}

fn spawn_error_message(state: &State) -> Option<UIElement> {
    state
        .error_message
        .as_ref()
        .map(|err| {
            container(text(format!("⚠️ {}", err)))
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
    text_input(DEFAULT_TCP_PLACEHOLDER, &state.tcp_addr_field)
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
fn send_packet_button(state: &State) -> Option<UIElement> {
    if state.active_baton_connection {
        Some(button("Send Packet").on_press(Message::SendPacket).into())
    } else {
        Some(button("Send Packet (No Baton Connection)").into())
    }
}

/// Format bytes-per-second into a human-friendly string.
fn human_bps(bps: f64) -> String {
    if bps <= 0.0 {
        return "0 B/s".into();
    }
    if bps < 1024.0 {
        return format!("{:.0} B/s", bps);
    }
    let kb = bps / 1024.0;
    if kb < 1024.0 {
        return format!("{:.1} KB/s", kb);
    }
    let mb = kb / 1024.0;
    format!("{:.2} MB/s", mb)
}
