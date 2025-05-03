use iced::{time::Duration, Task};
use std::fs::File;
use std::io::prelude::*;

use crate::{message::ToTcpThreadMessage, FromIpcThreadMessage, Message, State};

pub(crate) fn update(state: &mut State, message: Message) -> Task<Message> {
    use Message as M;

    #[allow(unreachable_patterns)]
    match message {
        M::Update => {
            state.elapsed_time += Duration::from_millis(10);

            // check for messages from IPC thread
            if let Some(ipc_bichannel) = &state.ipc_bichannel {
                for message in ipc_bichannel.received_messages() {
                    match message {
                        FromIpcThreadMessage::BatonData(data) => {
                            state.tcp_bichannel.as_mut().map(|tcp_bichannel| {
                                tcp_bichannel.send_to_child(ToTcpThreadMessage::Send(data.clone()))
                            });
                            state.latest_baton_send = Some(data);
                            state.active_baton_connection = true;
                        }
                        FromIpcThreadMessage::BatonShutdown => {
                            let _ = state.tcp_disconnect();
                            state.active_baton_connection = false;
                        }
                        _ => (),
                    }
                }
            }

            // check for messages from TCP thread
            if let Some(tcp_bichannel) = &state.tcp_bichannel {
                for message in tcp_bichannel.received_messages() {
                    match message {
                        _ => (),
                    }
                }
            }

            Task::none()
        }
        M::WindowCloseRequest(id) => {
            // pre-shutdown operations go here
            if let Some(ref bichannel) = state.ipc_bichannel {
                let _ = bichannel.killswitch_engage();
            }

            if let Some(ref bichannel) = state.tcp_bichannel {
                let _ = bichannel.killswitch_engage();
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
        M::ConnectionMessage => {
            if let Some(status) = state
                .tcp_bichannel
                .as_ref()
                .and_then(|bichannel| bichannel.is_conn_to_endpoint().ok())
            {
                state.tcp_connected = status
            } else {
                state.tcp_connected = false
            }
            Task::none()
        }
        M::ConnectIpc => {
            if let Err(e) = state.ipc_connect() {
                state.log_event(format!("Error: {e:?}"));
            };
            Task::none()
        }
        M::DisconnectIpc => {
            if let Err(e) = state.ipc_disconnect() {
                state.log_event(format!("Error: {e:?}"));
            };
            Task::none()
        }
        M::ConnectTcp => {
            let address = state.tcp_addr_field.clone();
            if let Err(e) = state.tcp_connect(address) {
                state.log_event(format!("Error: {e:?}"));
            };
            Task::none()
        }
        M::DisconnectTcp => {
            if let Err(e) = state.tcp_disconnect() {
                state.log_event(format!("Error: {e:?}"));
            };
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
        M::TcpAddrFieldUpdate(addr) => {
            // Update the TCP address text input in the GUI
            let is_chars_valid = addr.chars().all(|c| c.is_numeric() || c == '.' || c == ':');
            let dot_count = addr.chars().filter(|&c| c == '.').count();
            let colon_count = addr.chars().filter(|&c| c == ':').count();
            if is_chars_valid && dot_count <= 3 && colon_count <= 1 {
                state.tcp_addr_field = addr;
            }
            Task::none()
        }
        _ => Task::none(),
    }
}

// Creates a default XML file when a button is clicked in the GUI
fn create_xml_file(state: &mut State) -> Task<Message> {
    // Get the user's downloads directory
    let mut downloads_path =
        dirs::download_dir().expect("Retrieving the user's Downloads file directory.");
    downloads_path.push("iMotions.xml");

    // Create file in downloads directory. If alr there, will overwrite the existing file.
    let mut file = File::create(&downloads_path).expect("Creating XML File.");

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

    // NOTE: This XML formatting was found in the PilotDataSync Slack. Double check this is the correct formatting.
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
    contents.push_str("</EventSource>");

    // Write XML file
    file.write_all(contents.as_bytes())
        .expect("Writing to XML file");

    Task::none() // Return type that we need for the Update logic
}
