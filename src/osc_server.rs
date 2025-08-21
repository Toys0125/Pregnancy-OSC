// src/osc_server.rs
use std::{
    net::{SocketAddrV4, UdpSocket},
    str::FromStr,
    sync::{Arc, Mutex},
    thread::sleep,
    time::Duration,
};

use lazy_static::lazy_static;
use log::{error, info};
use rosc::OscPacket;

#[derive(Clone, Debug)]
pub struct VRChatOscAddresss {
    pub host: String,
    pub port: u16,
}
#[derive(Debug)]
pub enum ValueType {
    Float,
    Int,
    Bool,
    Unknown,
    // Add other types as necessary
}

lazy_static! {
    static ref OSC_PORT: Mutex<Option<u16>> = Mutex::default();
    static ref OSC_QUERY: Mutex<Option<VRChatOscAddresss>> = Mutex::default();
    static ref VRC_PORT: Mutex<Option<u16>> = Mutex::default();
    static ref UDP_SOCKET: Mutex<Option<Arc<UdpSocket>>> = Mutex::new(None);
}

pub trait PacketHandler: Send + Sync {
    fn handle(&self, packet: OscPacket);
    fn start(&self) {}
}

pub struct OscServer;

impl OscServer {
    pub fn start(host: &str, port: u16, handlers: Vec<Arc<dyn PacketHandler>>) {
        let addr = SocketAddrV4::from_str(&format!("{}:{}", host, port)).unwrap();
        let socket = UdpSocket::bind(addr).expect("Could not bind socket");
        let socket = Arc::new(socket);
        {
            let mut socket_guard = UDP_SOCKET.lock().unwrap();
            *socket_guard = Some(socket);
        }

        std::thread::spawn(move || {
            let sock = UDP_SOCKET.lock().unwrap().as_ref().unwrap().clone();
            Self::set_osc_port(sock.local_addr().unwrap().port());
            info!(
                "Listening for OSC packets on {}",
                sock.local_addr().unwrap()
            );
            // TODO Find a better to alert this to the vrchat server
            info!("5 second wait for warm up.");
            sleep(Duration::from_secs(5));
            for handler in &handlers {
                handler.start();
            }
            let mut buf = [0u8; rosc::decoder::MTU];
            loop {
                match sock.recv_from(&mut buf) {
                    Ok((size, _)) => {
                        if let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..size]) {
                            for handler in &handlers {
                                handler.handle(packet.clone());
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error receiving from socket: {}", e);
                        break;
                    }
                }
            }
        });
    }

    pub fn set_osc_port(port: u16) {
        *OSC_PORT.lock().unwrap() = Some(port);
    }

    pub fn get_osc_port() -> Option<u16> {
        *OSC_PORT.lock().unwrap()
    }

    pub fn set_osc_query(host: String, port: u16) {
        *OSC_QUERY.lock().unwrap() = Some(VRChatOscAddresss { host, port });
    }

    pub fn get_osc_query() -> Option<String> {
        OSC_QUERY
            .lock()
            .unwrap()
            .as_ref()
            .map(|a| format!("http://{}:{}", a.host, a.port))
    }

    pub fn set_vrc_port(port: u16) {
        *VRC_PORT.lock().unwrap() = Some(port);
    }

    pub fn get_vrc_port() -> Option<u16> {
        *VRC_PORT.lock().unwrap()
    }

    pub fn send_osc_data(data: &[u8]) {
        let sock = {
            let socket_guard = UDP_SOCKET.lock().unwrap();
            socket_guard.as_ref().expect("UDP socket not initialized").try_clone().unwrap()
        };
    
        let target_address = OSC_QUERY
            .lock()
            .unwrap()
            .as_ref()
            .map(|addr| format!("{}:{}", addr.host, OscServer::get_vrc_port().unwrap_or(9000)))
            .unwrap_or_else(|| "127.0.0.1:9000".to_string());
    
        sock.send_to(data, target_address).expect("Failed to send OSC data");
    }

    pub fn auto_convert(input: &str) -> Option<(ValueType, String)> {
        // Strip the brackets
        let trimmed = input.strip_prefix('[').and_then(|s| s.strip_suffix(']'))?;

        // Match the type prefix and convert accordingly
        if trimmed.starts_with("Float") {
            trimmed
                .strip_prefix("Float(")
                .and_then(|s| s.strip_suffix(')'))
                .and_then(|s| s.parse::<f32>().ok())
                .map(|val| (ValueType::Float, val.to_string()))
        } else if trimmed.starts_with("Int") {
            trimmed
                .strip_prefix("Int(")
                .and_then(|s| s.strip_suffix(')'))
                .and_then(|s| s.parse::<i16>().ok())
                .map(|val| (ValueType::Int, val.to_string()))
        } else if trimmed.starts_with("Bool") {
            trimmed
                .strip_prefix("Bool(")
                .and_then(|s| s.strip_suffix(')'))
                .and_then(|s| s.parse::<bool>().ok())
                .map(|val| (ValueType::Bool, val.to_string()))
        } else {
            Some((ValueType::Unknown, input.to_string()))
        }
    }
}
