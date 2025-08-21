use serde_json::Value;
pub fn json_path_exists(json_data: &Value, path: &str) -> bool {
    json_data.pointer(path).is_some()
}
pub fn get_save_path() -> std::path::PathBuf {
    let mut path = dirs::data_dir().expect("Failed to find app data directory");
    path.push("ToysOSC");
    std::fs::create_dir_all(&path).expect("Failed to create ToysOSC directory");
    path
}