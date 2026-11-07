use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

const EN: &str = include_str!("../lang/en.json");
const FA: &str = include_str!("../lang/fa.json");
const FONT_EN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/en.ttf"));
const FONT_FA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fa.ttf"));

pub fn valid(code: &str) -> bool {
    code == "en" || code == "fa"
}

fn parse(text: &str) -> HashMap<String, Value> {
    let text = text.trim_start_matches('\u{feff}');
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => map.into_iter().collect(),
        _ => HashMap::new(),
    }
}

pub fn load(root: &Path, code: &str) -> HashMap<String, Value> {
    let mut map = parse(EN);
    if code == "fa" {
        for (k, v) in parse(FA) {
            map.insert(k, v);
        }
    }
    let override_path = root.join("lang").join(format!("{}.json", code));
    if let Ok(content) = std::fs::read_to_string(&override_path) {
        for (k, v) in parse(&content) {
            if v.is_string() {
                map.insert(k, v);
            }
        }
    }
    map
}

pub fn text(map: &HashMap<String, Value>, key: &str) -> String {
    map.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

fn face(name: &str, data: &[u8]) -> String {
    if data.len() < 1000 || data.len() > 600_000 {
        return String::new();
    }
    format!(
        "@font-face{{font-family:'{}';src:url(data:font/ttf;base64,{}) format('truetype');font-weight:100 900;font-style:normal;font-display:block}}",
        name,
        STANDARD.encode(data)
    )
}

pub fn font_css(code: Option<&str>) -> String {
    match code {
        Some("en") => face("AC-EN", FONT_EN),
        Some("fa") => face("AC-FA", FONT_FA),
        _ => format!("{}{}", face("AC-EN", FONT_EN), face("AC-FA", FONT_FA)),
    }
}

pub fn font_stack(code: &str) -> &'static str {
    if code == "fa" {
        "'AC-FA',Tahoma,'Segoe UI',sans-serif"
    } else {
        "'AC-EN',Consolas,'Segoe UI',monospace"
    }
}
