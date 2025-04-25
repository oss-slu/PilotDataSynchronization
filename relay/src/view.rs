use iced::{
    widget::{button, column, container, text},
    Element,
};

use crate::{Message, State};

// Render the full UI view
pub(crate) fn view(state: &State) -> Element<Message> {
    column![
        render_elapsed_time(state),
        baton_text(state),
        render_connection_text(state),
        render_status_button(),
    ]
    .into()
}


// Renders elapsed time text
fn render_elapsed_time(state: &State) -> Element<Message> {
    text(format!("Elapsed time: {:?}", state.elapsed_time)).into()
}

// Renders baton elevation or no-data text
fn baton_text(state: &State) -> Element<Message> {
    let baton_data = match &state.latest_baton_send {
        Some(num) => format!("[Baton] Pilot Elevation: {num:.3} ft"),
        None => "No data from baton.".into(),
    };
    text(baton_data).into()
}

// Renders connection status text (normal connection status + baton connection)
fn render_connection_text(state: &State) -> Element<Message> {
    let connection_status = match &state.connection_status {
        Some(channel_msg) => format!("{:?}", channel_msg),
        None => "No connection established".to_string(),
    };

    let baton_connection_status = if state.active_baton_connection {
        format!(":) Baton Connected!")
    } else {
        format!(":( No Baton Connection")
    };

    // Combine connection status + baton connection in a small column
    container(
        column![
            text(format!("Connection Status: {}", connection_status)),
            text(baton_connection_status)
        ]
    )
    .padding(10)
    .center(400)
    .style(container::rounded_box)
    .into()
}

// Renders the button to check connection status
fn render_status_button<'a>() -> Element<'a, Message> {
    button("Check Connection Status")
        .on_press(Message::ConnectionMessage)
        .into()
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::State;
    use std::time::Duration;
    use crate::channel::ChannelMessage;

    fn dummy_state() -> State {
        State {
            latest_baton_send: Some(123.456.to_string()),
            active_baton_connection: true,
            connection_status: Some(ChannelMessage::Connect),
            elapsed_time: Duration::from_secs(42),
            ..Default::default()
        }
    }

    #[test]
    fn test_render_elapsed_time() {
        let state = dummy_state();
        let element = render_elapsed_time(&state);
        let _: Element<Message> = element;
    }

    #[test]
    fn test_baton_text_with_value() {
        let state = dummy_state();
        let element = baton_text(&state);
        let _: Element<Message> = element;
    }

    #[test]
    fn test_render_connection_text_connected() {
        let state = dummy_state();
        let element = render_connection_text(&state);
        let _: Element<Message> = element;
    }

    #[test]
    fn test_render_connection_text_disconnected() {
        let mut state = dummy_state();
        state.connection_status = None;
        state.active_baton_connection = false;
        let element = render_connection_text(&state);
        let _: Element<Message> = element;
    }

    #[test]
    fn test_render_status_button() {
        let element = render_status_button();
        let _: Element<Message> = element;
    }

    #[test]
    fn test_full_view() {
        let state = dummy_state();
        let element = view(&state);
        let _: Element<Message> = element;
    }
}