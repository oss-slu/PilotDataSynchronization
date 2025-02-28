use iced::{
    widget::{column, text},
    Element,
};

use crate::{Message, State};

pub(crate) fn view(state: &State) -> Element<Message> {
    column![
        text("Hello, world."),
        text(format!("Elapsed time: {:?}", state.elapsed_time)),
        text(format!("Flicker is: {}", state.flicker))
    ]
    .into()
}
