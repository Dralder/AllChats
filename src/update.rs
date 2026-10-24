use serde_json::Value;
use std::time::Duration;

pub fn check(current: &str) -> Option<(String, String)> {
    let resp = ureq::get("https://api.github.com/repos/Dralder/AllChats/releases?per_page=10")
        .set("User-Agent", "AllChats")
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(8))
        .call()
        .ok()?;
    let text = resp.into_string().ok()?;
    let releases: Vec<Value> = serde_json::from_str(&text).ok()?;
    
    let current_ver = parse_version(current)?;
    let mut highest: Option<((u32, u32, u32), String, String)> = None;
    
    for rel in releases {
        if rel["draft"].as_bool().unwrap_or(false) {
            continue;
        }
        let tag = rel["tag_name"].as_str()?;
        let url = rel["html_url"].as_str()?;
        if let Some(ver) = parse_version(tag) {
            if highest.is_none() || ver > highest.as_ref().unwrap().0 {
                highest = Some((ver, tag.to_string(), url.to_string()));
            }
        }
    }
    
    if let Some((ver, tag, url)) = highest {
        if ver > current_ver {
            return Some((tag, url));
        }
    }
    None
}

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let parts: Vec<&str> = s.split(|c: char| !c.is_ascii_digit() && c != '.').next()?.split('.').collect();
    if parts.len() < 3 {
        return None;
    }
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    let patch = parts[2].parse().ok()?;
    Some((major, minor, patch))
}
