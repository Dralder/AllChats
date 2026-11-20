use serde::{Deserialize, Deserializer, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, path::PathBuf};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:x}{:x}", nanos, n)
}

#[derive(Serialize, Clone)]
pub struct Item {
    pub sel: String,
    pub name: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawItem {
    Plain(String),
    Full {
        sel: String,
        #[serde(default)]
        name: String,
    },
}

impl<'de> Deserialize<'de> for Item {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match RawItem::deserialize(d)? {
            RawItem::Plain(s) => Item {
                name: s.clone(),
                sel: s,
            },
            RawItem::Full { sel, name } => Item {
                name: if name.is_empty() { sel.clone() } else { name },
                sel,
            },
        })
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub url: String,
    pub kind: String,
    pub kick: String,
    pub twitch: String,
    pub hidden: Vec<Item>,
    pub fade: Vec<Item>,
    pub backgrounds: Vec<Item>,
    pub auto_bg: bool,
    pub seven_tv: bool,
    pub opacity: f64,
    pub hide_border: bool,
    pub conn_check: bool,
    #[serde(default = "default_zoom")]
    pub zoom: f64,
    pub fade_secs: u32,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

fn default_zoom() -> f64 {
    1.0
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: "Default".into(),
            url: String::new(),
            kind: "custom".into(),
            kick: String::new(),
            twitch: String::new(),
            hidden: Vec::new(),
            fade: Vec::new(),
            backgrounds: Vec::new(),
            auto_bg: true,
            seven_tv: true,
            opacity: 0.0,
            hide_border: false,
            conn_check: true,
            zoom: 1.0,
            fade_secs: 0,
            x: 100,
            y: 100,
            w: 420,
            h: 720,
        }
    }
}

impl Profile {
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Self::default()
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Geo {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub w: u32,
    pub h: u32,
}

impl Default for Geo {
    fn default() -> Self {
        Self { x: None, y: None, w: 480, h: 720 }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Opt {
    pub topmost: bool,
    pub seven: bool,
    pub zoom: f64,
}

impl Default for Opt {
    fn default() -> Self {
        Self { topmost: false, seven: true, zoom: 1.0 }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct WinCfg {
    pub kind: String,
    pub url: String,
    pub kick: String,
    pub twitch: String,
    pub topmost: bool,
    pub seven: bool,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub w: u32,
    pub h: u32,
    pub geo: std::collections::HashMap<String, Geo>,
    pub open: Vec<String>,
    pub opts: std::collections::HashMap<String, Opt>,
}

impl Default for WinCfg {
    fn default() -> Self {
        Self {
            kind: "custom".into(),
            url: String::new(),
            kick: String::new(),
            twitch: String::new(),
            topmost: false,
            seven: true,
            x: None,
            y: None,
            w: 480,
            h: 720,
            geo: std::collections::HashMap::new(),
            open: Vec::new(),
            opts: std::collections::HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub last_profile: String,
    pub lang: String,
    pub mode: String,
    pub win: WinCfg,
    pub ctl_x: Option<i32>,
    pub ctl_y: Option<i32>,
    pub overlay_open: Option<bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_profile: "Default".into(),
            lang: String::new(),
            mode: "overlay".into(),
            win: WinCfg::default(),
            ctl_x: None,
            ctl_y: None,
            overlay_open: None,
        }
    }
}

pub struct Store {
    pub root: PathBuf,
    profiles: PathBuf,
}

fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').to_string();
    if cleaned.is_empty() {
        "profile".into()
    } else {
        cleaned
    }
}

fn write_atomic(path: &PathBuf, data: &str) {
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    let wrote = fs::File::create(&tmp)
        .and_then(|mut f| {
            f.write_all(data.as_bytes())?;
            f.sync_all()
        })
        .is_ok();
    if wrote {
        if fs::rename(&tmp, path).is_err() {
            let _ = fs::write(path, data);
            let _ = fs::remove_file(&tmp);
        }
    }
}

impl Store {
    pub fn new() -> Self {
        let root = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Dralder")
            .join("AllChats");
        let profiles = root.join("profiles");
        let _ = fs::create_dir_all(&profiles);
        let _ = fs::create_dir_all(root.join("webdata"));
        let _ = fs::create_dir_all(root.join("lang"));
        Self { root, profiles }
    }

    fn settings_path(&self) -> PathBuf {
        self.root.join("AllChats.js")
    }

    fn profile_path(&self, name: &str) -> PathBuf {
        self.profiles.join(format!("{}.json", safe_name(name)))
    }

    pub fn ui_dir(&self) -> PathBuf {
        self.root.join("ui")
    }

    pub fn data_dir(&self, id: &str) -> PathBuf {
        self.root.join("webdata").join(id)
    }

    pub fn cleanup(&self, ids: &[String]) {
        if let Ok(rd) = fs::read_dir(self.root.join("webdata")) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !ids.contains(&name) {
                    let _ = fs::remove_dir_all(entry.path());
                }
            }
        }
    }

    pub fn delete_data_later(&self, id: &str) {
        let path = self.data_dir(id);
        std::thread::spawn(move || {
            for _ in 0..30 {
                if !path.exists() || fs::remove_dir_all(&path).is_ok() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        });
    }

    pub fn load_settings(&self) -> Settings {
        fs::read_to_string(self.settings_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) {
        if let Ok(s) = serde_json::to_string_pretty(settings) {
            write_atomic(&self.settings_path(), &s);
        }
    }

    pub fn load_profiles(&self) -> Vec<Profile> {
        let mut list: Vec<Profile> = Vec::new();
        if let Ok(rd) = fs::read_dir(&self.profiles) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(text) = fs::read_to_string(&path) {
                    if let Ok(mut p) = serde_json::from_str::<Profile>(&text) {
                        if p.name.trim().is_empty() {
                            p.name = path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Profile")
                                .to_string();
                        }
                        if !text.contains("\"id\"") {
                            self.save_profile(&p);
                        }
                        list.push(p);
                    }
                }
            }
        }
        list.sort_by_key(|p| p.name.to_lowercase());
        list
    }

    pub fn save_profile(&self, profile: &Profile) {
        if let Ok(s) = serde_json::to_string_pretty(profile) {
            write_atomic(&self.profile_path(&profile.name), &s);
        }
    }

    pub fn delete_profile(&self, name: &str) {
        let _ = fs::remove_file(self.profile_path(name));
    }
}
