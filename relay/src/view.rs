//! This module defines the UI View layout using ICED.
//!
//! UI elements should be defined as **separate functions** and added to the UI elements vector
//! instead of being written inline in the `view` function.
//!
//! This improves modularity, testing, and code readability.
//!
//! Examples:
//!     ```fn spawn_error_message(state: &State) -> Option<UIElement> { /* ... */ } ```
//!     ``` if let Some(error_element) = spawn_error_message(state) {
//!             elements.push(error_element.into());
//!         }   ```
//!
//!     ``` fn ipc_disconnect_button(_state: &State) -> UIElement {/* ... */}   ```
//!     ``` elements.push(tcp_disconnect_button(state)); ```

use std::net::ToSocketAddrs;

use iced::{
    widget::{button, column, container, row, text, text_input, toggler},
    Element, Length,
};
use iced_aw::{helpers::card, style};

use crate::{Message, State};

type UIElement<'a> = Element<'a, Message>;

// TODO: Fix the close button on the UI card. It displays a chinese character meaning "plowed earth"?????
pub(crate) fn view(state: &State) -> UIElement {
    let mut elements: Vec<UIElement> = Vec::new();

    // OPTIONAL Error message
    if let Some(error_element) = spawn_error_message(state) {
        elements.push(error_element.into());
    }

    // Elapsed Time Text
    elements.push(elapsed_time_element(state));

    // Baton Latest Send Text
    elements.push(baton_data_element(state));
    elements.push(baton_connect_status_element(state));

    //added for tcp packet count -Nyla Hughes
    //elements.push(tcp_packet_count_element(state));
    // Clicks in the last 60 seconds -Nyla Hughes
    elements.push(clicks_last_60s_element(state));
    //

    // TCP Connection Status elements
    elements.push(tcp_connect_status_element(state));
    elements.push(check_tcp_status_button(state));

    // IPC Connect/Disconnect Buttons
    elements.push(ipc_connect_button(state));
    elements.push(ipc_disconnect_button(state));

    // TCP Connect/Disconnect buttons
    elements.push(tcp_connect_button(state));
    elements.push(tcp_disconnect_button(state));

    // XML popup
    elements.push(xml_downloader_popup(state));

    // Create and return the GUI column from that vector
    column(elements).into()
}

fn spawn_error_message(state: &State) -> Option<UIElement> {
    if let Some(error) = &state.error_message {
        Some(
            container(text(format!("⚠️ {}", error)))
                .padding(10)
                .width(Length::Fill)
                .style(container::rounded_box)
                .center_x(Length::Fill)
                .into(),
        )
    } else {
        None
    }
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
    // need to update view function with float parsing? perhaps? idk
    let baton_data = match &state.latest_baton_send {
        Some(data) => format!("[BATON]: {data:.3}"),
        None => "No data from baton.".into(),
    };
    text(baton_data).into()
}

//*  amount of packets sent over TCP connection -Nyla Hughes
// fn tcp_packet_count_element(state: &State) -> UIElement {
   // text(format!("TCP Packets Sent:")).to_string()}
    // will look something like text(format!("TCP Packets Sent: {}", state.tcp_packet_count)).to_string()
    // Once implemented -Nyla Hughes *//
//This just tracks clicks in the last 60 seconds to make sure the counting works -Nyla Hughes
fn clicks_last_60s_element(state: &State) -> UIElement {
    text(format!("'packets' sent in the last 60s: {}", state.clicks_last_60s)).into()
}
//



fn tcp_connect_status_element(state: &State) -> UIElement {
    text(format!("TCP Connection Status: {}", state.tcp_connected)).into()
}

fn check_tcp_status_button(_state: &State) -> UIElement {
    button("Check TCP Connection Status")
        .on_press(Message::ConnectionMessage)
        .into()
}

fn ipc_connect_button(_state: &State) -> UIElement {
    button("Connect IPC").on_press(Message::ConnectIpc).into()
}

fn ipc_disconnect_button(_state: &State) -> UIElement {
    button("Disconnect IPC")
        .on_press(Message::DisconnectIpc)
        .into()
}

fn tcp_connect_button(state: &State) -> UIElement {
    if state.tcp_addr_field.to_socket_addrs().is_ok() {
        row![
            button("Connect TCP").on_press(Message::ConnectTcp),
            text_input("127.0.0.1:9999", &state.tcp_addr_field)
                .on_input(|addr| Message::TcpAddrFieldUpdate(addr)),
            text("Address input is valid")
        ]
        .spacing(5)
        .into()
    } else {
        row![
            button("Connect TCP"),
            text_input("127.0.0.1:9999", &state.tcp_addr_field)
                .on_input(|addr| Message::TcpAddrFieldUpdate(addr)),
            text("Address input is invalid")
        ]
        .spacing(5)
        .into()
    }
}

fn tcp_disconnect_button(_state: &State) -> UIElement {
    button("Disconnect TCP")
        .on_press(Message::DisconnectTcp)
        .into()
}

fn xml_downloader_popup(state: &State) -> UIElement {
    if state.card_open {
        container(
            card(
                // FIXME: reword these toggles to actually be snappy wording
                text(format!("Download the XML File!")),
                column![
                    toggler(state.altitude_toggle)
                        .label("Altitude Toggle!")
                        .on_toggle(Message::AltitudeToggle),
                    toggler(state.airspeed_toggle)
                        .label("Airspeed Toggle")
                        .on_toggle(Message::AirspeedToggle),
                    toggler(state.vertical_airspeed_toggle)
                        .label("Vertical Airspeed Toggle")
                        .on_toggle(Message::VerticalAirspeedToggle),
                    toggler(state.heading_toggle)
                        .label("Heading Toggle")
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
