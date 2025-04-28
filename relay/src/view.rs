use iced::{
    widget::{button, column, container, text, text_input},
    Element,
};

use crate::{Message, State};

pub(crate) fn view(state: &State) -> Element<Message> {
    // need to update view function with float parsing? perhaps? idk
    let baton_data = match &state.latest_baton_send {
        Some(num) => format!("[BATON] Pilot Elevation: {num:.3} ft"),
        None => "No data from baton.".into(),
    };

    // make this better. Perhaps add a funny emoji or sm
    let baton_connect_status = if state.active_baton_connection {
        format!(":) Baton Connected!")
    } else {
        format!(":( No Baton Connection")
    };

    let connection_status = state.tcp_connected;

    column![
        text(format!("Elapsed time: {:?}", state.elapsed_time)),
        text(baton_data),
        text(format!("Connection Staus: {}", connection_status)),
        button("Check Connection Status").on_press(Message::ConnectionMessage),
        // if we use containers, it boxes up the text elements and makes them easier to read
        container(text(baton_connect_status))
            .padding(10)
            .center(400)
            .style(container::rounded_box),
        text_input("127.0.0.1:9999", &state.tcp_addr_field)
            .on_input(|addr| Message::TcpAddrFieldUpdate(addr)),
        button("Connect IPC").on_press(Message::ConnectIpc),
        button("Disconnect IPC").on_press(Message::DisconnectIpc),
        button("Connect TCP").on_press(Message::ConnectTcp),
        button("Disconect TCP").on_press(Message::DisconnectTcp),
    ]
    .into()
}
