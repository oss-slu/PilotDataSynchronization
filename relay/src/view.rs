use iced::{
    widget::{button, column, text},
    Element,
};

use crate::{Message, State};

pub(crate) fn view(state: &State) -> Element<Message> {
    let baton_data = match state.latest_baton_send {
        Some(num) => format!("[BATON] Pilot Elevation: {num:.3} ft"),
        None => "No data from baton.".into(),
    };

    let connection_status = &state.connection_status;

    column![
        text(format!("Elapsed time: {:?}", state.elapsed_time)),
        text(baton_data),
        text(format!("Connection Staus: {:?}", connection_status)),
        button("Check Connection Status").on_press(Message::ConnectionMessage)
    ]
    .into()
}