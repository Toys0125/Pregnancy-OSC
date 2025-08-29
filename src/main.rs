use std::{env, u16,net::IpAddr, vec};

mod osc_server;
use osc_server::{OscServer, PacketHandler};
mod pregancy_handler;
use pregancy_handler::{PregancyHandler,PregUI};
use eframe::egui;
mod osc_query_cache;
use dotenv::dotenv;

use log::info;
use std::sync::Arc;
use vrchat_osc::{Error, VRChatOSC};
mod utils;


fn main() -> eframe::Result<()> {
    // Spawn async OSC setup in a separate thread
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        let _ = rt.block_on(async_main());
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
async fn async_main() -> Result<(), Error> {
    dotenv().ok();
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info,vrchat_osc=warn,")
    }
    env_logger::init();
    let osc_query_enabled = env::var("OSCQuery")
        .unwrap_or("true".to_string())
        .parse::<bool>()
        .unwrap_or(true);
    let handlers: Vec<Arc<dyn PacketHandler>> = vec![Arc::new(PregancyHandler)];
    if osc_query_enabled {
        let vrchat_osc_instace = VRChatOSC::new().await?;
        OscServer::packet_handler(handlers).await;
        info!("OSCQuery Enabled and started.");
    } else {
        let portnumber: u16 = env::var("PORT")
            .unwrap_or("0".to_string())
            .parse()
            .expect("PORT must be a valid u16");
        OscServer::start("0.0.0.0", portnumber, handlers);
        info!(
            "OSC Server started on port {}",
            OscServer::get_osc_port().unwrap()
        );
        let vrc_osc: IpAddr = env::var("VRC_IP")
            .unwrap_or("127.0.0.1".to_string())
            .parse()
            .expect("address must be a valid IP address");
        let vrc_port: u16 = env::var("VRC_PORT")
            .unwrap_or("9000".to_string())
            .parse()
            .expect("vrc_port must be a valid u16");
        OscServer::set_vrc_address(vrc_osc, vrc_port);
    }
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
}
