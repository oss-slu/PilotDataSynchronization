use std::net::ToSocketAddrs;

use iced::{
    widget::{button, column, row, container, text, text_input, toggler},
    Element, Length,
};
use iced_aw::{helpers::card, style};

use crate::{Message, State};

type UIElement<'a> = Element<'a, Message>;

// TODO: Stick all UI elements into their own functions and make ALL functions return UIElement and NOT strings
// TODO: Fix the close button on the UI card. It displays a chinese character meaning "plowed earth"?????
pub(crate) fn view(state: &State) -> UIElement {
    let mut elements: Vec<UIElement> = Vec::new();

    // OPTIONAL Error message
    if let Some(error) = spawn_error_message(state) {
        elements.push(error.into());
    }

    // Elapsed Time Text
    elements.push(text(format!("Elapsed time: {:?}", state.elapsed_time)).into());

    // Baton Latest Send Text 
    elements.push(text(get_baton_data(state)).into());
    // Unnecessary????? or should this be in here? idk man
    // // Final wrapped container with styling
    // elements.push(
    //     container(text(get_baton_connect_status(state)))
    //         .padding(10)
    //         .center(400)
    //         .style(container::rounded_box)
    //         .into(),
    // );


    // TCP Connection Status elements
    elements.push(text(format!("TCP Connection Status: {}", state.tcp_connected)).into());
    elements.push(button("Check TCP Connection Status").on_press(Message::ConnectionMessage).into());

    // IPC Connect/Disconnect Buttons
    elements.push(button("Connect IPC").on_press(Message::ConnectIpc).into());
    elements.push(button("Disconnect IPC").on_press(Message::DisconnectIpc).into());

    // TCP Connect/Disconnect buttons
    elements.push(tcp_connect_button(state));
    elements.push(button("Disconnect TCP").on_press(Message::DisconnectTcp).into());

    // XML popup
    elements.push(xml_downloader_popup(state));

    // Create and return the GUI column from that vector
    column(elements).into()
}

// Perhaps unnecessary with new changes?
fn get_baton_connect_status(state: &State) -> String {
    if state.active_baton_connection {
        ":) Baton Connected!".to_string()
    } else {
        ":( No Baton Connection".to_string()
    }
}

fn get_baton_data(state: &State) -> String {
    // need to update view function with float parsing? perhaps? idk
    match &state.latest_baton_send {
        Some(data) => format!("[BATON]: {data:.3}"),
        None => "No data from baton.".into(),
    }
}

fn get_tcp_connect_status(state: &State) -> String {
    format!("TCP Connection Status: {}", state.tcp_connected)
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
