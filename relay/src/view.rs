use iced::{
    widget::{button, column, container, text},
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

    let connection_status = match &state.connection_status {
        Some(channel_msg) => format!("{:?}", channel_msg), // Using debug formatting
        None => "No connection established".to_string(),
    };

    column![
        text(format!("Elapsed time: {:?}", state.elapsed_time)),
        text(baton_data),
        text(format!("Connection Staus: {}", connection_status)),
        button("Check Connection Status").on_press(Message::ConnectionMessage),
        // if we use containers, it boxes up the text elements and makes them easier to read
        container(text(baton_connect_status))
            .padding(10)
            .center(400)
            .style(container::rounded_box)
    ]
    .into()
}
