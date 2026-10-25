use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub struct Info {
    pub tag: String,
    pub url: String,
    pub asset: Option<Asset>,
}

pub struct Asset {
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

pub fn check(current: &str) -> Option<Info> {
    let resp = ureq::get("https://api.github.com/repos/Dralder/AllChats/releases?per_page=10")
        .set("User-Agent", "AllChats")
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(8))
        .call()
        .ok()?;
    let text = resp.into_string().ok()?;
    let releases: Vec<Value> = serde_json::from_str(&text).ok()?;
    
    let current_ver = parse_version(current)?;
    let mut highest: Option<((u32, u32, u32), String, String, Option<Asset>)> = None;
    
    for rel in releases {
        if rel["draft"].as_bool().unwrap_or(false) {
            continue;
        }
        let tag = rel["tag_name"].as_str()?;
        let url = rel["html_url"].as_str()?;
        let mut asset_opt = None;
        if let Some(assets) = rel["assets"].as_array() {
            for a in assets {
                if let Some(name) = a["name"].as_str() {
                    if name.to_lowercase().ends_with(".zip") {
                        let asset_url = a["browser_download_url"].as_str()?.to_string();
                        let size = a["size"].as_u64().unwrap_or(0);
                        let mut sha256_opt = None;
                        if let Some(digest) = a["digest"].as_str() {
                            if let Some(hash) = digest.strip_prefix("sha256:") {
                                sha256_opt = Some(hash.to_string());
                            }
                        }
                        asset_opt = Some(Asset {
                            url: asset_url,
                            size,
                            sha256: sha256_opt,
                        });
                        break;
                    }
                }
            }
        }
        if let Some(ver) = parse_version(tag) {
            if highest.is_none() || ver > highest.as_ref().unwrap().0 {
                highest = Some((ver, tag.to_string(), url.to_string(), asset_opt));
            }
        }
    }
    
    if let Some((ver, tag, url, asset)) = highest {
        if ver > current_ver {
            return Some(Info { tag, url, asset });
        }
    }
    None
}

pub fn download<F>(
    url: &str,
    dest_dir: &Path,
    expected_size: u64,
    sha256: Option<&str>,
    cancel: &AtomicBool,
    mut progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(u64, u64),
{
    if !url.starts_with("https://github.com/Dralder/AllChats/") {
        return Err("Invalid URL".to_string());
    }
    
    let part_path = dest_dir.join("AllChats-update.zip.part");
    let final_path = dest_dir.join("AllChats-update.zip");
    
    let resp = ureq::get(url)
        .set("User-Agent", "AllChats")
        .timeout(Duration::from_secs(30))
        .call()
        .map_err(|e| format!("Download failed: {}", e))?;
    
    let total = resp.header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    
    if total > 200 * 1024 * 1024 || (expected_size > 0 && expected_size > 200 * 1024 * 1024) {
        return Err("File too large".to_string());
    }
    
    let mut file = File::create(&part_path).map_err(|e| format!("Cannot create file: {}", e))?;
    let mut reader = resp.into_reader();
    let mut buf = [0u8; 65536];
    let mut downloaded = 0u64;
    let mut last_progress = Instant::now();
    
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = fs::remove_file(&part_path);
            return Err("Cancelled".to_string());
        }
        
        let n = reader.read(&mut buf).map_err(|e| {
            let _ = fs::remove_file(&part_path);
            format!("Read error: {}", e)
        })?;
        
        if n == 0 {
            break;
        }
        
        file.write_all(&buf[..n]).map_err(|e| {
            let _ = fs::remove_file(&part_path);
            format!("Write error: {}", e)
        })?;
        
        downloaded += n as u64;
        
        if downloaded > 200 * 1024 * 1024 {
            let _ = fs::remove_file(&part_path);
            return Err("File too large".to_string());
        }
        
        if last_progress.elapsed() >= Duration::from_millis(100) {
            progress(downloaded, total);
            last_progress = Instant::now();
        }
    }
    
    drop(file);
    progress(downloaded, total);
    
    if total > 0 && downloaded != total {
        let _ = fs::remove_file(&part_path);
        return Err("Incomplete download".to_string());
    }
    
    let mut file = File::open(&part_path).map_err(|e| {
        let _ = fs::remove_file(&part_path);
        format!("Cannot reopen file: {}", e)
    })?;
    
    let mut header = [0u8; 4];
    file.read_exact(&mut header).map_err(|e| {
        let _ = fs::remove_file(&part_path);
        format!("Cannot read header: {}", e)
    })?;
    
    if header != [0x50, 0x4B, 0x03, 0x04] {
        let _ = fs::remove_file(&part_path);
        return Err("Invalid ZIP file".to_string());
    }
    
    if let Some(expected_hash) = sha256 {
        file.seek(std::io::SeekFrom::Start(0)).map_err(|e| {
            let _ = fs::remove_file(&part_path);
            format!("Cannot seek: {}", e)
        })?;
        
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 65536];
        loop {
            let n = file.read(&mut buf).map_err(|e| {
                let _ = fs::remove_file(&part_path);
                format!("Cannot hash: {}", e)
            })?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        
        let hash = format!("{:x}", hasher.finalize());
        if hash != expected_hash.to_lowercase() {
            let _ = fs::remove_file(&part_path);
            return Err("Checksum mismatch".to_string());
        }
    }
    
    drop(file);
    
    fs::rename(&part_path, &final_path).map_err(|e| {
        let _ = fs::remove_file(&part_path);
        format!("Cannot rename: {}", e)
    })?;
    
    Ok(final_path)
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
