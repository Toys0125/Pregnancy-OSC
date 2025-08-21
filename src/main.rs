use std::{env, u16, vec};

mod osc_server;
use osc_server::{OscServer, PacketHandler};
mod pregancy_handler;
use pregancy_handler::{PregancyHandler,PregUI};
use eframe::egui;
mod osc_query_cache;
use dotenv::dotenv;

use log::info;
use oyasumivr_oscquery::OSCMethod;
use std::sync::Arc;
mod utils;


fn main() -> eframe::Result<()> {
    // Spawn async OSC setup in a separate thread
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async_main());
    });

    // Launch UI on the main thread
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Pregnancy Monitor")
            .with_inner_size(egui::vec2(500.0, 200.0))
            .with_always_on_top()
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("./Pregancy_Logo_512.png")).unwrap()),
        ..Default::default()
    };

    eframe::run_native("Pregnancy Monitor", options, Box::new(|_cc| Ok(Box::new(PregUI::new(_cc)))))
}
#[allow(unused_variables)]
async fn async_main() {
    dotenv().ok();
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info")
    }
    env_logger::init();
    let osc_query_enabled = env::var("OSCQuery")
    .unwrap_or("true".to_string())
    .parse::<bool>()
    .unwrap_or(true);
    if osc_query_enabled
    {
        oyasumivr_oscquery::client::init()
            .await
            .unwrap();
    }
    let mut portnumber: u16 = env::var("PORT")
        .unwrap_or("0".to_string())
        .parse()
        .expect("PORT must be a valid u16");
    let handlers: Vec<Arc<dyn PacketHandler>> = vec![Arc::new(PregancyHandler)];
    //handlers.push(Arc::new(PregancyHandler));
    OscServer::start("127.0.0.1", portnumber, handlers);
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    portnumber = OscServer::get_osc_port().expect("Failed to get port number from OSC Server");

    // Wait a bit for the MDNS daemon to find the services
    tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
    // Get the address of the VRChat OSC server
    
    let (mut _host, mut _port) = (String::new(), 0);
    loop {
        if let Some((h, p)) = oyasumivr_oscquery::client::get_vrchat_osc_address().await {
            _host = h;
            _port = p;
            log::info!("Connected to OSC Query Server at {}:{}", _host, _port);
            break;
        } else {
            log::error!("Failed to find OSC Query Server. Retrying in 2 seconds...");
            tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
        }
    }
    let normal_address = osc_server::VRChatOscAddresss {
        host: _host,
        port: _port,
    };
    info!(
        "VRChat OSC address: {}:{}",
        normal_address.host, normal_address.port
    );
    OscServer::set_vrc_port(normal_address.port);

    // Get the address of the VRChat OSCQuery server
    let (host, port) = oyasumivr_oscquery::client::get_vrchat_oscquery_address()
        .await
        .expect("Failed to find oscquery address");
    let osc_query_address = osc_server::VRChatOscAddresss {
        host: host,
        port: port,
    };
    info!(
        "VRChat OSC Query address: {}:{}",
        osc_query_address.host, osc_query_address.port
    );
    OscServer::set_osc_query(osc_query_address.host, osc_query_address.port);

    let (host, port) =
        oyasumivr_oscquery::server::init("Pregancy OSC", portnumber)
            .await
            .unwrap();
    info!("Pregancy OSCquery address: {}:{}", host, port);

    //oyasumivr_oscquery::server::receive_vrchat_avatar_parameters().await; // /avatar/*, /avatar/parameters/*, etc.
    //oyasumivr_oscquery::server::receive_vrchat_tracking_data().await; // /tracking/vrsystem/*

    // Configure the OSC Query server by registering addresses we're interesting in receiving
    // Getting VRChat avatar parameters
    oyasumivr_oscquery::server::add_osc_method(OSCMethod {
        description: Some("VRChat Avatar Parameters".to_string()),
        address: "/avatar/parmaters/Likes".to_string(),
        // Write: We only want to receive these values from VRChat, not send them
        ad_type: oyasumivr_oscquery::OSCMethodAccessType::ReadWrite,
        value_type: None,
        value: None,
    })
    .await;
    /*
        // Also getting VR tracking data
        oyasumivr_oscquery::server::add_osc_method(OSCMethod {
            description: Some("VRChat VR Tracking Data".to_string()),
            address: "/tracking/vrsystem".to_string(),
            // Write: We only want to receive these values from VRChat, not send them
            ad_type: oyasumivr_oscquery::OSCMethodAccessType::ReadWrite,
            value_type: None,
            value: None,
        })
        .await;
    */

    // Now we can start broadcasting the advertisement for the OSC and OSCQuery server
    oyasumivr_oscquery::server::advertise().await.unwrap();

    // Keep process alive
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
}
