use iced::{
    widget::{button, column, container, text, Column, Text},
    {Element, Length},
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
        text(format!("Connection Status: {:?}", connection_status)),
        button("Check Connection Status").on_press(Message::ConnectionMessage),
        text("Iced test chart"),
        container(state.chart.view())
            .width(Length::Fill)
            .height(Length::Fixed(300.0))
            .center_x(Length::Fill)
            .center_y(Length::Shrink),
    ]
    .into()
}
