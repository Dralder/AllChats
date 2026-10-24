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
    opacity: f64,
    auto: bool,
    seven: bool,
) -> String {
    let body = body
        .replace("__HIDDEN__", &items_json(hidden))
        .replace("__BGS__", &items_json(bgs))
        .replace("__OPACITY__", &opacity.to_string())
        .replace("__AUTO__", flag(auto))
        .replace("__SEVEN__", flag(seven));
    format!("{}\n{}\n{}", COMMON, SEVEN, body)
}

pub fn preview(hidden: &[Item], bgs: &[Item], opacity: f64, auto: bool, seven: bool) -> String {
    fill(PREVIEW, hidden, bgs, opacity, auto, seven)
}

pub fn overlay(hidden: &[Item], bgs: &[Item], opacity: f64, auto: bool, seven: bool, hide_border: bool) -> String {
    let s = fill(OVERLAY, hidden, bgs, opacity, auto, seven);
    s.replace("__HIDE__", flag(hide_border))
}

pub fn apply(hidden: &[Item], bgs: &[Item], opacity: f64, auto: bool, seven: bool, hide_border: bool) -> String {
    format!(
        "window.__acApply&&window.__acApply({},{},{},{},{},{})",
        items_json(hidden),
        items_json(bgs),
        opacity,
        flag(auto),
        flag(seven),
        flag(!hide_border)
    )
}

pub fn emotes(json: &str) -> String {
    format!("window.__ac7tv&&window.__ac7tv({})", json)
}
