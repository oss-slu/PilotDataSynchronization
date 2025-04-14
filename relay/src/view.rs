use iced::{
    widget::{button, column, text},
    Element,
};

use crate::{Message, State};

pub(crate) fn view(state: &State) -> Element<Message> {
    let baton_data = match &state.latest_baton_send {
        Some(s) => format!("[BATON] Packet bundle: {s}"),
        None => "No data from baton.".into(),
    };

    let connection_status = &state.tcp_connection_status;

    column![
        text(format!("Elapsed time: {:?}", state.elapsed_time)),
        text(baton_data),
        text(format!("Connection Staus: {:?}", connection_status)),
        button("Check Connection Status").on_press(Message::ConnectionMessage)
    ]
    .into()
}
