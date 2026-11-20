use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

static BUSY: AtomicUsize = AtomicUsize::new(0);

enum Got {
    Data(Value),
    Missing,
    Failed,
}

enum Id {
    Found(String),
    Missing,
    Failed,
}

fn pause(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

fn get(url: &str) -> Got {
    for attempt in 0..3u64 {
        if attempt > 0 {
            pause(700 * attempt);
        }
        match ureq::get(url)
            .set("User-Agent", "AllChats")
            .timeout(Duration::from_secs(15))
            .call()
        {
            Ok(resp) => {
                if let Ok(text) = resp.into_string() {
                    if let Ok(v) = serde_json::from_str::<Value>(&text) {
                        return Got::Data(v);
                    }
                }
            }
            Err(ureq::Error::Status(code, _)) if code < 500 && code != 429 => return Got::Missing,
            Err(_) => {}
        }
    }
    Got::Failed
}

fn add(map: &mut BTreeMap<String, String>, out: &mut Vec<(String, String)>, list: &Value) {
    if let Some(arr) = list.as_array() {
        for e in arr {
            if let (Some(id), Some(name)) = (e["id"].as_str(), e["name"].as_str()) {
                let url = format!("https://cdn.7tv.app/emote/{}/2x.webp", id);
                map.insert(name.to_string(), url.clone());
                out.push((name.to_string(), url));
            }
        }
    }
}

fn twitch_id(login: &str) -> Id {
    let query = format!("{{\"query\":\"query{{user(login:\\\"{}\\\"){{id}}}}\"}}", login);
    for attempt in 0..2u64 {
        if attempt > 0 {
            pause(700);
        }
        let resp = ureq::post("https://gql.twitch.tv/gql")
            .set("Client-ID", "kimne78kx3ncx6brgo4mv6wki5h1ko")
            .set("Content-Type", "application/json")
            .timeout(Duration::from_secs(8))
            .send_string(&query);
        let id = resp
            .ok()
            .and_then(|r| r.into_string().ok())
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["data"]["user"]["id"].as_str().map(String::from));
        if let Some(id) = id {
            return Id::Found(id);
        }
    }
    match get(&format!("https://api.ivr.fi/v2/twitch/user?login={}", login)) {
        Got::Data(v) => match v[0]["id"].as_str() {
            Some(id) => Id::Found(id.to_string()),
            None => Id::Missing,
        },
        Got::Missing => Id::Missing,
        Got::Failed => Id::Failed,
    }
}

pub fn fetch(platform: &str, ident: &str) -> (String, String, bool) {
    let mut map = BTreeMap::new();
    let mut chan: Vec<(String, String)> = Vec::new();
    let mut glob: Vec<(String, String)> = Vec::new();
    let mut complete = true;
    match get("https://7tv.io/v3/emote-sets/global") {
        Got::Data(v) => add(&mut map, &mut glob, &v["emotes"]),
        Got::Missing => {}
        Got::Failed => complete = false,
    }
    let id = if platform == "twitch" {
        if ident.is_empty()
            || ident.len() > 25
            || !ident
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            Id::Missing
        } else {
            twitch_id(ident)
        }
    } else {
        Id::Found(ident.to_string())
    };
    match id {
        Id::Found(pid) => match get(&format!("https://7tv.io/v3/users/{}/{}", platform, pid)) {
            Got::Data(v) => add(&mut map, &mut chan, &v["emote_set"]["emotes"]),
            Got::Missing => {}
            Got::Failed => complete = false,
        },
        Id::Missing => {}
        Id::Failed => complete = false,
    }
    if map.is_empty() {
        return ("{}".to_string(), "{}".to_string(), false);
    }
    glob.retain(|g| !chan.iter().any(|c| c.0 == g.0));
    let picker = serde_json::json!({ "c": chan, "g": glob }).to_string();
    (
        serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string()),
        picker,
        complete,
    )
}

fn grab(url: &str) -> Option<Vec<u8>> {
    for attempt in 0..2u64 {
        if attempt > 0 {
            pause(500);
        }
        if let Ok(resp) = ureq::get(url)
            .set("User-Agent", "AllChats")
            .timeout(Duration::from_secs(12))
            .call()
        {
            let mut buf = Vec::new();
            if resp.into_reader().take(2_000_000).read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                return Some(buf);
            }
        }
    }
    None
}

pub fn image(url: &str) -> String {
    use base64::Engine;
    while BUSY.load(Ordering::SeqCst) >= 4 {
        pause(30);
    }
    BUSY.fetch_add(1, Ordering::SeqCst);
    let mut bytes = grab(url);
    if bytes.is_none() && url.ends_with("/2x.webp") {
        bytes = grab(&url.replace("/2x.webp", "/1x.webp"));
    }
    BUSY.fetch_sub(1, Ordering::SeqCst);
    match bytes {
        Some(b) => format!("data:image/webp;base64,{}", base64::engine::general_purpose::STANDARD.encode(b)),
        None => String::new(),
    }
}
