use iced::{
    widget::{button, column, container, text, toggler},
    Element, Length,
};
use iced_aw::{helpers::card, style};

use crate::{Message, State};

type UIElement<'a> = Element<'a, Message>;

// TODO: Fix the close button on the UI card. It displays a chinese character meaning "plowed earth"?????
pub(crate) fn view(state: &State) -> UIElement {
    let mut elements: Vec<UIElement> = Vec::new();

    if let Some(error) = spawn_error_message(state) {
        elements.push(error.into());
    }

    elements.push(
        button("Check Connection Status")
            .on_press(Message::ConnectionMessage)
            .into(),
    );

    elements.push(text(format!("Elapsed time: {:?}", state.elapsed_time)).into());
    elements.push(text(get_baton_data(state)).into());
    elements.push(
        text(format!(
            "Connection Status: {}",
            get_connection_status(state)
        ))
        .into(),
    );

    // XML popup
    elements.push(xml_downloader_popup(state));

    // Final wrapped container with styling
    elements.push(
        container(text(get_baton_connect_status(state)))
            .padding(10)
            .center(400)
            .style(container::rounded_box)
            .into(),
    );

    // Create and return the GUI column from that vector
    column(elements).into()
}

fn get_baton_data(state: &State) -> String {
    // need to update view function with float parsing? perhaps? idk
    match &state.latest_baton_send {
        Some(num) => format!("[BATON] Pilot Elevation: {num:.3} ft"),
        None => "No data from baton.".into(),
    }
}

fn get_connection_status(state: &State) -> String {
    match &state.connection_status {
        Some(channel_msg) => format!("{:?}", channel_msg),
        None => "No connection established".to_string(),
    }
}

fn get_baton_connect_status(state: &State) -> String {
    if state.active_baton_connection {
        ":) Baton Connected!".to_string()
    } else {
        ":( No Baton Connection".to_string()
    }
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
