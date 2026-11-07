use crate::store::Item;

const COMMON: &str = include_str!("js/common.js");
const SEVEN: &str = include_str!("js/seven.js");
const PREVIEW: &str = include_str!("js/preview.js");
const OVERLAY: &str = include_str!("js/overlay.js");

fn items_json(list: &[Item]) -> String {
    serde_json::to_string(list).unwrap_or_else(|_| "[]".into())
}

fn flag(v: bool) -> &'static str {
    if v {
        "true"
    } else {
        "false"
    }
}

fn fill(
    body: &str,
    hidden: &[Item],
    bgs: &[Item],
    fade: &[Item],
    fade_secs: u32,
    opacity: f64,
    auto: bool,
    seven: bool,
) -> String {
    let body = body
        .replace("__HIDDEN__", &items_json(hidden))
        .replace("__BGS__", &items_json(bgs))
        .replace("__FADE__", &items_json(fade))
        .replace("__FADESECS__", &fade_secs.to_string())
        .replace("__OPACITY__", &opacity.to_string())
        .replace("__AUTO__", flag(auto))
        .replace("__SEVEN__", flag(seven));
    format!("{}\n{}\n{}", COMMON, SEVEN, body)
}

pub fn preview(hidden: &[Item], bgs: &[Item], fade: &[Item], fade_secs: u32, opacity: f64, auto: bool, seven: bool) -> String {
    fill(PREVIEW, hidden, bgs, fade, fade_secs, opacity, auto, seven)
}

pub fn overlay(hidden: &[Item], bgs: &[Item], fade: &[Item], fade_secs: u32, opacity: f64, auto: bool, seven: bool, hide_border: bool, texts: &[String; 3]) -> String {
    let s = fill(OVERLAY, hidden, bgs, fade, fade_secs, opacity, auto, seven);
    let s = s.replace("__HIDE__", flag(hide_border));
    let s = s.replace("__T_OK__", &serde_json::to_string(&texts[0]).unwrap_or_else(|_| "\"\"".into()));
    let s = s.replace("__T_LOST__", &serde_json::to_string(&texts[1]).unwrap_or_else(|_| "\"\"".into()));
    s.replace("__T_LABEL__", &serde_json::to_string(&texts[2]).unwrap_or_else(|_| "\"\"".into()))
}

pub fn texts_js(texts: &[String; 3]) -> String {
    let a = serde_json::to_string(&texts[0]).unwrap_or_else(|_| "\"\"".into());
    let b = serde_json::to_string(&texts[1]).unwrap_or_else(|_| "\"\"".into());
    let c = serde_json::to_string(&texts[2]).unwrap_or_else(|_| "\"\"".into());
    format!("window.__acText&&window.__acText({},{},{})", a, b, c)
}

pub fn apply(hidden: &[Item], bgs: &[Item], fade: &[Item], fade_secs: u32, opacity: f64, auto: bool, seven: bool, hide_border: bool) -> String {
    format!(
        "window.__acApply&&window.__acApply({},{},{},{},{},{},{},{})",
        items_json(hidden),
        items_json(bgs),
        opacity,
        flag(auto),
        flag(seven),
        flag(!hide_border),
        items_json(fade),
        fade_secs
    )
}

pub fn emotes(json: &str) -> String {
    format!("window.__ac7tv&&window.__ac7tv({})", json)
}
