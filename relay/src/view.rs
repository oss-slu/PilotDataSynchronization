use std::net::ToSocketAddrs;

use iced::{
    widget::{button, column, container, row, text, text_input},
    Element,
};

use crate::{Message, State};

pub(crate) fn view(state: &State) -> Element<Message> {
    // need to update view function with float parsing? perhaps? idk
    let baton_data = match &state.latest_baton_send {
        Some(data) => format!("[BATON]: {data}"),
        None => "No data from baton.".into(),
    };

    let baton_connect_status = if state.active_baton_connection {
        format!("Baton Status: Connected")
    } else {
        format!("Baton Status: Disconnected")
    };

    let connection_status = state.tcp_connected;

    column![
        text(format!("Elapsed time: {:?}", state.elapsed_time)),
        text(baton_data),
        text(format!("TCP Connection Status: {}", connection_status)),
        button("Check Connection Status").on_press(Message::ConnectionMessage),
        // if we use containers, it boxes up the text elements and makes them easier to read
        container(text(baton_connect_status))
            .padding(10)
            .style(container::rounded_box),
        button("Connect IPC").on_press(Message::ConnectIpc),
        button("Disconnect IPC").on_press(Message::DisconnectIpc),
        if state.tcp_addr_field.to_socket_addrs().is_ok() {
            row![
                button("Connect TCP").on_press(Message::ConnectTcp),
                text_input("127.0.0.1:9999", &state.tcp_addr_field)
                    .on_input(|addr| Message::TcpAddrFieldUpdate(addr)),
                text("Address input is valid")
            ]
            .spacing(5)
        } else {
            row![
                button("Connect TCP"),
                text_input("127.0.0.1:9999", &state.tcp_addr_field)
                    .on_input(|addr| Message::TcpAddrFieldUpdate(addr)),
                text("Address input is invalid")
            ]
            .spacing(5)
        },
        button("Disconnect TCP").on_press(Message::DisconnectTcp),
    ]
    .padding(10)
    .into()
}
