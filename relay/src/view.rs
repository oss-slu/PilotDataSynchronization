use iced::{
    widget::{column, text, button},
    Element,
};

use crate::{Message, State};

pub(crate) fn view(state: &State) -> Element<Message> {
    let baton_data = match state.latest_baton_send {
        Some(num) => format!("[BATON] Pilot Elevation: {num:.3} ft"),
        None => "No data from baton.".into(),
    };

    column![
        text(format!("Elapsed time: {:?}", state.elapsed_time)),
        text(baton_data),
        // create element for baton connection status with a funny emoji or something

        // create button for reconnect
        button("Reset Baton Connection!").on_press(Message::BatonReconnectMessage)
    ]
    .into()
}
