use crate::osc_server::OscServer;
use lazy_static::lazy_static;
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::runtime::Handle;

pub struct OscQueryCache {
    last_fetched: Option<Instant>,
    cached_data: Option<Value>,
    avatar_id: Option<String>,
    avatar_name: Option<String>,
}
lazy_static! {
    static ref CACHE: Mutex<OscQueryCache> = Mutex::new(OscQueryCache::new());
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::new();
    static ref Tokio_RT: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
}

impl OscQueryCache {
    pub fn new() -> Self {
        Self {
            last_fetched: None,
            cached_data: None,
            avatar_id: None,
            avatar_name: None,
        }
    }
    pub fn clear_avatar(&mut self) {
        let now = Instant::now();
        if let Some(timestamp) = &self.last_fetched {
            if now.duration_since(*timestamp) > Duration::from_millis(500) {
                self.avatar_id = None;
                self.avatar_name = None;
                self.cached_data = None;
                self.last_fetched = Some(now);
            }
        }
    }

    // Common async block for both sync/async paths
    async fn fetch_avatar_data(url: &str) -> Result<String, Box<dyn std::error::Error>> {
        let resp = HTTP_CLIENT.get(url).send().await?;
        let resp = resp.error_for_status()?;
        Ok(resp.text().await?)
    }

    pub fn get_avatar_id(&mut self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        if let Some(avatar_id) = &self.avatar_id {
            //log::debug!("Returning cloned avatar id");
            return Ok(Some(avatar_id.clone()));
        }
        let url = match OscServer::get_osc_query() {
            Some(base_url) => format!("{}/avatar/change", base_url),
            None => return Ok(None),
        };

        // --- Core logic ---
        let response = match Handle::try_current() {
            // Already inside runtime → spawn task instead of blocking
            Ok(handle) => {
                // Use `block_in_place` to temporarily allow blocking inside async
                tokio::task::block_in_place(|| {
                    let rt = handle.clone();
                    rt.block_on(async { OscQueryCache::fetch_avatar_data(&url).await })
                })
            }

            // Not inside runtime → use your global or local runtime
            Err(_) => Tokio_RT.block_on(async { OscQueryCache::fetch_avatar_data(&url).await }),
        }
        .map_err(|e: Box<dyn std::error::Error>| {
            log::error!("Failed to fetch avatar data from {}: {}", url, e);
            e
        })?;
        log::debug!("Avatar data is{}", response);
        match serde_json::from_str::<Value>(&response) {
            Ok(json) => {
                self.avatar_id = json["VALUE"][0]
                    .as_str()
                    .and_then(|v| String::try_from(v).ok());
                Ok(self.avatar_id.clone())
            }
            Err(e) => {
                println!("Failed to parse JSON: {}", e);
                Ok(Some(String::new()))
            }
        }
    }
    pub fn get_avatar_parameters(&mut self) -> Result<Value, Box<dyn std::error::Error>> {
        let now = Instant::now();
        if let (Some(timestamp), Some(data)) = (&self.last_fetched, &self.cached_data) {
            if now.duration_since(*timestamp) < Duration::from_secs(5) {
                log::debug!("Returning cloned avatar parameters");
                return Ok(data.clone());
            }
        }

        let url = match OscServer::get_osc_query() {
            Some(base_url) => format!("{}/avatar/parameters", base_url),
            None => return Ok(Value::Null),
        };
        self.avatar_id = None;

        // --- Core logic ---
        let response = match Handle::try_current() {
            // Already inside runtime → spawn task instead of blocking
            Ok(handle) => {
                // Use `block_in_place` to temporarily allow blocking inside async
                tokio::task::block_in_place(|| {
                    let rt = handle.clone();
                    rt.block_on(async { OscQueryCache::fetch_avatar_data(&url).await })
                })
            }

            // Not inside runtime → use your global or local runtime
            Err(_) => Tokio_RT.block_on(async { OscQueryCache::fetch_avatar_data(&url).await }),
        }
        .map_err(|e: Box<dyn std::error::Error>| {
            log::error!("Failed to fetch avatar data from {}: {}", url, e);
            e
        })?;
        match serde_json::from_str::<Value>(&response) {
            Ok(json) => {
                self.last_fetched = Some(now);
                self.cached_data = Some(json.clone());
                Ok(json)
            }
            Err(e) => {
                println!("Failed to parse JSON: {}", e);
                Ok(Value::Null)
            }
        }
    }
}
pub fn get_osc_query_cache() -> std::sync::MutexGuard<'static, OscQueryCache> {
    CACHE.lock().expect("Failed to lock OSC Query Cache")
}
