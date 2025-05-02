use iced::{time::Duration, Task};
use std::fs::File;
use std::io::prelude::*;

use crate::{IpcThreadMessage, Message, State};

pub(crate) fn update(state: &mut State, message: Message) -> Task<Message> {
    use Message as M;

    #[allow(unreachable_patterns)]
    match message {
        M::Update => {
            state.elapsed_time += Duration::from_secs(1);
            Task::none()
        }
        M::WindowCloseRequest(id) => {
            // pre-shutdown operations go here
            if let Some(ref tx) = state.tx_kill {
                let _ = tx.send(());
            }

            // delete socket file
            let socket_file_path = if cfg!(target_os = "macos") {
                "/tmp/baton.sock"
            } else {
                // TODO: add branch for Windows; mac branch is just for testing/building
                panic!(
                    "No implementation available for given operating system: {}",
                    std::env::consts::OS
                )
            };
            std::fs::remove_file(socket_file_path).unwrap();

            // necessary to actually shut down the window, otherwise the close button will appear to not work
            iced::window::close(id)
        }
        M::BatonMessage => {
            // if we get a message from the ipc_connection_thread, do something with it
            match state.rx_baton.as_ref().and_then(|rx| rx.try_recv().ok()) {
                Some(IpcThreadMessage::BatonData(data)) => {
                    state.latest_baton_send = Some(data);
                    state.active_baton_connection = true;
                }
                Some(IpcThreadMessage::BatonShutdown) => state.active_baton_connection = false,
                None => { /* do nothing */ }
            };
            Task::none()
        }
        M::ConnectionMessage => {
            println!("Check Connection Status");
            if let Some(status) = state.recv.as_ref().and_then(|recv| recv.try_recv().ok()) {
                state.connection_status = Some(status)
            }
            Task::none()
        }
        // Toggle messages for GUI XML generator
        M::AltitudeToggle(value) => {
            state.altitude_toggle = value;
            Task::none()
        }
        M::AirspeedToggle(value) => {
            state.airspeed_toggle = value;
            Task::none()
        }
        M::VerticalAirspeedToggle(value) => {
            state.vertical_airspeed_toggle = value;
            Task::none()
        }
        M::HeadingToggle(value) => {
            state.heading_toggle = value;
            Task::none()
        }
        M::CreateXMLFile => create_xml_file(state),
        // Card Open/Close messages for GUI pop-up-card window
        M::CardOpen => {
            state.card_open = true;
            Task::none()
        }
        M::CardClose => {
            state.card_open = false;
            Task::none()
        }
        _ => Task::none(),
    }
}

// Creates a default XML file when a button is clicked in the GUI
fn create_xml_file(state: &mut State) -> Task<Message> {
    // FIXME: How to make this file show up in the Downloads folder? Or do I want to just have file download in project root?
    let mut file = File::create("iMotions.xml").expect("Creating XML File");

    // Check if all dataref toggles are false. If so, return error message
    if !state.altitude_toggle
        && !state.airspeed_toggle
        && !state.vertical_airspeed_toggle
        && !state.heading_toggle
    {
        state.error_message = Some("Please select at least one dataref toggle".into());
        return Task::none();
    }
    state.error_message = None; // Clear previous error

    // TODO: Replace this with actual xml file contents. Get from the XPlane API.
    let mut contents = String::from(
        "<EventSource Version=\"1\" Id=\"PilotDataSync\" Name=\"Positional Flight Data\">\n",
    );
    if state.altitude_toggle {
        let mut altitude_str =
            String::from("\t<Sample Id=\"AltitudeSync\" Name=\"Altitude Synchronization\">\n");

        altitude_str.push_str(
            "\t\t<Field Id=\"FlightModelAltitude\" Range=\"Variable\" Min=\"0\" Max=\"50000\" />\n",
        );
        altitude_str.push_str(
            "\t\t<Field Id=\"PilotAltitude\" Range=\"Variable\" Min=\"0\" Max=\"50000\" />\n",
        );
        altitude_str.push_str("\t</Sample>\n");

        contents.push_str(&altitude_str);
    }
    if state.airspeed_toggle {
        let mut airspeed_str =
            String::from("\t<Sample Id=\"AirspeedSync\" Name=\"Airspeed Synchronization\">\n");

        airspeed_str.push_str(
            "\t\t<Field Id=\"FlightModelAirspeed\" Range=\"Variable\" Min=\"0\" Max=\"600\" />\n",
        );
        airspeed_str.push_str(
            "\t\t<Field Id=\"PilotAirspeed\" Range=\"Variable\" Min=\"0\" Max=\"600\" />\n",
        );
        airspeed_str.push_str("\t</Sample>\n");

        contents.push_str(&airspeed_str);
    }
    if state.vertical_airspeed_toggle {
        let mut vertical_airspeed_str = String::from(
            "\t<Sample Id=\"VerticalVelocitySync\" Name=\"Vertical Velocity Synchronization\">\n",
        );

        vertical_airspeed_str.push_str("\t\t<Field Id=\"FlightModelVerticalVelocity\" Range=\"Variable\" Min=\"-5000\" Max=\"5000\" />\n");
        vertical_airspeed_str.push_str("\t\t<Field Id=\"PilotVerticalVelocity\" Range=\"Variable\" Min=\"-5000\" Max=\"5000\" />\n");
        vertical_airspeed_str.push_str("\t</Sample>\n");

        contents.push_str(&vertical_airspeed_str);
    }
    if state.heading_toggle {
        let mut heading_str =
            String::from("\t<Sample Id=\"HeadingSync\" Name=\"Heading Synchronization\">\n");

        heading_str.push_str(
            "\t\t<Field Id=\"FlightModelHeading\" Range=\"Variable\" Min=\"0\" Max=\"360\" />\n",
        );
        heading_str.push_str(
            "\t\t<Field Id=\"PilotHeading\" Range=\"Variable\" Min=\"0\" Max=\"360\" />\n",
        );
        heading_str.push_str("\t</Sample>\n");

        contents.push_str(&heading_str);
    }

    // Write contents to that file
    file.write_all(contents.as_bytes())
        .expect("Writing to XML file");

    Task::none() // Return that we need for the Update logic
}
