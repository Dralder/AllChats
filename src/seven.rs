use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

fn get(url: &str) -> Option<Value> {
    let resp = ureq::get(url)
        .set("User-Agent", "AllChats")
        .timeout(Duration::from_secs(10))
        .call()
        .ok()?;
    let text = resp.into_string().ok()?;
    serde_json::from_str(&text).ok()
}

fn add(map: &mut BTreeMap<String, String>, list: &Value) {
    if let Some(arr) = list.as_array() {
        for e in arr {
            if let (Some(id), Some(name)) = (e["id"].as_str(), e["name"].as_str()) {
                map.insert(
                    name.to_string(),
                    format!("https://cdn.7tv.app/emote/{}/2x.webp", id),
                );
            }
        }
    }
}

pub fn fetch(platform: &str, ident: &str) -> String {
    let mut map = BTreeMap::new();
    let plat_id = if platform == "twitch" {
        if !ident.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            || ident.len() < 1
            || ident.len() > 25
        {
            return "{}".into();
        }
        let query = format!("{{\"query\":\"query{{user(login:\\\"{}\\\"){{id}}}}\"}}", ident);
        let resp = ureq::post("https://gql.twitch.tv/gql")
            .set("Client-ID", "kimne78kx3ncx6brgo4mv6wki5h1ko")
            .set("Content-Type", "application/json")
            .timeout(Duration::from_secs(10))
            .send_string(&query);
        let id = resp
            .ok()
            .and_then(|r| r.into_string().ok())
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["data"]["user"]["id"].as_str().map(String::from));
        if let Some(id) = id {
            id
        } else {
            let url = format!("https://api.ivr.fi/v2/twitch/user?login={}", ident);
            get(&url)
                .and_then(|v| v[0]["id"].as_str().map(String::from))
                .unwrap_or_else(|| return "{}".into())
        }
    } else {
        ident.to_string()
    };
    if let Some(v) = get(&format!("https://7tv.io/v3/users/{}/{}", platform, plat_id)) {
        add(&mut map, &v["emote_set"]["emotes"]);
    }
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
}
