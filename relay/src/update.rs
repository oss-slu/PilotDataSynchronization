use iced::{time::Duration, Task};
use std::fs::File;
use std::io::Write;
use std::time::{Duration as StdDuration, Instant};

use crate::{
    message::FromTcpThreadMessage, message::FromIpcThreadMessage, message::ToTcpThreadMessage, Message, State,
};

fn connect_tcp_with_validation_and_save(state: &mut State, address: String) {
    let trimmed = address.trim().to_string();
    match State::validate_tcp_addr(&trimmed) {
        Ok(()) => {
            state.tcp_addr_validation_error = None;
            state.tcp_addr_field = trimmed.clone();
            if let Err(e) = state.tcp_connect(trimmed.clone()) {
                state.log_event(format!("TCP connect failed: {}", e));
            } else if let Err(e) = state.save_tcp_addr_if_new(&trimmed) {
                state.log_event(format!("Saving TCP address failed: {}", e));
            }
        }
        Err(e) => {
            state.tcp_addr_validation_error = Some(e.to_string());
        }
    }
}

pub(crate) fn update(state: &mut State, message: Message) -> Task<Message> {
    use Message as M;

    const LAST_SEEN_WINDOW: StdDuration = StdDuration::from_secs(2);

    match message {
        M::Update => {
            state.elapsed_time += Duration::from_millis(10);

            // compute active baton connection using centralized helpers
            let ipc_conn_flag = state.is_ipc_connected();
            let recent_packet = state
                .last_baton_instant
                .map(|t| t.elapsed() <= LAST_SEEN_WINDOW)
                .unwrap_or(false);
            state.active_baton_connection = ipc_conn_flag || recent_packet;

            // process IPC messages
            if let Some(ipc_bichannel) = &state.ipc_bichannel {
                for msg in ipc_bichannel.received_messages() {
                    match msg {
                        FromIpcThreadMessage::BatonData(data) => {
                            if let Some(tcp_bi) = state.tcp_bichannel.as_mut() {
                                let _ = tcp_bi.send_to_child(ToTcpThreadMessage::Send(data.clone()));
                            }
                            state.log_event(format!("Baton packet: {}", data));
                            state.latest_baton_send = Some(data);
                            state.last_baton_instant = Some(Instant::now());
                            state.active_baton_connection = true;
                        }
                        FromIpcThreadMessage::BatonShutdown => {
                            let _ = state.tcp_disconnect();
                            state.active_baton_connection = false;
                        }
                    }
                }
            }

            // process TCP messages
            if let Some(tcp_bichannel) = &state.tcp_bichannel {
                for msg in tcp_bichannel.received_messages() {
                    match msg {
                        FromTcpThreadMessage::Connected => {
                            state.log_event("TCP connected to iMotions".into());
                            state.tcp_connected = true;
                        }
                        FromTcpThreadMessage::Disconnected(reason) => {
                            state.log_event(format!("TCP disconnected: {}", reason));
                            state.tcp_connected = false;
                        }
                        FromTcpThreadMessage::Sent(bytes) => {
                            state.on_tcp_packet_sent(bytes);
                        }
                        FromTcpThreadMessage::SendError(err) => {
                            state.log_event(format!("TCP send error: {}", err));
                        }
                    }
                }
            }

            Task::none()
        }
        M::WindowCloseRequest(id) => {
            if let Some(ref bichannel) = state.ipc_bichannel {
                let _ = bichannel.killswitch_engage();
            }
            if let Some(ref bichannel) = state.tcp_bichannel {
                let _ = bichannel.killswitch_engage();
            }

            // remove unix socket file on macos test/dev path only
            if cfg!(target_os = "macos") {
                let _ = std::fs::remove_file("/tmp/baton.sock");
            }

            iced::window::close(id)
        }
        M::ConnectionMessage => {
            state.tcp_connected = state.is_tcp_connected();
            Task::none()
        }
        M::ConnectIpc => {
            if let Err(e) = state.ipc_connect() {
                state.log_event(format!("IPC connect failed: {}", e));
            }
            Task::none()
        }
        M::DisconnectIpc => {
            if let Err(e) = state.ipc_disconnect() {
                state.log_event(format!("IPC disconnect failed: {}", e));
            }
            Task::none()
        }
        M::ConnectTcp => {
            let address = state.tcp_addr_field.clone();
            connect_tcp_with_validation_and_save(state, address);
            Task::none()
        }
        M::SavedTcpAddrSelected(address) => {
            state.selected_tcp_addr = Some(address.clone());
            connect_tcp_with_validation_and_save(state, address);
            Task::none()
        }
        M::DisconnectTcp => {
            if let Err(e) = state.tcp_disconnect() {
                state.log_event(format!("TCP disconnect failed: {}", e));
            }
            Task::none()
        }
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
        M::CardOpen => {
            state.card_open = true;
            Task::none()
        }
        M::CardClose => {
            state.card_open = false;
            Task::none()
        }
        M::TcpAddrFieldUpdate(addr) => {
            state.tcp_addr_field = addr;
            state.tcp_addr_validation_error = None;
            Task::none()
        }
        M::SendPacket => {
            let now = std::time::SystemTime::now();
            let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap();
            state.last_send_timestamp = Some(format!("{}", duration.as_secs()));
            Task::none()
        }
    }
}

fn create_xml_file(state: &mut State) -> Task<Message> {
    let mut downloads_path = match dirs::download_dir() {
        Some(p) => p,
        None => {
            let msg = "Could not determine Downloads directory".to_string();
            state.error_message = Some(msg.clone());
            state.log_event(msg);
            return Task::none();
        }
    };
    downloads_path.push("iMotions.xml");

    // Validate toggles
    if !state.altitude_toggle && !state.airspeed_toggle && !state.vertical_airspeed_toggle && !state.heading_toggle {
        state.error_message = Some("Please select at least one dataref toggle".into());
        return Task::none();
    }
    state.error_message = None;

    let mut contents = String::from("<EventSource Version=\"1\" Id=\"PilotDataSync\" Name=\"Positional Flight Data\">\n");
    if state.altitude_toggle {
        contents.push_str("\t<Sample Id=\"AltitudeSync\" Name=\"Altitude Synchronization\">\n");
        contents.push_str("\t\t<Field Id=\"FlightModelAltitude\" Range=\"Variable\" Min=\"0\" Max=\"50000\" />\n");
        contents.push_str("\t\t<Field Id=\"PilotAltitude\" Range=\"Variable\" Min=\"0\" Max=\"50000\" />\n");
        contents.push_str("\t</Sample>\n");
    }
    if state.airspeed_toggle {
        contents.push_str("\t<Sample Id=\"AirspeedSync\" Name=\"Airspeed Synchronization\">\n");
        contents.push_str("\t\t<Field Id=\"FlightModelAirspeed\" Range=\"Variable\" Min=\"0\" Max=\"600\" />\n");
        contents.push_str("\t\t<Field Id=\"PilotAirspeed\" Range=\"Variable\" Min=\"0\" Max=\"600\" />\n");
        contents.push_str("\t</Sample>\n");
    }
    if state.vertical_airspeed_toggle {
        contents.push_str("\t<Sample Id=\"VerticalVelocitySync\" Name=\"Vertical Velocity Synchronization\">\n");
        contents.push_str("\t\t<Field Id=\"FlightModelVerticalVelocity\" Range=\"Variable\" Min=\"-5000\" Max=\"5000\" />\n");
        contents.push_str("\t\t<Field Id=\"PilotVerticalVelocity\" Range=\"Variable\" Min=\"-5000\" Max=\"5000\" />\n");
        contents.push_str("\t</Sample>\n");
    }
    if state.heading_toggle {
        contents.push_str("\t<Sample Id=\"HeadingSync\" Name=\"Heading Synchronization\">\n");
        contents.push_str("\t\t<Field Id=\"FlightModelHeading\" Range=\"Variable\" Min=\"0\" Max=\"360\" />\n");
        contents.push_str("\t\t<Field Id=\"PilotHeading\" Range=\"Variable\" Min=\"0\" Max=\"360\" />\n");
        contents.push_str("\t</Sample>\n");
    }
    contents.push_str("</EventSource>");

    match File::create(&downloads_path) {
        Ok(mut file) => {
            if let Err(e) = file.write_all(contents.as_bytes()) {
                let msg = format!("Writing XML file failed: {}", e);
                state.error_message = Some(msg.clone());
                state.log_event(msg);
            } else {
                state.log_event(format!("XML file written to {}", downloads_path.display()));
            }
        }
        Err(e) => {
            let msg = format!("Creating XML file failed: {}", e);
            state.error_message = Some(msg.clone());
            state.log_event(msg);
        }
    }

    Task::none()
}
