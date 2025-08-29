use crate::osc_query_cache::get_osc_query_cache;
use crate::osc_server::{OscServer, PacketHandler, ValueType};
use crate::utils::{get_save_path, json_path_exists};
use chrono::{DateTime, Duration, Local};
use lazy_static::lazy_static;
use log::info;
use rosc::{OscPacket, OscType};
use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use strum::IntoEnumIterator;

use eframe::egui::{self, Key};
use eframe::App as EguiApp;
// ChildCount, PregnancySave, GestationTime, Gestation (1-4)
#[derive(Clone, Debug, Copy)]
struct ChildInfo {
    conception_time: Option<DateTime<Local>>,
    gestation_time: f32,
    gestation: GestationType,
    number_of_childern: u8,
}
impl Default for ChildInfo {
    fn default() -> Self {
        ChildInfo {
            conception_time: None,
            gestation_time: 8f32,
            gestation: GestationType::Hours,
            number_of_childern: 0,
        }
    }
}

impl Serialize for ChildInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ChildInfo", 4)?;
        if let Some(_dt) = self.conception_time {
            state.serialize_field(
                "conception_time",
                &self.conception_time.unwrap().to_rfc3339(),
            )?;
        } else {
            state.serialize_field("conception_time", &None::<String>)?;
        }

        state.serialize_field("gestation_time", &self.gestation_time)?;
        state.serialize_field("gestation", &self.gestation)?;
        state.serialize_field("number_of_childern", &self.number_of_childern)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ChildInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ChildInfoHelper {
            conception_time: Option<String>,
            gestation_time: f32,
            gestation: GestationType,
            number_of_childern: u8,
        }

        let helper = ChildInfoHelper::deserialize(deserializer)?;
        let conception_time = match helper.conception_time {
            Some(s) => Some(
                DateTime::parse_from_rfc3339(&s)
                    .map_err(serde::de::Error::custom)?
                    .with_timezone(&Local),
            ),
            None => None,
        };
        Ok(ChildInfo {
            conception_time,
            gestation_time: helper.gestation_time,
            gestation: helper.gestation,
            number_of_childern: helper.number_of_childern,
        })
    }
}
#[derive(Serialize, Deserialize, Debug, Default)]
struct SaveData {
    avatar_ids: HashMap<String, ChildInfo>,
}
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, strum_macros::EnumIter)]
enum GestationType {
    Hours = 0,
    Days = 1,
    Weeks = 2,
    Months = 3,
    Mins = 4,
}
impl GestationType {
    /// Returns the number of seconds that one unit represents.
    /// For Months, we’re using an approximation (30 days per month).
    #[inline] // Suggests to inline this small function
    pub fn seconds_per_unit(self) -> i64 {
        match self {
            GestationType::Hours => 3600,
            GestationType::Days => 86400,
            GestationType::Weeks => 604800,
            GestationType::Months => 2592000,
            GestationType::Mins => 60,
        }
    }
}
impl ToString for GestationType {
    fn to_string(&self) -> String {
        match self {
            GestationType::Hours => "Hours".to_owned(),
            GestationType::Days => "Days".to_owned(),
            GestationType::Weeks => "Weeks".to_owned(),
            GestationType::Months => "Months".to_owned(),
            GestationType::Mins => "Mins".to_owned(),
        }
    }
}
impl TryFrom<u8> for GestationType {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(GestationType::Hours),
            1 => Ok(GestationType::Days),
            2 => Ok(GestationType::Weeks),
            3 => Ok(GestationType::Months),
            4 => Ok(GestationType::Mins),
            _ => Err("Invalid value for TimeUnit"),
        }
    }
}
impl From<GestationType> for i32 {
    fn from(value: GestationType) -> Self {
        value as i32
    }
}
impl From<GestationType> for u8 {
    fn from(value: GestationType) -> Self {
        value as u8
    }
}

lazy_static! {
    static ref SystemActive: Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(Some(false)));
    static ref ChildData: Arc<Mutex<Option<ChildInfo>>> = Arc::new(Mutex::new(None));
}
pub struct PregancyHandler;

impl PacketHandler for PregancyHandler {
    fn handle(&self, packet: OscPacket) {
        match packet {
            OscPacket::Message(msg) => {
                let (_osc_type, osc_value) = OscServer::auto_convert(&format!("{:?}", msg.args))
                    .unwrap_or((ValueType::Unknown, format!("{:?}", msg.args)));
                match msg.addr.as_str() {
                    "/avatar/parameters/Childcount" => {
                        if get_system_active().unwrap() {
                            child_counter(osc_value.parse::<u8>().unwrap());
                            save_data().unwrap();
                        }
                    }
                    "/avatar/parameters/GestationTime" => {
                        log::debug!("Hitting gestationTime parameter");
                        if get_system_active().unwrap() {
                            set_gestation_time(osc_value.parse::<f32>().unwrap());
                            save_data().unwrap();
                        }
                    }
                    "/avatar/parameters/Gestation" => {
                        log::debug!("Hitting gestation parameter");
                        if get_system_active().unwrap() {
                            set_gestation_type(osc_value.parse::<u8>().unwrap());
                            save_data().unwrap();
                        }
                    }
                    "/avatar/change" => check_avatar_oscquery().unwrap(),
                    _ => {}
                }
            }
            OscPacket::Bundle(_bundle) => { /* println!("OSC Bundle: {:?}", bundle); */ }
        }
    }
    fn start(&self) {
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("Pregnancy Monitor")
                .with_inner_size(egui::vec2(300.0, 200.0)),
            ..Default::default()
        };
        let _ = eframe::run_native(
            "Pregnancy Monitor",
            native_options,
            Box::new(|_cc| Ok(Box::new(PregUI::new(_cc)))),
        );
        // Spawn UI in separate thread
        std::thread::spawn(move || loop {
            if get_system_active().unwrap() {
                if get_child_count() > 0 {
                    OscServer::send_osc_data(
                        "/avatar/parameters/PregnancySave".to_string(),
                        vec![OscType::Float(get_gestation_progress_fraction() as f32)],
                    );
                    log::debug!(
                        "Current Pregnacy Progress is {}",
                        get_gestation_progress_fraction()
                    );
                }
                std::thread::sleep(std::time::Duration::from_secs(5));
            } else {
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        });
        check_avatar_oscquery().unwrap();
    }
}
fn check_avatar_oscquery() -> Result<(), Box<dyn std::error::Error>> {
    let data = get_osc_query_cache().get_avatar_parameters()?;
    get_osc_query_cache().clear_avatar();
    info!("Calling check avatar");
    if json_path_exists(&data, "/CONTENTS/PregnancySave") {
        info!("Found Fertility system on avatar");
        let mut data = read_data()?;
        // Set my childInfo data if we have data from our appdata directory, otherwise set a default childInfo.
        set_child_data(
            *data
                .avatar_ids
                .entry(
                    get_osc_query_cache()
                        .get_avatar_id()
                        .unwrap()
                        .expect("Missing string"),
                )
                .or_insert(ChildInfo {
                    conception_time: None,
                    gestation_time: 8f32,
                    gestation: GestationType::Hours,
                    number_of_childern: 0,
                }),
        );
        set_system_active(true);
        // Extract all needed data before spawning the async block to avoid holding MutexGuard across await.
        let gestation_time = get_gestation_time();
        let gestation_type = get_gestation_type();
        let child_count = get_child_count();
        OscServer::send_osc_data(
            "/avatar/parameters/GestationTime".to_string(),
            vec![OscType::Float(gestation_time.into())],
        );
        OscServer::send_osc_data(
            "/avatar/parameters/Gestation".to_string(),
            vec![OscType::Int(gestation_type.into())],
        );
        if child_count > 0 {
            OscServer::send_osc_data(
                "/avatar/parameters/ChildCount".to_string(),
                vec![OscType::Int(child_count.into())],
            );

            OscServer::send_osc_data(
                "/avatar/parameters/IsPregnant".to_string(),
                vec![OscType::Bool(true)],
            );
        }

        save_data_writer(&data)?;
    } else {
        set_system_active(false);
        clear_child_data();
    }
    Ok(())
}

fn save_data_writer(data: &SaveData) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(data).expect("Failed to serialize data");
    let path = get_save_path().join("save_data.json");
    let mut file = std::fs::File::create(path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

fn save_data() -> std::io::Result<()> {
    let mut save_data = read_data().unwrap();
    let child_data = save_data
        .avatar_ids
        .get_mut(
            &get_osc_query_cache()
                .get_avatar_id()
                .unwrap()
                .expect("Missing avatar id"),
        )
        .unwrap();
    *child_data = get_child_data().unwrap();
    save_data_writer(&save_data).unwrap();
    Ok(())
}

fn read_data() -> std::io::Result<SaveData> {
    let path = get_save_path().join("save_data.json");

    // Check if file exists, if not create it with default SaveData
    if !path.exists() {
        let default_data = SaveData::default(); // Requires SaveData to implement Default
        let json = serde_json::to_string_pretty(&default_data)
            .expect("Failed to serialize default SaveData");
        let mut file = std::fs::File::create(&path)?;
        file.write_all(json.as_bytes())?;
    }

    let content = std::fs::read_to_string(path)?;
    let data: SaveData = serde_json::from_str(&content).expect("Failed to deserialize JSON");
    Ok(data)
}

fn child_counter(value: u8) {
    if value > get_child_count() {
        set_child_count(value);
        if get_conception_time() == None {
            set_conception_time(Local::now());
        }
    }
}

fn get_system_active() -> Option<bool> {
    *SystemActive.lock().unwrap()
}

fn set_system_active(value: bool) {
    *SystemActive.lock().unwrap() = Some(value);
}
fn get_child_data() -> Option<ChildInfo> {
    ChildData.lock().unwrap().clone()
}
fn set_child_data(value: ChildInfo) {
    let mut lock = ChildData.lock().unwrap();
    *lock = Some(value);
}
fn clear_child_data() {
    let mut lock = ChildData.lock().unwrap();
    *lock = None;
}
fn get_child_count() -> u8 {
    let childdata: ChildInfo = get_child_data().unwrap_or_default();
    return childdata.number_of_childern;
}
fn set_child_count(value: u8) {
    let mut lock = ChildData.lock().unwrap();
    if let Some(ref mut childdata) = *lock {
        childdata.number_of_childern = value;
    }
    OscServer::send_osc_data("/avatar/parameters/ChildCount".to_string(), vec![OscType::Int(value.into())]);
}
fn get_conception_time() -> Option<DateTime<Local>> {
    let childdata: ChildInfo = get_child_data().unwrap_or_default();
    return childdata.conception_time;
}
fn clear_conception_time() {
    let mut lock = ChildData.lock().unwrap();
    if let Some(ref mut childdata) = *lock {
        childdata.conception_time = None;
    }
}
fn set_conception_time(value: DateTime<Local>) {
    let mut lock = ChildData.lock().unwrap();
    if let Some(ref mut childdata) = *lock {
        childdata.conception_time = Some(value);
    }
}
fn get_gestation_time() -> f32 {
    let childdata: ChildInfo = get_child_data().unwrap_or_default();
    return childdata.gestation_time;
}
fn set_gestation_time(value: f32) {
    let mut lock = ChildData.lock().unwrap();
    if let Some(ref mut childdata) = *lock {
        childdata.gestation_time = value;
    }
}
fn get_gestation_type() -> GestationType {
    let childdata: ChildInfo = get_child_data().unwrap_or_default();
    return GestationType::try_from(childdata.gestation).unwrap_or(GestationType::Hours);
}
fn set_gestation_type(value: u8) {
    let mut lock = ChildData.lock().unwrap();
    if let Some(ref mut childdata) = *lock {
        childdata.gestation = GestationType::try_from(value).unwrap_or(GestationType::Hours);
    }
}
/// Calculates a future DateTime by adding a duration (in whole seconds)
/// computed as multiplier * (seconds per unit).
#[inline] // Hint to inline the function
pub fn calculate_future_time() -> DateTime<Local> {
    // Casting directly from f64 to i64 truncates the fractional part.
    let total_duration_secs =
        get_gestation_time() as f64 * get_gestation_type().seconds_per_unit() as f64;
    let conception_time = get_conception_time();
    if conception_time == None {
        return Local::now();
    }
    return conception_time.unwrap() + Duration::seconds(total_duration_secs as i64);
}
/// Returns the remaining percentage of gestation time as a decimal between 0.0 and 1.0
#[inline] // Hint to inline the function
pub fn get_gestation_progress_fraction() -> f64 {
    if get_child_count() == 0 {
        return 0f64;
    }
    let total_duration_secs =
        get_gestation_time() as f64 * get_gestation_type().seconds_per_unit() as f64;

    let conception_time = get_conception_time();
    if conception_time == None {
        return 0f64;
    }

    let elapsed_secs = (Local::now() - conception_time.unwrap()).num_seconds() as f64;
    //log::debug!("elpased time: {}, total time: {}, returned fraction {}",elapsed_secs, total_duration_secs, elapsed_secs/total_duration_secs);
    // remaining fraction as decimal
    return (elapsed_secs / total_duration_secs).min(1.0);
}
#[derive(Default)]
pub struct PregUI {
    last_content_size: egui::Vec2,
}

impl PregUI {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self::default()
    }
}
/// Formats a chrono::Duration into a human-readable string like:
/// "2 months, 3 days, 4 hours, 5 minutes, 6 seconds"
fn format_duration_human(dur: chrono::Duration) -> String {
    let mut secs = dur.num_seconds().max(0);

    let months = secs / 2_592_000; // 30 days per month
    secs %= 2_592_000;

    let days = secs / 86_400;
    secs %= 86_400;

    let hours = secs / 3600;
    secs %= 3600;

    let minutes = secs / 60;
    secs %= 60;

    let mut parts = Vec::new();

    if months > 0 {
        parts.push(format!(
            "{} month{}",
            months,
            if months != 1 { "s" } else { "" }
        ));
    }
    if days > 0 {
        parts.push(format!("{} day{}", days, if days != 1 { "s" } else { "" }));
    }
    if hours > 0 {
        parts.push(format!(
            "{} hour{}",
            hours,
            if hours != 1 { "s" } else { "" }
        ));
    }
    if minutes > 0 {
        parts.push(format!(
            "{} minute{}",
            minutes,
            if minutes != 1 { "s" } else { "" }
        ));
    }
    if secs > 0 || parts.is_empty() {
        parts.push(format!(
            "{} second{}",
            secs,
            if secs != 1 { "s" } else { "" }
        ));
    }

    parts.join(", ")
}
impl EguiApp for PregUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        let mut current_content_size = egui::vec2(0.0, 0.0);

        let child_data = get_child_data();
        let active = get_system_active().unwrap_or(false);
        let avatar_id = get_osc_query_cache()
            .get_avatar_id()
            .unwrap()
            .unwrap_or("Unknown".to_string());

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Pregnancy Monitor");
            ui.label(format!("System Active: {}", active));
            ui.label(format!("Avatar ID: {}", avatar_id));

            if let Some(child) = child_data {
                if child.number_of_childern > 0 {
                    let progress = get_gestation_progress_fraction();
                    let remaining = if let Some(conception) = child.conception_time {
                        let future = conception
                            + chrono::Duration::seconds(
                                (child.gestation_time * child.gestation.seconds_per_unit() as f32)
                                    as i64,
                            );
                        let now = chrono::Local::now();
                        let remaining = future.signed_duration_since(now);
                        format_duration_human(remaining)
                    } else {
                        "N/A".into()
                    };
                    
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Estimated Date: {} Remaining Time: {}",
                            calculate_future_time().format("%m/%d/%Y %H:%M"),
                            remaining
                        ));
                        if ui.button("Reset Pregancy.").clicked() {
                            set_conception_time(Local::now());
                            save_data().unwrap();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!("Gestation Progress:"));
                        ui.add(egui::ProgressBar::new(progress as f32)
                            .text(format!("{:.1}%", progress * 100.0)).show_percentage().animate(false));
                    });
                    

                    //ui.label(format!("Gestation Time: {:.2}", child.gestation_time));
                }
                ui.horizontal(|ui| {
                    ui.label("Gestation Type:");
                    egui::ComboBox::new("Gestation", "")
                        .selected_text(child.gestation.to_string())
                        .show_ui(ui, |ui| {
                            for ty in GestationType::iter() {
                                if ui
                                    .selectable_label(child.gestation == ty, ty.to_string())
                                    .clicked()
                                {
                                    let new_time = child.gestation.seconds_per_unit() as f64
                                        * child.gestation_time as f64
                                        / ty.seconds_per_unit() as f64;
                                    set_gestation_time(new_time as f32);
                                    set_gestation_type(ty as u8);
                                }
                            }
                        });
                });
                //Gestation Time
                ui.horizontal(|ui| {
                    ui.label("Gestation Time:");

                    // === DragValue (syncs with gestation_time) ===
                    let mut temp_value = child.gestation_time;
                    let gestation_response = ui.add(
                        egui::DragValue::new(&mut temp_value)
                            .range(0.01..=f32::INFINITY)
                            .speed(0.1)
                            .suffix(&format!(" {}", child.gestation.to_string())),
                    );

                    if gestation_response.changed() {
                        set_gestation_time(temp_value);
                        save_data().unwrap();
                    }
                    /* // === Text input ===
                    let text_response = ui.add_sized(
                        [80.0, 20.0],
                        egui::TextEdit::singleline(&mut self.gestation_time_input),
                    );
                    if text_response.lost_focus()
                        && ui.input(|i| {
                            i.key_pressed(egui::Key::Enter) || i.pointer.any_released()
                        })
                    {
                        if let Ok(parsed) = self.gestation_time_input.trim().parse::<f32>() {
                            if parsed > 0.0 {
                                set_gestation_time(parsed);
                                save_data().unwrap();
                            } else {
                                println!("Value must be > 0");
                            }
                        } else {
                            println!("Invalid float input");
                        }
                    } */
                });
                ui.label(format!("Child Count: {}", child.number_of_childern));
                ui.horizontal(|ui| {
                    // Handlers
                    if ui.button("Add Child").clicked() || ctx.input(|i| i.key_pressed(Key::Plus)) {
                        // Run this logic only when there's a user action
                        let child_count = get_child_count();
                        if get_conception_time().is_none() {
                            set_conception_time(Local::now());
                        }
                        if child_count < 12 {
                            set_child_count(child_count + 1);
                            save_data().unwrap();
                        }
                    }
                    if ui.button("Remove Child").clicked()
                        || ctx.input(|i| i.key_pressed(Key::Minus))
                    {
                        let child_count = get_child_count();
                        if child_count != 0 {
                            if child_count == 1 {
                                clear_conception_time();
                            }
                            set_child_count(child_count - 1);
                            save_data().unwrap();
                        }
                    }
                });
            } else {
                ui.label("No Child Data Available");
            }

            egui::CollapsingHeader::new("Help & Instructions")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("This panel shows the current pregnancy status.");
                    ui.label("• Remaining time is calculated based on gestation settings.");
                    ui.label("• Gestation progress updates every second.");
                    ui.label("• Click the Help button again to hide this.");
                });
            if ui.button("Recheck Avatar").clicked() {
                check_avatar_oscquery().unwrap();
            }
            current_content_size = ui.min_size();
        });
        // Check if the content size has changed significantly
        let threshold = 30f32; // Prevent resizing for subpixel jitter
        if (current_content_size - self.last_content_size).length_sq() > threshold * threshold {
            let padding = egui::vec2(32.0, 32.0);
            let new_size = current_content_size + padding;

            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(new_size));
            self.last_content_size = current_content_size;
        }
    }
}
