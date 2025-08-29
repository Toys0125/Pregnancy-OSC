// src/osc_server.rs
use std::{
    net::{IpAddr, SocketAddrV4, UdpSocket},
    str::FromStr,
    sync::{Arc, Mutex},
    thread::sleep,
    time::Duration,
};

use lazy_static::lazy_static;
use log::{debug, error, info};
use rosc::{OscPacket, OscType};
use vrchat_osc::{models::OscRootNode, ServiceType, VRChatOSC};

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
    static ref VRC_OSC: Mutex<Option<Arc<VRChatOSC>>> = Mutex::new(None);
    static ref Tokio_RT: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
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
    pub async fn packet_handler(handlers: Vec<Arc<dyn PacketHandler>>) {
        let vrchat_osc = VRChatOSC::new().await.expect("Failed to create VRChatOSC");
        {
            let mut vrc_osc_guard = VRC_OSC.lock().unwrap();
            *vrc_osc_guard = Some(vrchat_osc);
        }
        let vrchat_osc = VRC_OSC.lock().unwrap().as_ref().unwrap().clone();
        vrchat_osc
            .on_connect(move |res| match res {
                ServiceType::Osc(name, addr) => {
                    info!("Found vrchat OSC server: {} at {}", name, addr);
                }
                ServiceType::OscQuery(name, addr) => {
                    info!("Connected to OSCQuery server: {} at {}", name, addr);
                    OSC_QUERY.lock().unwrap().replace(VRChatOscAddresss {
                        host: addr.ip().to_string(),
                        port: addr.port(),
                    });
                }
            })
            .await;
        for handler in &handlers {
            handler.start();
        }
        let root_node = OscRootNode::new().with_avatar();
        vrchat_osc
            .register("Pregancy OSC", root_node, move |packet| {
                for handler in &handlers {
                    handler.handle(packet.clone());
                }
            })
            .await
            .expect("Failed to register packet handler");
    }
    pub fn get_osc_port() -> Option<u16> {
        let port_guard = OSC_PORT.lock().unwrap();
        *port_guard
    }
    fn set_osc_port(port: u16) {
        let mut port_guard = OSC_PORT.lock().unwrap();
        *port_guard = Some(port);
    }
    pub fn set_vrc_address(host: IpAddr, port: u16) {
        let mut osc_query_guard = OSC_QUERY.lock().unwrap();
        *osc_query_guard = Some(VRChatOscAddresss {
            host: host.to_string(),
            port,
        });
    }

    pub fn get_osc_query() -> Option<String> {
        OSC_QUERY
            .lock()
            .unwrap()
            .as_ref()
            .map(|a| format!("http://{}:{}", a.host, a.port))
    }

    pub fn send_osc_data(addr: String, args: Vec<OscType>) {
        let vrc_osc_guard = VRC_OSC.lock().unwrap();
        if let Some(vrc_osc) = vrc_osc_guard.as_ref() {
            debug!("Calling Tokio spawn");
            let vrc_osc = Arc::clone(vrc_osc);
            // Spawn a task on the existing Tokio runtime
            Tokio_RT.spawn(async move {
                debug!("Sending OSC data to VRChat via VRChatOSC");
                vrc_osc
                    .send(
                        OscPacket::Message(rosc::OscMessage {
                            addr: addr,
                            args: args,
                        }),
                        "VRChat-Client-*",
                    )
                    .await
                    .expect("Failed to send OSC data");
            });
            return;
        } else {
            let sock = {
                let socket_guard = UDP_SOCKET.lock().unwrap();
                socket_guard
                    .as_ref()
                    .expect("UDP socket not initialized")
                    .try_clone()
                    .unwrap()
            };
            let target_address = OSC_QUERY
                .lock()
                .unwrap()
                .as_ref()
                .map(|addr| format!("{}:{}", addr.host, addr.port))
                .unwrap_or_else(|| "127.0.0.1:9000".to_string());

            sock.send_to(
                &rosc::encoder::encode(&OscPacket::Message(rosc::OscMessage {
                    addr: addr,
                    args: args,
                }))
                .unwrap(),
                target_address,
            )
            .expect("Failed to send OSC data");
        }
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
