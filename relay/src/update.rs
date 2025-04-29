use std::fs::File;
use std::io::prelude::*;
use iced::{time::Duration, Task};

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
            if let Some(status) = state.recv.as_ref().and_then(|recv| recv.try_recv().ok()){
                state.connection_status = Some(status)
            }                                                                                           
            Task::none() 
        },
        // Toggle messages for GUI XML generator
        M::AltitudeToggle(value) => {
            state.altitude_toggle = value;
            Task::none()
        },
        M::AirspeedToggle(value) => {
            state.altitude_toggle = value;
            Task::none()
        },
        M::VerticalAirspeedToggle(value) => {
            state.altitude_toggle = value;
            Task::none()
        },
        M::OtherToggle(value) => {
            state.altitude_toggle = value;
            Task::none()
        },
        M::CreateXMLFile => {
            create_xml_file(state)
        },
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

    // TODO: Check if all datarefs are false. If so, don't generate anything? or figure this out ngl

    // TODO: Replace this with actual xml file contents. Get from the XPlane API.
    let mut contents = String::from("xml document header\n");
    if state.altitude_toggle {
        // add altitude dataref to contents
        contents.push_str("Altitude\n");
    }
    if state.airspeed_toggle {
        // add airspeed dataref to contents
        contents.push_str("Airspeed\n");
    }
    if state.vertical_airspeed_toggle {
        // add vertical airspeed to contents
        contents.push_str("Vertical Airspeed\n");
    }
    if state.other_toggle {
        // add other_toggle dataref to contents
        contents.push_str("Other\n");
    }

    // Write contents to that file
    file.write_all(contents.as_bytes()).expect("Writing to XML file");   

    Task::none()    // Return that we need for the Update logic
}
