use iced::{
    widget::{button, column, container, text, toggler},
    Element,
};
use iced_aw::{helpers::card, style};

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
        // create_toggle_button(state),
        // create_xml_button(state),
        xml_downloader_popup(state),
        button("Check Connection Status").on_press(Message::ConnectionMessage),
        // if we use containers, it boxes up the text elements and makes them easier to read
        container(text(baton_connect_status))
            .padding(10)
            .center(400)
            .style(container::rounded_box),
    ]
    .into()
}


// fn create_toggle_button(state: &State) -> Element<Message> {
//     toggler(state.toggler_button_is_checked)
//         .label("Toggle Button!")
//         .on_toggle(Message::ToggleMessage)
//         .into()
// }

fn xml_downloader_popup(state: &State) -> Element<Message> {
    if state.card_open {
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
                    toggler(state.other_toggle)
                        .label("Other Toggle")
                        .on_toggle(Message::OtherToggle),
                    button("Generate XML File").on_press(Message::CreateXMLFile),
                ]
        )
            //.foot(text(format!("Foot")))
            .style(style::card::primary)
            .on_close(Message::CardClose)
            .into()
    } else {
        button("Open XML Download Menu")
            .on_press(Message::CardOpen)
            .into()
    }
}
