#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod lang;
mod scripts;
mod seven;
mod store;
mod topmost;
mod typer;
mod update;

use std::time::{Duration, Instant};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tao::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{
    ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget,
};
use tao::window::{Icon, Window, WindowBuilder, WindowId};
use wry::http::Request;
use wry::{PageLoadEvent, WebContext, WebView, WebViewBuilder};

use std::collections::{HashMap, HashSet};
use store::{Geo, Item, Opt, Profile, Saved, Settings, Store, WinCfg};

const UI_HTML: &str = include_str!("ui.html");
const BAR_HTML: &str = include_str!("winbar.html");
const PICKER_HTML: &str = include_str!("picker.html");
const BAR_H: f64 = 32.0;
const ICON_BYTES: &[u8] = include_bytes!("../allchats.ico");

#[cfg(windows)]
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion,OptimizationHints,MediaRouter,Translate --enable-features=RemoveRedirectionBitmap --disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows --disable-smooth-scrolling --disable-background-networking --disable-breakpad --no-pings";

const SIZE_OVERLAY: (f64, f64) = (780.0, 475.0);

const SIZE_WINDOW: (f64, f64) = (480.0, 372.0);

fn ctl_size(mode: &str) -> LogicalSize<f64> {
    if mode == "window" {
        LogicalSize::new(SIZE_WINDOW.0, SIZE_WINDOW.1)
    } else {
        LogicalSize::new(SIZE_OVERLAY.0, SIZE_OVERLAY.1)
    }
}

type Target = EventLoopWindowTarget<UserEvent>;

enum UserEvent {
    Ui(String),
    Web(String),
    PreviewLoaded,
    OverlayLoaded,
    WinLoaded(String, String),
    TypeTick(String),
    Img(String, String),
    Emotes(String, String, String, bool),
    Prep(String, String, String),
    Flush,
    Update(update::Info),
    UpdateProgress(u64, u64),
    Raise(bool),
    UpdateReady(PathBuf),
    UpdateFailed(String),
}

struct Win {
    view: WebView,
    window: Window,
}

struct Pane {
    view: WebView,
    bar: WebView,
    window: Window,
    picker: Option<WebView>,
    picker_on: bool,
}

fn layout_pane(p: &Pane) {
    let s = p.window.inner_size();
    let bh = ((BAR_H * p.window.scale_factor()).round() as u32).min(s.height);
    let _ = p.bar.set_bounds(wry::Rect {
        position: wry::dpi::PhysicalPosition::new(0, 0).into(),
        size: wry::dpi::PhysicalSize::new(s.width, bh).into(),
    });
    let _ = p.view.set_bounds(wry::Rect {
        position: wry::dpi::PhysicalPosition::new(0, bh as i32).into(),
        size: wry::dpi::PhysicalSize::new(s.width, s.height - bh).into(),
    });
    if let Some(pk) = &p.picker {
        let sf = p.window.scale_factor();
        let m = (8.0 * sf).round() as u32;
        let w = (((340.0 * sf).round() as u32).min(s.width.saturating_sub(m * 2))).max(1);
        let h = (((420.0 * sf).round() as u32).min(s.height.saturating_sub(bh + m * 2))).max(1);
        let _ = pk.set_bounds(wry::Rect {
            position: wry::dpi::PhysicalPosition::new(m as i32, (bh + m) as i32).into(),
            size: wry::dpi::PhysicalSize::new(w, h).into(),
        });
    }
}

fn load_icon() -> Option<Icon> {
    let img = image::load_from_memory_with_format(ICON_BYTES, image::ImageFormat::Ico)
        .ok()?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).ok()
}

#[cfg(windows)]
fn apply_window_attrs(window: &Window) {
    use tao::platform::windows::WindowExtWindows;
    let hwnd = window.hwnd() as isize;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_DONOTROUND: u32 = 1;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_COLOR_NONE: u32 = 0xFFFFFFFE;
    const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
    const DWMSBT_NONE: u32 = 1;
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(hwnd: isize, attr: u32, value: *const u32, size: u32) -> i32;
    }
    unsafe {
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_DONOTROUND, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &DWMWA_COLOR_NONE, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &DWMSBT_NONE, 4);
    }
}

#[cfg(not(windows))]
fn apply_window_attrs(_window: &Window) {}

#[cfg(windows)]
fn base_window() -> WindowBuilder {
    use tao::platform::windows::WindowBuilderExtWindows;
    WindowBuilder::new()
        .with_window_icon(load_icon())
        .with_taskbar_icon(load_icon())
}

#[cfg(not(windows))]
fn base_window() -> WindowBuilder {
    WindowBuilder::new().with_window_icon(load_icon())
}

#[cfg(windows)]
fn tune<'a>(builder: WebViewBuilder<'a>) -> WebViewBuilder<'a> {
    use wry::WebViewBuilderExtWindows;
    builder.with_additional_browser_args(BROWSER_ARGS)
}

#[cfg(not(windows))]
fn tune<'a>(builder: WebViewBuilder<'a>) -> WebViewBuilder<'a> {
    builder
}

#[cfg(target_os = "linux")]
fn build_view(builder: WebViewBuilder<'_>, window: &Window) -> WebView {
    use tao::platform::unix::WindowExtUnix;
    use wry::WebViewBuilderExtUnix;
    let vbox = window.default_vbox().expect("window container");
    builder.build_gtk(vbox).expect("webview")
}

#[cfg(windows)]
fn build_child(builder: WebViewBuilder<'_>, window: &Window) -> WebView {
    builder.build_as_child(window).expect("webview")
}

#[cfg(not(windows))]
fn build_child(builder: WebViewBuilder<'_>, window: &Window) -> WebView {
    build_view(builder, window)
}

#[cfg(not(target_os = "linux"))]
fn build_view(builder: WebViewBuilder<'_>, window: &Window) -> WebView {
    builder.build(window).expect("webview")
}

#[cfg(windows)]
fn lower_priority() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, SetPriorityClass, BELOW_NORMAL_PRIORITY_CLASS,
    };
    unsafe {
        let _ = SetPriorityClass(GetCurrentProcess(), BELOW_NORMAL_PRIORITY_CLASS);
    }
}

#[cfg(not(windows))]
fn lower_priority() {}

fn norm(url: &str) -> String {
    let u = url.trim();
    if u.is_empty() {
        return "about:blank".into();
    }
    if u.contains("://") || u.starts_with("about:") || u.starts_with("file:") {
        u.to_string()
    } else {
        format!("https://{}", u)
    }
}

fn names(list: &[&Item]) -> Vec<String> {
    list.iter().map(|i| i.name.clone()).collect()
}

fn clean_user(s: &str) -> String {
    s.trim()
        .trim_start_matches('@')
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

fn win_link(c: &WinCfg) -> Option<String> {
    link_for(c, &c.kind)
}

fn win_opt(c: &WinCfg, kind: &str) -> Opt {
    c.opts.get(kind).cloned().unwrap_or(Opt { topmost: c.topmost, seven: c.seven, zoom: 1.0 })
}

fn win_geo(c: &WinCfg, kind: &str) -> Geo {
    c.geo.get(kind).cloned().unwrap_or(Geo { x: c.x, y: c.y, w: c.w, h: c.h })
}

fn build_link(kind: &str, url: &str, kick: &str, twitch: &str) -> Option<String> {
    match kind {
        "kick" => {
            let u = clean_user(kick);
            if u.is_empty() {
                None
            } else {
                Some(format!("https://kick.com/popout/{}/chat", u))
            }
        }
        "twitch" => {
            let u = clean_user(twitch);
            if u.is_empty() {
                None
            } else {
                Some(format!("https://www.twitch.tv/popout/{}/chat?popout=", u))
            }
        }
        _ => {
            let u = url.trim();
            if u.is_empty() {
                None
            } else {
                Some(norm(u))
            }
        }
    }
}

fn link_for(c: &WinCfg, kind: &str) -> Option<String> {
    if let Some(id) = kind.strip_prefix("c:") {
        let u = c.saved.iter().find(|s| s.id == id)?.url.trim().to_string();
        return if u.is_empty() { None } else { Some(norm(&u)) };
    }
    build_link(kind, &c.url, &c.kick, &c.twitch)
}

fn kind_ok(c: &WinCfg, kind: &str) -> bool {
    matches!(kind, "custom" | "kick" | "twitch")
        || kind.strip_prefix("c:").map(|id| c.saved.iter().any(|s| s.id == id)).unwrap_or(false)
}

fn all_kinds(c: &WinCfg) -> Vec<String> {
    let mut v: Vec<String> = vec!["custom".into(), "kick".into(), "twitch".into()];
    for s in &c.saved {
        v.push(format!("c:{}", s.id));
    }
    v
}

fn clean_name(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '<' | '>' | '&' | '"' | '\\' | '\n' | '\r')).collect::<String>().trim().chars().take(24).collect()
}

fn prof_link(p: &Profile) -> Option<String> {
    build_link(&p.kind, &p.url, &p.kick, &p.twitch)
}

fn is_7tv_host(url: &str) -> bool {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#', ':']).next().unwrap_or("").to_lowercase();
    ["kick.com", "twitch.tv", "youtube.com"]
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{}", d)))
}

fn resize_dir(dir: &str) -> Option<tao::window::ResizeDirection> {
    use tao::window::ResizeDirection as R;
    match dir {
        "W" => Some(R::West),
        "E" => Some(R::East),
        "N" => Some(R::North),
        "NW" => Some(R::NorthWest),
        "NE" => Some(R::NorthEast),
        "S" => Some(R::South),
        "SW" => Some(R::SouthWest),
        "SE" => Some(R::SouthEast),
        _ => None,
    }
}

fn on_screen(t: &Target, x: i32, y: i32) -> bool {
    t.available_monitors().any(|m| {
        let p = m.position();
        let s = m.size();
        x >= p.x - 40 && y >= p.y - 40 && x < p.x + s.width as i32 && y < p.y + s.height as i32
    })
}

fn make_ctx(store: &Store, id: &str) -> WebContext {
    WebContext::new(Some(store.data_dir(id)))
}

struct App {
    store: Store,
    profiles: Vec<Profile>,
    cur: usize,
    cands: Vec<Item>,
    emotes: HashMap<String, (String, Instant)>,
    pickers: HashMap<String, String>,
    flush_pending: bool,
    fetching: HashSet<String>,
    seven_want: HashMap<String, String>,
    proxy: EventLoopProxy<UserEvent>,
    ctl: Win,
    preview: Option<Win>,
    help: Option<Win>,
    overlay: Option<Win>,
    prof_ctx: WebContext,
    update: Option<update::Info>,
    upd_busy: bool,
    upd_cancel: Arc<AtomicBool>,
    lang: String,
    mode: String,
    wcfg: WinCfg,
    ctl_x: Option<i32>,
    ctl_y: Option<i32>,
    wins: HashMap<String, Pane>,
    win_url: HashMap<String, String>,
    imgs: HashMap<String, String>,
    overlay_want: bool,
    booted: bool,
    quitting: bool,
    img_wait: HashMap<String, Vec<String>>,
    typeq: HashMap<String, (String, Vec<String>, Instant, Instant)>,
    win_ctx: WebContext,
    login: Option<Win>,
}

impl App {
    fn new(target: &Target, proxy: EventLoopProxy<UserEvent>) -> Self {
        let store = Store::new();
        let settings = store.load_settings();
        let mut profiles = store.load_profiles();
        if profiles.is_empty() {
            let p = Profile::named("Default");
            store.save_profile(&p);
            profiles.push(p);
        }
        let cur = profiles
            .iter()
            .position(|p| p.name == settings.last_profile)
            .unwrap_or(0);
        let mut ids: Vec<String> = profiles.iter().map(|p| p.id.clone()).collect();
        ids.push("window".to_string());
        store.cleanup(&ids);

        let upd_dir = store.root.join("update");
        let _ = std::fs::remove_file(upd_dir.join("AllChats-update.zip"));
        let _ = std::fs::remove_file(upd_dir.join("AllChats-update.zip.part"));
        let _ = std::fs::remove_dir_all(upd_dir.join("extract"));

        let mode = if settings.mode == "window" { "window".to_string() } else { "overlay".to_string() };
        let mut wcfg = settings.win.clone();
        if !kind_ok(&wcfg, &wcfg.kind) {
            wcfg.kind = "custom".to_string();
        }
        let settings_overlay = settings.overlay_open.unwrap_or(mode != "window");
        let ctl_x = settings.ctl_x;
        let ctl_y = settings.ctl_y;

        let prof_ctx = make_ctx(&store, &profiles[cur].id);

        let mut wb = base_window()
            .with_title("AllChats")
            .with_decorations(false)
            .with_resizable(false)
            .with_inner_size(ctl_size(&mode));
        if let (Some(x), Some(y)) = (ctl_x, ctl_y) {
            if on_screen(target, x, y) {
                wb = wb.with_position(PhysicalPosition::new(x, y));
            }
        }
        let window = wb.build(target).expect("window");

        let p = proxy.clone();
        let mut ctl_ctx = WebContext::new(Some(store.ui_dir()));
        let html = UI_HTML.replace("/*__FONTS__*/", &lang::font_css(None));
        let builder = WebViewBuilder::with_web_context(&mut ctl_ctx)
            .with_html(&html)
            .with_background_color((32, 32, 32, 255))
            .with_ipc_handler(move |req: Request<String>| {
                let _ = p.send_event(UserEvent::Ui(req.into_body()));
            });
        let view = build_view(builder, &window);

        let win_ctx = make_ctx(&store, "window");
        let lang = if lang::valid(&settings.lang) { settings.lang } else { String::new() };

        Self {
            store,
            profiles,
            cur,
            cands: Vec::new(),
            emotes: HashMap::new(),
            pickers: HashMap::new(),
            flush_pending: false,
            fetching: HashSet::new(),
            seven_want: HashMap::new(),
            proxy,
            ctl: Win { view, window },
            preview: None,
            help: None,
            overlay: None,
            prof_ctx,
            update: None,
            upd_busy: false,
            upd_cancel: Arc::new(AtomicBool::new(false)),
            lang,
            mode,
            wcfg,
            ctl_x,
            ctl_y,
            wins: HashMap::new(),
            win_url: HashMap::new(),
            imgs: HashMap::new(),
            overlay_want: settings_overlay,
            booted: false,
            quitting: false,
            img_wait: HashMap::new(),
            typeq: HashMap::new(),
            win_ctx,
            login: None,
        }
    }

    fn start_if_url(&mut self, t: &Target) {
        if prof_link(self.profile()).is_some() {
            self.open_overlay(t);
        }
    }

    fn open_win(&mut self, kind: &str, t: &Target) {
        if let Some(w) = self.wins.get(kind) {
            w.window.set_focus();
            return;
        }
        let link = match link_for(&self.wcfg, kind) {
            Some(l) => l,
            None => return,
        };
        let code = if self.lang.is_empty() { "en" } else { &self.lang };
        let map = lang::load(&self.store.root, code);
        let script = scripts::window();
        let title = match kind.strip_prefix("c:") {
            Some(id) => self.wcfg.saved.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_default(),
            None => lang::text(&map, &format!("w.win.{}", kind)),
        };
        let c = self.wcfg.clone();
        let g = win_geo(&c, kind);
        let mut wb = base_window()
            .with_title(&title)
            .with_decorations(false)
            .with_resizable(true)
            .with_always_on_top(win_opt(&c, kind).topmost)
            .with_inner_size(PhysicalSize::new(g.w.max(200), g.h.max(200)));
        if let (Some(x), Some(y)) = (g.x, g.y) {
            if on_screen(t, x, y) {
                wb = wb.with_position(PhysicalPosition::new(x, y));
            }
        }
        let window = wb.build(t).expect("window");
        let p1 = self.proxy.clone();
        let p2 = self.proxy.clone();
        let k1 = kind.to_string();
        let k2 = kind.to_string();
        let builder = WebViewBuilder::with_web_context(&mut self.win_ctx)
            .with_background_color((32, 32, 32, 255))
            .with_url(&link)
            .with_initialization_script(&script)
            .with_ipc_handler(move |req: Request<String>| {
                let body = req.into_body();
                if body.starts_with("{\"t\":\"7tv") || body.starts_with("{\"t\":\"wresize\"") {
                    let tagged = body.replacen('{', &format!("{{\"k\":\"{}\",", k1), 1);
                    let _ = p1.send_event(UserEvent::Web(tagged));
                }
            })
            .with_on_page_load_handler(move |_ev: PageLoadEvent, url: String| {
                let _ = p2.send_event(UserEvent::WinLoaded(k2.clone(), url));
            });
        let view = build_child(tune(builder), &window);
        let k3 = kind.to_string();
        let p3 = self.proxy.clone();
        let bar_html = BAR_HTML.replace("/*TITLE*/AllChats Window", &title.replace('<', "")).replace("/*SEVEN*/false", if self.seven_btn(kind) { "true" } else { "false" });
        let bar_builder = WebViewBuilder::with_web_context(&mut self.win_ctx)
            .with_background_color((32, 32, 32, 255))
            .with_html(&bar_html)
            .with_ipc_handler(move |req: Request<String>| {
                let msg = format!("{{\"k\":\"{}\",\"t\":\"wbar\",\"b\":{}}}", k3, req.into_body());
                let _ = p3.send_event(UserEvent::Web(msg));
            });
        let bar = build_child(tune(bar_builder), &window);
        let pane = Pane { view, bar, window, picker: None, picker_on: false };
        layout_pane(&pane);
        self.wins.insert(kind.to_string(), pane);
        if !self.wcfg.open.iter().any(|k| k == kind) {
            self.wcfg.open.push(kind.to_string());
            self.save_settings();
        }
    }

    fn apply_win(&self, kind: &str) {
        if let Some(w) = self.wins.get(kind) {
            let o = win_opt(&self.wcfg, kind);
            let _ = w.view.evaluate_script(&scripts::seven_cfg(o.seven));
            let _ = w.view.zoom(o.zoom);
        }
    }

    fn seven_btn(&self, kind: &str) -> bool {
        let cur = self.win_url.get(kind).cloned().or_else(|| link_for(&self.wcfg, kind)).unwrap_or_default();
        win_opt(&self.wcfg, kind).seven && is_7tv_host(&cur)
    }

    fn sync_bars(&mut self) {
        let kinds: Vec<String> = self.wins.keys().cloned().collect();
        for k in kinds {
            let on = self.seven_btn(&k);
            if let Some(p) = self.wins.get_mut(&k) {
                let _ = p.bar.evaluate_script(&format!("window.__seven&&window.__seven({})", on));
                if !on {
                    p.picker_on = false;
                    if let Some(pk) = &p.picker {
                        let _ = pk.set_visible(false);
                    }
                }
            }
        }
    }

    fn push_picker(&self, kind: &str) {
        if let Some(p) = self.wins.get(kind) {
            if let Some(pk) = &p.picker {
                let data = self
                    .seven_want
                    .get(kind)
                    .and_then(|k| self.pickers.get(k))
                    .cloned()
                    .unwrap_or_else(|| "{}".to_string());
                let _ = pk.evaluate_script(&format!("window.__load({})", data));
            }
        }
    }

    fn toggle_picker(&mut self, kind: &str) {
        let code = if self.lang.is_empty() { "en".to_string() } else { self.lang.clone() };
        let map = lang::load(&self.store.root, &code);
        let texts = serde_json::json!({
            "search": lang::text(&map, "w.emote.search"),
            "none": lang::text(&map, "w.emote.none"),
            "chan": lang::text(&map, "w.emote.chan"),
            "glob": lang::text(&map, "w.emote.glob"),
            "noinput": lang::text(&map, "w.emote.noinput"),
            "fail": lang::text(&map, "w.emote.fail"),
        });
        let html = PICKER_HTML.replace("/*TEXTS*/{}", &texts.to_string());
        let px = self.proxy.clone();
        let k = kind.to_string();
        let p = match self.wins.get_mut(kind) {
            Some(p) => p,
            None => return,
        };
        if p.picker_on {
            p.picker_on = false;
            if let Some(pk) = &p.picker {
                let _ = pk.set_visible(false);
            }
            let _ = p.view.focus();
            return;
        }
        if p.picker.is_none() {
            let builder = WebViewBuilder::with_web_context(&mut self.win_ctx)
                .with_background_color((27, 27, 31, 255))
                .with_html(&html)
                .with_ipc_handler(move |req: Request<String>| {
                    let msg = format!("{{\"k\":\"{}\",\"t\":\"wpick\",\"b\":{}}}", k, req.into_body());
                    let _ = px.send_event(UserEvent::Web(msg));
                });
            p.picker = Some(build_child(tune(builder), &p.window));
        }
        p.picker_on = true;
        layout_pane(p);
        if let Some(pk) = &p.picker {
            let _ = pk.set_visible(true);
            let _ = pk.focus();
        }
        self.push_picker(kind);
        if let Some(pk) = self.wins.get(kind).and_then(|p| p.picker.as_ref()) {
            let _ = pk.evaluate_script("window.__show&&window.__show()");
        }
    }

    fn quit(&mut self) -> bool {
        for k in self.wins.keys().cloned().collect::<Vec<String>>() {
            if !self.wcfg.open.contains(&k) {
                self.wcfg.open.push(k);
            }
        }
        topmost::set_active(false);
        self.persist();
        self.quitting = true;
        self.ctl.window.set_visible(false);
        for w in self.wins.values() {
            w.window.set_visible(false);
        }
        for w in [&self.overlay, &self.login, &self.preview, &self.help].into_iter().flatten() {
            w.window.set_visible(false);
        }
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(1500));
            std::process::exit(0);
        });
        true
    }

    fn close_win(&mut self, kind: &str) {
        self.wins.remove(kind);
        self.win_url.remove(kind);
        self.wcfg.open.retain(|x| x != kind);
        self.save_settings();
        self.push();
    }

    fn win_kind_of(&self, id: WindowId) -> Option<String> {
        self.wins
            .iter()
            .find(|(_, w)| w.window.id() == id)
            .map(|(k, _)| k.clone())
    }

    fn reopen_wins(&mut self, t: &Target) {
        let kinds: Vec<String> = self.wins.keys().cloned().collect();
        self.wins.clear();
        for k in kinds {
            self.open_win(&k, t);
        }
    }

    fn open_login(&mut self, t: &Target) {
        let url = match self.wcfg.kind.as_str() {
            "kick" => "https://kick.com/",
            "twitch" => "https://www.twitch.tv/login",
            _ => return,
        };
        if let Some(w) = &self.login {
            let _ = w.view.load_url(url);
            w.window.set_focus();
            return;
        }
        let window = base_window()
            .with_title("AllChats Login")
            .with_decorations(true)
            .with_resizable(true)
            .with_always_on_top(true)
            .with_inner_size(LogicalSize::new(1000.0, 760.0))
            .build(t)
            .expect("window");
        let builder = WebViewBuilder::with_web_context(&mut self.win_ctx)
            .with_background_color((32, 32, 32, 255))
            .with_url(url);
        let view = build_view(tune(builder), &window);
        self.login = Some(Win { view, window });
    }

    fn boot(&mut self, t: &Target) {
        if self.booted {
            return;
        }
        self.booted = true;
        self.start_mode(t);
        self.push();
    }

    fn start_mode(&mut self, t: &Target) {
        if self.overlay_want {
            self.start_if_url(t);
        }
        let saved = self.wcfg.open.clone();
        for k in saved {
            if kind_ok(&self.wcfg, &k) {
                self.open_win(&k, t);
            }
        }
    }

    fn schedule_flush(&mut self) {
        if self.flush_pending {
            return;
        }
        self.flush_pending = true;
        let px = self.proxy.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            let _ = px.send_event(UserEvent::Flush);
        });
    }

    fn sgeo_save(&mut self) {
        self.schedule_flush();
    }

    fn profile(&self) -> &Profile {
        &self.profiles[self.cur]
    }

    fn profile_mut(&mut self) -> &mut Profile {
        &mut self.profiles[self.cur]
    }

    fn persist(&self) {
        if self.quitting {
            return;
        }
        self.store.save_profile(self.profile());
        self.save_settings();
    }

    fn save_settings(&self) -> Settings {
        let s = Settings {
            last_profile: self.profile().name.clone(),
            lang: self.lang.clone(),
            mode: self.mode.clone(),
            win: self.wcfg.clone(),
            ctl_x: self.ctl_x,
            ctl_y: self.ctl_y,
            overlay_open: Some(self.overlay_want),
        };
        if !self.quitting {
            self.store.save_settings(&s);
        }
        s
    }

    fn cand_view(&self) -> Vec<&Item> {
        let bgs = &self.profile().backgrounds;
        self.cands
            .iter()
            .filter(|c| !bgs.iter().any(|b| b.sel == c.sel))
            .collect()
    }

    fn state(&self) -> String {
        let p = self.profile();
        let bgs: Vec<&Item> = p.backgrounds.iter().collect();
        let update_val = self.update.as_ref().map(|info| {
            json!({
                "tag": info.tag,
                "url": info.url,
                "asset": info.asset.is_some(),
                "size": info.asset.as_ref().map(|a| a.size).unwrap_or(0)
            })
        });
        json!({
            "profiles": self.profiles.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            "current": p.name,
            "url": p.url,
            "kind": p.kind,
            "kick": p.kick,
            "twitch": p.twitch,
            "link": prof_link(p).unwrap_or_default(),
            "opacity": p.opacity,
            "hide_border": p.hide_border,
            "auto_bg": p.auto_bg,
            "seven_tv": p.seven_tv,
            "zoom": p.zoom,
            "conn_check": p.conn_check,
            "fade": names(&p.fade.iter().collect::<Vec<_>>()),
            "fade_secs": p.fade_secs,
            "bgs": names(&bgs),
            "cands": names(&self.cand_view()),
            "overlay": self.overlay.is_some(),
            "preview": self.preview.is_some(),
            "update": update_val,
            "lang": &self.lang,
            "mode": self.mode,
            "w": {
                "kind": &self.wcfg.kind,
                "url": &self.wcfg.url,
                "kick": &self.wcfg.kick,
                "twitch": &self.wcfg.twitch,
                "topmost": win_opt(&self.wcfg, &self.wcfg.kind).topmost,
                "seven": win_opt(&self.wcfg, &self.wcfg.kind).seven,
                "zoom": win_opt(&self.wcfg, &self.wcfg.kind).zoom,
                "running": self.wins.contains_key(&self.wcfg.kind),
                "saved": self.wcfg.saved.iter().map(|s| json!({ "id": s.id, "name": s.name, "url": s.url, "running": self.wins.contains_key(&format!("c:{}", s.id)) })).collect::<Vec<_>>(),
                "link": win_link(&self.wcfg).unwrap_or_default()
            }
        })
        .to_string()
    }

    fn push(&self) {
        let _ = self
            .ctl
            .view
            .evaluate_script(&format!("window.setState({})", self.state()));
    }

    fn apply_js(&self) -> String {
        let p = self.profile();
        scripts::apply(&p.hidden, &p.backgrounds, &p.fade, p.fade_secs, p.opacity, p.auto_bg, p.seven_tv, p.hide_border)
    }

    fn apply_overlay(&self) {
        if let Some(o) = &self.overlay {
            let _ = o.view.evaluate_script(&self.apply_js());
        }
    }

    fn apply_zoom(&self) {
        if let Some(o) = &self.overlay {
            let z = self.profile().zoom;
            let _ = o
                .view
                .evaluate_script(&format!("window.__acSetZoom&&window.__acSetZoom({})", z));
            let _ = o.view.zoom(z);
        }
    }

    fn net_js(&self, on: bool) {
        if let Some(o) = &self.overlay {
            let _ = o
                .view
                .evaluate_script(&format!("window.__acNetEnable&&window.__acNetEnable({})", on));
        }
    }

    fn apply_preview(&self) {
        if let Some(w) = &self.preview {
            let _ = w.view.evaluate_script(&self.apply_js());
        }
    }

    fn apply_all(&self) {
        self.apply_overlay();
        self.apply_preview();
    }

    fn reply_img(&self, target: &str, url: &str, data: &str) {
        let u = serde_json::to_string(url).unwrap_or_else(|_| "\"\"".into());
        let d = serde_json::to_string(data).unwrap_or_else(|_| "\"\"".into());
        if let Some(kind) = target.strip_prefix("p:") {
            if let Some(pk) = self.wins.get(kind).and_then(|p| p.picker.as_ref()) {
                let _ = pk.evaluate_script(&format!("window.__img&&window.__img({},{})", u, d));
            }
        } else {
            self.send_seven_to(target, &format!("window.__ac7img&&window.__ac7img({},{})", u, d));
        }
    }

    fn want_img(&mut self, target: String, url: String) {
        if !url.starts_with("https://cdn.7tv.app/emote/") {
            self.reply_img(&target, &url, "");
            return;
        }
        if let Some(d) = self.imgs.get(&url).cloned() {
            self.reply_img(&target, &url, &d);
            return;
        }
        let first = !self.img_wait.contains_key(&url);
        self.img_wait.entry(url.clone()).or_default().push(target);
        if first {
            let px = self.proxy.clone();
            std::thread::spawn(move || {
                let data = seven::image(&url);
                let _ = px.send_event(UserEvent::Img(url, data));
            });
        }
    }

    fn send_seven_to(&self, target: &str, js: &str) {
        if target == "overlay" {
            if let Some(o) = &self.overlay {
                let _ = o.view.evaluate_script(js);
            }
            if let Some(w) = &self.preview {
                let _ = w.view.evaluate_script(js);
            }
        } else if let Some(w) = self.wins.get(target) {
            let _ = w.view.evaluate_script(js);
        }
    }

    fn scan(&self) {
        let w = self.preview.as_ref().or(self.overlay.as_ref());
        if let Some(w) = w {
            let _ = w
                .view
                .evaluate_script("window.__acScan&&window.__acScan()");
        }
    }

    fn send_lang(&self) {
        if self.lang.is_empty() {
            return;
        }
        let map = lang::load(&self.store.root, &self.lang);
        let dict = serde_json::Value::Object(map.into_iter().collect());
        let code_json = serde_json::to_string(&self.lang).unwrap_or_default();
        let dict_json = serde_json::to_string(&dict).unwrap_or_default();
        let js = format!("window.setLang({},{})", code_json, dict_json);
        let _ = self.ctl.view.evaluate_script(&js);
    }

    fn overlay_texts(&self) -> [String; 3] {
        let code = if self.lang.is_empty() { "en" } else { &self.lang };
        let map = lang::load(&self.store.root, code);
        [
            lang::text(&map, "ov.conn.ok"),
            lang::text(&map, "ov.conn.lost"),
            lang::text(&map, "ov.label"),
        ]
    }

    fn close_overlay(&mut self) {
        topmost::set_active(false);
        if self.overlay.is_some() {
            self.persist();
            self.overlay = None;
        }
    }

    fn raise_overlay(&self, from_fg: bool) {
        #[cfg(windows)]
        if let Some(o) = &self.overlay {
            use tao::platform::windows::WindowExtWindows;
            topmost::apply(o.window.hwnd() as isize, from_fg);
        }
    }

    fn open_overlay(&mut self, t: &Target) {
        if !self.overlay_want {
            self.overlay_want = true;
            self.save_settings();
        }
        if self.overlay.is_some() {
            return;
        }
        let p = self.profile().clone();
        let url = norm(&prof_link(&p).unwrap_or_default());
        let texts = self.overlay_texts();
        #[cfg(windows)]
        let window = {
            use tao::platform::windows::WindowBuilderExtWindows;
            base_window()
                .with_title("AllChats Overlay")
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top(true)
                .with_focused(false)
                .with_no_redirection_bitmap(true)
                .with_undecorated_shadow(false)
                .with_inner_size(PhysicalSize::new(p.w.max(100), p.h.max(100)))
                .with_position(PhysicalPosition::new(p.x, p.y))
                .build(t)
                .expect("window")
        };
        #[cfg(not(windows))]
        let window = base_window()
            .with_title("AllChats Overlay")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(true)
            .with_focused(false)
            .with_inner_size(PhysicalSize::new(p.w.max(100), p.h.max(100)))
            .with_position(PhysicalPosition::new(p.x, p.y))
            .build(t)
            .expect("window");
        apply_window_attrs(&window);
        #[cfg(windows)]
        {
            use tao::platform::windows::WindowExtWindows;
            let _ = window.set_skip_taskbar(true);
            topmost::apply(window.hwnd() as isize, false);
        }
        if p.hide_border {
            let _ = window.set_ignore_cursor_events(true);
        }
        let script = scripts::overlay(&p.hidden, &p.backgrounds, &p.fade, p.fade_secs, p.opacity, p.auto_bg, p.seven_tv, p.hide_border, &texts);
        let p1 = self.proxy.clone();
        let p2 = self.proxy.clone();
        let builder = WebViewBuilder::with_web_context(&mut self.prof_ctx)
            .with_transparent(true)
            .with_background_color((0, 0, 0, 0))
            .with_url(&url)
            .with_initialization_script(&script)
            .with_ipc_handler(move |req: Request<String>| {
                let _ = p1.send_event(UserEvent::Web(req.into_body()));
            })
            .with_on_page_load_handler(move |ev: PageLoadEvent, _url: String| {
                if let PageLoadEvent::Finished = ev {
                    let _ = p2.send_event(UserEvent::OverlayLoaded);
                }
            });
        let view = build_view(tune(builder), &window);
        #[cfg(windows)]
        {
            topmost::set_active(true);
        }
        self.overlay = Some(Win { view, window });
    }

    fn open_preview(&mut self, t: &Target) {
        if let Some(w) = &self.preview {
            w.window.set_focus();
            return;
        }
        let p = self.profile().clone();
        let url = norm(&prof_link(&p).unwrap_or_default());
        let script = scripts::preview(&p.hidden, &p.backgrounds, &p.fade, p.fade_secs, p.opacity, p.auto_bg, p.seven_tv);
        let window = base_window()
            .with_title("AllChats Preview")
            .with_decorations(true)
            .with_resizable(true)
            .with_inner_size(LogicalSize::new(1000.0, 760.0))
            .build(t)
            .expect("window");
        let p1 = self.proxy.clone();
        let p2 = self.proxy.clone();
        let builder = WebViewBuilder::with_web_context(&mut self.prof_ctx)
            .with_background_color((32, 32, 32, 255))
            .with_url(&url)
            .with_initialization_script(&script)
            .with_ipc_handler(move |req: Request<String>| {
                let _ = p1.send_event(UserEvent::Web(req.into_body()));
            })
            .with_on_page_load_handler(move |ev: PageLoadEvent, _url: String| {
                if let PageLoadEvent::Finished = ev {
                    let _ = p2.send_event(UserEvent::PreviewLoaded);
                }
            });
        let view = build_view(tune(builder), &window);
        self.preview = Some(Win { view, window });
        self.open_help(t);
    }

    fn open_help(&mut self, t: &Target) {
        #[cfg(windows)]
        {
            use tao::platform::windows::{WindowExtWindows, WindowBuilderExtWindows};
            if let Some(p) = &self.preview {
                let code = if self.lang.is_empty() { "en" } else { &self.lang };
                let map = lang::load(&self.store.root, code);
                let dir = if code == "fa" { "rtl" } else { "ltr" };
                let font = lang::font_stack(code);
                
                fn escape_html(s: &str) -> String {
                    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
                }
                
                let mut html = include_str!("help.html").to_string();
                html = html.replace("/*__FONTS__*/", &lang::font_css(Some(code)));
                html = html.replace("{{dir}}", dir);
                html = html.replace("{{font}}", font);
                for (k, v) in &map {
                    if k.starts_with("help.") {
                        if let Some(text) = v.as_str() {
                            html = html.replace(&format!("{{{{{}}}}}", k), &escape_html(text));
                        }
                    }
                }
                
                let hwnd = p.window.hwnd();
                let help_window = base_window()
                    .with_title("AllChats Help")
                    .with_decorations(false)
                    .with_resizable(false)
                    .with_focused(false)
                    .with_inner_size(LogicalSize::new(340.0, 275.0))
                    .with_owner_window(hwnd)
                    .with_skip_taskbar(true)
                    .build(t)
                    .expect("help window");
                let help_view = WebViewBuilder::new()
                    .with_html(&html)
                    .build(&help_window)
                    .expect("help webview");
                self.help = Some(Win { view: help_view, window: help_window });
                self.place_help();
            }
        }
    }

    fn place_help(&self) {
        #[cfg(windows)]
        if let (Some(p), Some(h)) = (&self.preview, &self.help) {
            let pos = p.window.outer_position().unwrap_or_default();
            let size = p.window.outer_size();
            let _ = h.window.set_outer_position(PhysicalPosition::new(pos.x + size.width as i32, pos.y));
        }
    }

    fn switch(&mut self, idx: usize, t: &Target) {
        let running = self.overlay.is_some();
        self.close_overlay();
        self.preview = None;
        self.help = None;
        self.persist();
        self.cur = idx;
        self.cands.clear();
        self.prof_ctx = make_ctx(&self.store, &self.profiles[idx].id);
        self.save_settings();
        if running {
            self.open_overlay(t);
        }
        self.push();
    }

    fn unique_name(&self) -> String {
        let mut n = self.profiles.len() + 1;
        loop {
            let name = format!("Profile {}", n);
            if !self
                .profiles
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(&name))
            {
                return name;
            }
            n += 1;
        }
    }

    fn set(&mut self, v: &Value) {
        let commit = v["commit"].as_bool().unwrap_or(true);
        let before = prof_link(self.profile());
        if let Some(k) = v["kind"].as_str() {
            if matches!(k, "custom" | "kick" | "twitch") {
                self.profile_mut().kind = k.to_string();
            }
        }
        if let Some(u) = v["url"].as_str() {
            self.profile_mut().url = u.trim().to_string();
        }
        if let Some(u) = v["kick"].as_str() {
            self.profile_mut().kick = clean_user(u);
        }
        if let Some(u) = v["twitch"].as_str() {
            self.profile_mut().twitch = clean_user(u);
        }
        let url_changed = prof_link(self.profile()) != before;
        if let Some(o) = v["opacity"].as_f64() {
            self.profile_mut().opacity = o.clamp(0.0, 1.0);
            self.apply_all();
        }
        if let Some(z) = v["zoom"].as_f64() {
            self.profile_mut().zoom = z.clamp(0.5, 2.0);
            self.apply_zoom();
        }
        if let Some(f) = v["fade_secs"].as_u64() {
            self.profile_mut().fade_secs = f.min(600) as u32;
            self.apply_all();
        }
        if let Some(c) = v["conn_check"].as_bool() {
            self.profile_mut().conn_check = c;
            self.net_js(c);
        }
        if let Some(a) = v["auto_bg"].as_bool() {
            self.profile_mut().auto_bg = a;
            self.apply_all();
        }
        if let Some(s) = v["seven_tv"].as_bool() {
            self.profile_mut().seven_tv = s;
            self.apply_all();
        }
        if let Some(b) = v["hide_border"].as_bool() {
            self.profile_mut().hide_border = b;
            if let Some(o) = &self.overlay {
                let _ = o.window.set_ignore_cursor_events(b);
                o.window.set_always_on_top(true);
                #[cfg(windows)]
                {
                    use tao::platform::windows::WindowExtWindows;
                    topmost::apply(o.window.hwnd() as isize, false);
                }
                let _ = o.view.evaluate_script(&format!("window.__acEdit&&window.__acEdit({})", !b));
            }
        }
        if url_changed {
            let url = norm(&prof_link(self.profile()).unwrap_or_default());
            if let Some(o) = &self.overlay {
                let _ = o.view.load_url(&url);
            }
            if let Some(p) = &self.preview {
                let _ = p.view.load_url(&url);
            }
        }
        if commit {
            self.persist();
        } else {
            self.schedule_flush();
        }
    }

    fn handle_ui(&mut self, msg: &str, t: &Target) -> bool {
        let v: Value = match serde_json::from_str(msg) {
            Ok(v) => v,
            Err(_) => return false,
        };
        if !matches!(
            v["cmd"].as_str().unwrap_or(""),
            "drag" | "min" | "fit" | "init" | "boot" | "scan" | "cands" | "preview" | "open_update" | "update_now" | "cancel_update" | "close"
        ) {
            self.schedule_flush();
        }
        match v["cmd"].as_str().unwrap_or("") {
            "drag" => {
                let _ = self.ctl.window.drag_window();
                return false;
            }
            "min" => {
                self.ctl.window.set_minimized(true);
                return false;
            }
            "close" => {
                return self.quit();
            }
            "select" => {
                if let Some(n) = v["name"].as_str() {
                    if let Some(i) = self.profiles.iter().position(|p| p.name == n) {
                        if i != self.cur {
                            self.switch(i, t);
                        }
                    }
                }
            }
            "new" => {
                let name = if let Some(n) = v["name"].as_str() {
                    let n = n.trim();
                    if !n.is_empty() {
                        n.chars().take(40).collect()
                    } else {
                        self.unique_name()
                    }
                } else {
                    self.unique_name()
                };
                let mut p = Profile::named(&name);
                if self.profiles.iter().any(|x| x.name.eq_ignore_ascii_case(&name)) {
                    let mut suffix = 2;
                    loop {
                        let candidate = format!("{} {}", name, suffix);
                        if !self.profiles.iter().any(|x| x.name.eq_ignore_ascii_case(&candidate)) {
                            p.name = candidate;
                            break;
                        }
                        suffix += 1;
                    }
                }
                self.store.save_profile(&p);
                self.profiles.push(p);
                let i = self.profiles.len() - 1;
                self.switch(i, t);
            }
            "add" => {
                let name = if let Some(n) = v["name"].as_str() {
                    let n = n.trim();
                    if !n.is_empty() {
                        n.chars().take(40).collect()
                    } else {
                        self.unique_name()
                    }
                } else {
                    self.unique_name()
                };
                let mut p = Profile::named(&name);
                if self.profiles.iter().any(|x| x.name.eq_ignore_ascii_case(&name)) {
                    let mut suffix = 2;
                    loop {
                        let candidate = format!("{} {}", name, suffix);
                        if !self.profiles.iter().any(|x| x.name.eq_ignore_ascii_case(&candidate)) {
                            p.name = candidate;
                            break;
                        }
                        suffix += 1;
                    }
                }
                self.store.save_profile(&p);
                self.profiles.push(p);
                let i = self.profiles.len() - 1;
                self.switch(i, t);
            }
            "dup" => {
                let mut p = self.profile().clone();
                p.id = store::new_id();
                let base: String = match v["name"].as_str().map(|n| n.trim()) {
                    Some(n) if !n.is_empty() => n.chars().take(40).collect(),
                    _ => format!("{} (copy)", p.name),
                };
                let mut name = base.clone();
                let mut suffix = 2;
                while self.profiles.iter().any(|x| x.name.eq_ignore_ascii_case(&name)) {
                    name = format!("{} {}", base, suffix);
                    suffix += 1;
                }
                p.name = name;
                self.store.save_profile(&p);
                self.profiles.push(p);
                let i = self.profiles.len() - 1;
                self.switch(i, t);
            }
            "delete" => {
                if self.profiles.len() > 1 {
                    let running = self.overlay.is_some();
                    self.close_overlay();
                    self.preview = None;
                    self.help = None;
                    self.cands.clear();
                    let old = self.profile().clone();
                    self.store.delete_profile(&old.name);
                    self.profiles.remove(self.cur);
                    self.cur = self.cur.saturating_sub(1);
                    self.prof_ctx = make_ctx(&self.store, &self.profiles[self.cur].id);
                    self.store.delete_data_later(&old.id);
                    self.save_settings();
                    if running {
                        self.open_overlay(t);
                    }
                }
            }
            "rename" => {
                if let Some(n) = v["name"].as_str() {
                    let n = n.trim().to_string();
                    let taken = self
                        .profiles
                        .iter()
                        .enumerate()
                        .any(|(i, p)| i != self.cur && p.name.eq_ignore_ascii_case(&n));
                    if !n.is_empty() && !taken {
                        let old = self.profile().name.clone();
                        self.store.delete_profile(&old);
                        self.profile_mut().name = n;
                        self.persist();
                    }
                }
            }
            "save" => self.persist(),
            "set" => self.set(&v),
            "scan" => self.scan(),
            "add_bg" => {
                let idx = v["index"].as_u64().unwrap_or(u64::MAX) as usize;
                let item = self.cand_view().get(idx).map(|i| (*i).clone());
                if let Some(item) = item {
                    self.profile_mut().backgrounds.push(item);
                    self.persist();
                    self.apply_all();
                }
            }
            "remove" => {
                let idx = v["index"].as_u64().unwrap_or(u64::MAX) as usize;
                {
                    let list = &mut self.profile_mut().backgrounds;
                    if idx < list.len() {
                        list.remove(idx);
                    }
                }
                self.persist();
                self.apply_all();
                self.scan();
            }
            "preview" => {
                if self.preview.is_some() {
                    self.preview = None;
                } else {
                    self.open_preview(t);
                }
            }
            "start" => self.open_overlay(t),
            "stop" => {
                self.close_overlay();
                self.overlay_want = false;
                self.save_settings();
            }
            "init" => {
                self.send_lang();
                self.push();
                let px = self.proxy.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(700));
                    let _ = px.send_event(UserEvent::Ui("{\"cmd\":\"boot\"}".to_string()));
                });
            }
            "boot" => self.boot(t),
            "lang" => {
                if let Some(code) = v["lang"].as_str() {
                    if lang::valid(code) {
                        self.lang = code.to_string();
                        self.save_settings();
                        self.send_lang();
                        if self.overlay.is_some() {
                            let texts = self.overlay_texts();
                            if let Some(o) = &self.overlay {
                                let _ = o.view.evaluate_script(&scripts::texts_js(&texts));
                            }
                        }
                        if self.help.is_some() {
                            self.help = None;
                            self.open_help(t);
                        }
                        self.push();
                    }
                }
            }
            "open_update" => {
                if let Some(info) = &self.update {
                    if info.url.starts_with("https://github.com/Dralder/AllChats/") {
                        let _ = open::that(&info.url);
                    }
                }
            }
            "update_now" => {
                if let Some(info) = &self.update {
                    if let Some(asset) = &info.asset {
                        if !self.upd_busy {
                            self.upd_busy = true;
                            self.upd_cancel.store(false, Ordering::Relaxed);
                            let upd_dir = self.store.root.join("update");
                            let _ = std::fs::create_dir_all(&upd_dir);
                            let url = asset.url.clone();
                            let size = asset.size;
                            let sha256 = asset.sha256.clone();
                            let cancel = Arc::clone(&self.upd_cancel);
                            let proxy = self.proxy.clone();
                            std::thread::spawn(move || {
                                let result = update::download(
                                    &url,
                                    &upd_dir,
                                    size,
                                    sha256.as_deref(),
                                    &cancel,
                                    |done, total| {
                                        let _ = proxy.send_event(UserEvent::UpdateProgress(done, total));
                                    },
                                );
                                match result {
                                    Ok(path) => {
                                        let _ = proxy.send_event(UserEvent::UpdateReady(path));
                                    }
                                    Err(e) => {
                                        let _ = proxy.send_event(UserEvent::UpdateFailed(e));
                                    }
                                }
                            });
                        }
                    }
                }
            }
            "fit" => {
                if self.mode == "window" {
                    if let Some(h) = v["h"].as_f64() {
                        let h = h.clamp(260.0, 700.0);
                        self.ctl.window.set_inner_size(LogicalSize::new(SIZE_WINDOW.0, h));
                    }
                }
            }
            "mode" => {
                let m = v["mode"].as_str().unwrap_or("");
                if (m == "overlay" || m == "window") && m != self.mode {
                    if m == "window" {
                        self.preview = None;
                        self.help = None;
                        self.cands.clear();
                    } else {
                        self.login = None;
                    }
                    self.mode = m.to_string();
                    self.ctl.window.set_inner_size(ctl_size(&self.mode));
                    self.save_settings();
                }
            }
            "wset" => {
                let kinds = all_kinds(&self.wcfg);
                let before: Vec<Option<String>> = kinds.iter().map(|k| link_for(&self.wcfg, k)).collect();
                if let Some(k) = v["kind"].as_str() {
                    if kind_ok(&self.wcfg, k) {
                        self.wcfg.kind = k.to_string();
                    }
                }
                if let Some(u) = v["surl"].as_str() {
                    if let Some(id) = self.wcfg.kind.strip_prefix("c:").map(|x| x.to_string()) {
                        if let Some(s) = self.wcfg.saved.iter_mut().find(|s| s.id == id) {
                            s.url = u.trim().to_string();
                        }
                    }
                }
                if let Some(u) = v["url"].as_str() {
                    self.wcfg.url = u.trim().to_string();
                }
                if let Some(u) = v["kick"].as_str() {
                    self.wcfg.kick = clean_user(u);
                }
                if let Some(u) = v["twitch"].as_str() {
                    self.wcfg.twitch = clean_user(u);
                }
                let cur = self.wcfg.kind.clone();
                if let Some(b) = v["topmost"].as_bool() {
                    let mut o = win_opt(&self.wcfg, &cur);
                    o.topmost = b;
                    self.wcfg.opts.insert(cur.clone(), o);
                    if let Some(w) = self.wins.get(&cur) {
                        w.window.set_always_on_top(b);
                    }
                }
                if let Some(z) = v["zoom"].as_f64() {
                    let mut o = win_opt(&self.wcfg, &cur);
                    o.zoom = z.clamp(0.5, 2.0);
                    self.wcfg.opts.insert(cur.clone(), o.clone());
                    if let Some(w) = self.wins.get(&cur) {
                        let _ = w.view.zoom(o.zoom);
                    }
                }
                if let Some(b) = v["seven"].as_bool() {
                    let mut o = win_opt(&self.wcfg, &cur);
                    o.seven = b;
                    self.wcfg.opts.insert(cur.clone(), o);
                    self.apply_win(&cur);
                    self.sync_bars();
                }
                for (i, k) in kinds.iter().enumerate() {
                    let after = link_for(&self.wcfg, k);
                    if after != before[i] {
                        if let (Some(w), Some(l)) = (self.wins.get(k.as_str()), &after) {
                            let _ = w.view.load_url(l);
                        }
                    }
                }
                self.sync_bars();
                self.save_settings();
            }
            "wadd" => {
                let name = clean_name(v["name"].as_str().unwrap_or(""));
                let url = v["url"].as_str().unwrap_or("").trim().to_string();
                let taken = self.wcfg.saved.iter().any(|s| s.name.to_lowercase() == name.to_lowercase());
                if !name.is_empty() && !taken {
                    let mut n = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    while self.wcfg.saved.iter().any(|s| s.id == format!("{:x}", n)) {
                        n += 1;
                    }
                    let id = format!("{:x}", n);
                    self.wcfg.saved.push(Saved { id: id.clone(), name, url });
                    self.wcfg.kind = format!("c:{}", id);
                    self.save_settings();
                }
            }
            "wdel" => {
                let k = self.wcfg.kind.clone();
                if let Some(id) = k.strip_prefix("c:") {
                    self.wins.remove(&k);
                    self.win_url.remove(&k);
                    self.wcfg.open.retain(|x| *x != k);
                    self.wcfg.geo.remove(&k);
                    self.wcfg.opts.remove(&k);
                    let id = id.to_string();
                    self.wcfg.saved.retain(|s| s.id != id);
                    self.wcfg.kind = "custom".to_string();
                    self.save_settings();
                }
            }
            "wstart" => {
                if self.mode == "window" {
                    let k = self.wcfg.kind.clone();
                    self.open_win(&k, t);
                }
            }
            "wstop" => {
                let k = self.wcfg.kind.clone();
                self.wins.remove(&k);
                self.wcfg.open.retain(|x| *x != k);
                self.save_settings();
            }
            "wlogin" => {
                if self.mode == "window" {
                    self.open_login(t);
                }
            }
            "wreopen" => {
                if !self.wins.is_empty() {
                    self.reopen_wins(t);
                }
            }
            "cancel_update" => {
                self.upd_cancel.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
        self.push();
        false
    }

    fn handle_web(&mut self, msg: &str) {
        let v: Value = match serde_json::from_str(msg) {
            Ok(v) => v,
            Err(_) => return,
        };
        match v["t"].as_str().unwrap_or("") {
            "lists" => {
                let hidden: Vec<Item> =
                    serde_json::from_value(v["hidden"].clone()).unwrap_or_default();
                self.profile_mut().hidden = hidden;
                if v.get("fade").is_some() {
                    let fade: Vec<Item> = serde_json::from_value(v["fade"].clone()).unwrap_or_default();
                    self.profile_mut().fade = fade;
                }
                self.persist();
                self.apply_overlay();
            }
            "cands" => {
                self.cands = serde_json::from_value(v["list"].clone()).unwrap_or_default();
                self.push();
            }
            "7tv_id" => self.seven_id(&v),
            "7tv_img" => {
                let target = v["k"].as_str().unwrap_or("overlay").to_string();
                if let Some(u) = v["url"].as_str() {
                    self.want_img(target, u.to_string());
                }
            }
            "wpickjs" => {
                let k = v["k"].as_str().unwrap_or("");
                if let (Some(p), Some(n)) = (self.wins.get(k), v["n"].as_str()) {
                    let name = serde_json::to_string(n).unwrap_or_else(|_| "\"\"".into());
                    let js = format!("window.__ac7Insert?window.__ac7Insert({}):window.ipc.postMessage(JSON.stringify({{t:'7tv_ins',r:'nojs'}}))", name);
                    let _ = p.view.evaluate_script(&js);
                }
            }
            "7tv_send" => {
                let k = v["k"].as_str().unwrap_or("").to_string();
                let text = v["text"].as_str().unwrap_or("").to_string();
                let click = v["act"].as_str() == Some("click");
                if let Some(p) = self.wins.get(&k) {
                    let _ = p.view.focus();
                    let px = self.proxy.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(80));
                        if !typer::type_text(&text) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                        if click {
                            let msg = serde_json::json!({ "k": k, "t": "7tv_after" }).to_string();
                            let _ = px.send_event(UserEvent::Web(msg));
                        } else {
                            typer::press_enter();
                        }
                    });
                }
            }
            "7tv_key" => {
                let k = v["k"].as_str().unwrap_or("");
                if let Some(p) = self.wins.get(k) {
                    let _ = p.view.focus();
                    std::thread::spawn(|| {
                        std::thread::sleep(Duration::from_millis(20));
                        typer::press_backspace();
                    });
                }
            }
            "7tv_after" => {
                let k = v["k"].as_str().unwrap_or("");
                if let Some(p) = self.wins.get(k) {
                    let _ = p.view.evaluate_script("window.__ac7After&&window.__ac7After()");
                }
            }
            "7tv_ins" => {
                let k = v["k"].as_str().unwrap_or("");
                if let Some(pk) = self.wins.get(k).and_then(|p| p.picker.as_ref()) {
                    let r = serde_json::to_string(v["r"].as_str().unwrap_or("")).unwrap_or_else(|_| "\"\"".into());
                    let _ = pk.evaluate_script(&format!("window.__status&&window.__status({})", r));
                }
            }
            "wresize" => {
                let k = v["k"].as_str().unwrap_or("");
                if let (Some(p), Some(dir)) = (self.wins.get(k), v["dir"].as_str()) {
                    if let Some(d) = resize_dir(dir) {
                        let _ = p.window.drag_resize_window(d);
                    }
                }
            }
            "wpick" => {
                let k = v["k"].as_str().unwrap_or("").to_string();
                match v["b"]["op"].as_str().unwrap_or("") {
                    "close" => self.toggle_picker(&k),
                    "img" => {
                        if let Some(u) = v["b"]["url"].as_str() {
                            self.want_img(format!("p:{}", k), u.to_string());
                        }
                    }
                    "ready" => {
                        if self.wins.get(&k).map(|p| p.picker_on).unwrap_or(false) {
                            self.push_picker(&k);
                            if let Some(pk) = self.wins.get(&k).and_then(|p| p.picker.as_ref()) {
                                let _ = pk.evaluate_script("window.__show&&window.__show()");
                            }
                        }
                    }
                    "pick" => {
                        if let (Some(p), Some(n)) = (self.wins.get(&k), v["b"]["n"].as_str()) {
                            let _ = p.view.focus();
                            let px = self.proxy.clone();
                            let k2 = k.clone();
                            let n2 = n.to_string();
                            let nj = serde_json::to_string(n).unwrap_or_else(|_| "\"\"".into());
                            let _ = p.view.evaluate_script_with_callback(
                                &format!("window.__ac7Prepare?window.__ac7Prepare({}):'nojs'", nj),
                                move |r| {
                                    let _ = px.send_event(UserEvent::Prep(k2.clone(), n2.clone(), r));
                                },
                            );
                        }
                    }
                    _ => {}
                }
            }
            "wbar" => {
                let k = v["k"].as_str().unwrap_or("").to_string();
                let op = v["b"]["op"].as_str().unwrap_or("");
                match op {
                    "close" => self.close_win(&k),
                    "seven" => self.toggle_picker(&k),
                    _ => {
                        if let Some(p) = self.wins.get(&k) {
                            match op {
                                "drag" => {
                                    let _ = p.window.drag_window();
                                }
                                "min" => p.window.set_minimized(true),
                                "max" => p.window.set_maximized(!p.window.is_maximized()),
                                "resize" => {
                                    if let Some(d) = v["b"]["dir"].as_str().and_then(resize_dir) {
                                        let _ = p.window.drag_resize_window(d);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            "win" => {
                let kind = v["kind"].as_str().unwrap_or("");
                let op = v["op"].as_str().unwrap_or("");
                if op == "close" {
                    if kind == "overlay" {
                        self.close_overlay();
                        self.overlay_want = false;
                        self.save_settings();
                        self.push();
                    }
                    return;
                }
                if kind == "overlay" {
                    if let Some(w) = &self.overlay {
                        if op == "drag" {
                            let _ = w.window.drag_window();
                        } else if op == "resize" {
                            if let Some(dir) = v["dir"].as_str() {
                                use tao::window::ResizeDirection;
                                let direction = match dir {
                                    "W" => ResizeDirection::West,
                                    "E" => ResizeDirection::East,
                                    "N" => ResizeDirection::North,
                                    "NW" => ResizeDirection::NorthWest,
                                    "NE" => ResizeDirection::NorthEast,
                                    "S" => ResizeDirection::South,
                                    "SW" => ResizeDirection::SouthWest,
                                    "SE" => ResizeDirection::SouthEast,
                                    _ => return,
                                };
                                let _ = w.window.drag_resize_window(direction);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn seven_id(&mut self, v: &Value) {
        let target = v["k"].as_str().unwrap_or("overlay").to_string();
        let on = if target == "overlay" { self.profile().seven_tv } else { win_opt(&self.wcfg, &target).seven };
        if !on {
            return;
        }
        let platform = v["platform"].as_str();
        let ident = match (&v["id"], &v["login"]) {
            (Value::String(s), _) => s.clone(),
            (Value::Number(n), _) => n.to_string(),
            (_, Value::String(s)) => s.clone(),
            _ => return,
        };
        let key = format!("{}:{}", platform.unwrap_or("kick"), ident);
        self.seven_want.insert(target.clone(), key.clone());
        let cached = self.emotes.get(&key).map(|(j, at)| (j.clone(), at.elapsed() < Duration::from_secs(600)));
        if let Some((json, fresh)) = cached {
            self.send_seven_to(&target, &scripts::emotes(&json, false));
            if fresh {
                return;
            }
        }
        if !self.fetching.insert(key.clone()) {
            return;
        }
        let proxy = self.proxy.clone();
        let plat = platform.unwrap_or("kick").to_string();
        std::thread::spawn(move || {
            let (json, picker, ok) = seven::fetch(&plat, &ident);
            let _ = proxy.send_event(UserEvent::Emotes(key, json, picker, ok));
        });
    }

    fn geo_save(&mut self) {
        self.schedule_flush();
    }

    fn on_window(&mut self, id: WindowId, ev: WindowEvent) -> bool {
        if id == self.ctl.window.id() {
            match ev {
                WindowEvent::CloseRequested => {
                    return self.quit();
                }
                WindowEvent::Moved(pos) => {
                    if pos.x > -10000 && pos.y > -10000 {
                        self.ctl_x = Some(pos.x);
                        self.ctl_y = Some(pos.y);
                    }
                    self.sgeo_save();
                }
                WindowEvent::Resized(size) => {
                    if size.width > 0 && size.height > 0 {
                        let _ = self.ctl.view.set_bounds(wry::Rect {
                            position: wry::dpi::PhysicalPosition::new(0, 0).into(),
                            size: wry::dpi::PhysicalSize::new(size.width, size.height).into(),
                        });
                    }
                }
                _ => {}
            }
            return false;
        }
        if self.login.as_ref().map(|w| w.window.id()) == Some(id) {
            if let WindowEvent::CloseRequested = ev {
                self.login = None;
                if !self.wins.is_empty() {
                    let _ = self.proxy.send_event(UserEvent::Ui("{\"cmd\":\"wreopen\"}".to_string()));
                }
            }
            return false;
        }
        if let Some(kind) = self.win_kind_of(id) {
            match ev {
                WindowEvent::CloseRequested => {
                    self.close_win(&kind);
                }
                WindowEvent::Moved(pos) => {
                    if pos.x > -10000 && pos.y > -10000 {
                        let mut g = win_geo(&self.wcfg, &kind);
                        g.x = Some(pos.x);
                        g.y = Some(pos.y);
                        self.wcfg.geo.insert(kind, g);
                    }
                    self.sgeo_save();
                }
                WindowEvent::Resized(size) => {
                    if size.width > 0 && size.height > 0 {
                        let mut g = win_geo(&self.wcfg, &kind);
                        g.w = size.width;
                        g.h = size.height;
                        self.wcfg.geo.insert(kind.clone(), g);
                        if let Some(p) = self.wins.get(&kind) {
                            layout_pane(p);
                        }
                    }
                    self.sgeo_save();
                }
                _ => {}
            }
            return false;
        }
        if self.preview.as_ref().map(|w| w.window.id()) == Some(id) {
            match ev {
                WindowEvent::CloseRequested => {
                    self.preview = None;
                    self.help = None;
                    self.push();
                }
                WindowEvent::Moved(_) => {
                    self.place_help();
                }
                WindowEvent::Resized(size) => {
                    if size.width > 0 && size.height > 0 {
                        if let Some(w) = &self.preview {
                            let _ = w.view.set_bounds(wry::Rect {
                                position: wry::dpi::PhysicalPosition::new(0, 0).into(),
                                size: wry::dpi::PhysicalSize::new(size.width, size.height).into(),
                            });
                        }
                    }
                    self.place_help();
                }
                _ => {}
            }
            return false;
        }
        if self.help.as_ref().map(|w| w.window.id()) == Some(id) {
            return false;
        }
        if self.overlay.as_ref().map(|w| w.window.id()) == Some(id) {
            match ev {
                WindowEvent::CloseRequested => {
                    self.close_overlay();
                    self.overlay_want = false;
                    self.save_settings();
                    self.push();
                }
                WindowEvent::Moved(pos) => {
                    if pos.x > -10000 && pos.y > -10000 {
                        let p = self.profile_mut();
                        p.x = pos.x;
                        p.y = pos.y;
                    }
                    self.geo_save();
                }
                WindowEvent::Resized(size) => {
                    if size.width > 0 && size.height > 0 {
                        let p = self.profile_mut();
                        p.w = size.width;
                        p.h = size.height;
                        if let Some(o) = &self.overlay {
                            let _ = o.view.set_bounds(wry::Rect {
                                position: wry::dpi::PhysicalPosition::new(0, 0).into(),
                                size: wry::dpi::PhysicalSize::new(size.width, size.height).into(),
                            });
                        }
                    }
                    self.geo_save();
                }
                _ => {}
            }
        }
        false
    }
}

fn main() {
    lower_priority();
    let event_loop: EventLoop<UserEvent> = EventLoopBuilder::with_user_event().build();
    let proxy = event_loop.create_proxy();
    topmost::init(proxy.clone());
    let mut app = App::new(&event_loop, proxy.clone());
    {
        let px = proxy.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(4000));
            let _ = px.send_event(UserEvent::Ui("{\"cmd\":\"boot\"}".to_string()));
        });
    }

    std::thread::spawn(move || {
        if let Some(info) = update::check(env!("CARGO_PKG_VERSION")) {
            let _ = proxy.send_event(UserEvent::Update(info));
        }
    });

    event_loop.run(move |event, target, flow| {
        *flow = ControlFlow::Wait;
        let exit = match event {
            Event::UserEvent(UserEvent::Ui(m)) => app.handle_ui(&m, target),
            Event::UserEvent(UserEvent::Web(m)) => {
                app.handle_web(&m);
                false
            }
            Event::UserEvent(UserEvent::PreviewLoaded) => {
                app.apply_preview();
                false
            }
            Event::UserEvent(UserEvent::OverlayLoaded) => {
                app.apply_zoom();
                app.apply_overlay();
                app.net_js(app.profile().conn_check);
                false
            }
            Event::UserEvent(UserEvent::Prep(kind, name, res)) => {
                let val: String = serde_json::from_str(&res).unwrap_or_else(|_| res.clone());
                if val.contains("noinput") {
                    if let Some(pk) = app.wins.get(&kind).and_then(|p| p.picker.as_ref()) {
                        let _ = pk.evaluate_script("window.__status&&window.__status('noinput')");
                    }
                } else {
                    if let Some(p) = app.wins.get(&kind) {
                        let _ = p.view.focus();
                    }
                    let typed = match val.strip_prefix("ok|") {
                        Some(t) if !t.is_empty() => t.to_string(),
                        _ => format!("{} ", name),
                    };
                    let now = Instant::now();
                    let fresh = !app.typeq.contains_key(&kind);
                    let e = app.typeq.entry(kind.clone()).or_insert((String::new(), Vec::new(), now, now));
                    e.0.push_str(&typed);
                    e.1.push(name);
                    e.3 = now;
                    if fresh {
                        let px = app.proxy.clone();
                        let k = kind.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(60));
                            let _ = px.send_event(UserEvent::TypeTick(k));
                        });
                    }
                }
                false
            }
            Event::UserEvent(UserEvent::TypeTick(kind)) => {
                let ready = app
                    .typeq
                    .get(&kind)
                    .map(|e| e.3.elapsed() >= Duration::from_millis(140) || e.2.elapsed() >= Duration::from_millis(450));
                if ready == Some(true) {
                    if let Some((text, names, _, _)) = app.typeq.remove(&kind) {
                        if let Some(p) = app.wins.get(&kind) {
                            let _ = p.view.focus();
                        }
                        let px = app.proxy.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(60));
                            if !typer::type_text(&text) {
                                for n in names {
                                    let msg = serde_json::json!({ "k": kind, "t": "wpickjs", "n": n }).to_string();
                                    let _ = px.send_event(UserEvent::Web(msg));
                                }
                            }
                        });
                    }
                } else if ready == Some(false) {
                    let px = app.proxy.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(60));
                        let _ = px.send_event(UserEvent::TypeTick(kind));
                    });
                }
                false
            }
            Event::UserEvent(UserEvent::Img(url, data)) => {
                if !data.is_empty() {
                    if app.imgs.len() > 800 {
                        app.imgs.clear();
                    }
                    app.imgs.insert(url.clone(), data.clone());
                }
                if let Some(list) = app.img_wait.remove(&url) {
                    for t in list {
                        app.reply_img(&t, &url, &data);
                    }
                }
                false
            }
            Event::UserEvent(UserEvent::Flush) => {
                app.flush_pending = false;
                app.persist();
                false
            }
            Event::UserEvent(UserEvent::WinLoaded(kind, url)) => {
                app.win_url.insert(kind.clone(), url);
                app.apply_win(&kind);
                app.sync_bars();
                false
            }
            Event::UserEvent(UserEvent::Emotes(key, json, picker, ok)) => {
                app.fetching.remove(&key);
                let has = json != "{}";
                if has {
                    app.pickers.insert(key.clone(), picker);
                }
                let targets: Vec<String> = app
                    .seven_want
                    .iter()
                    .filter(|(_, k)| **k == key)
                    .map(|(t, _)| t.clone())
                    .collect();
                for t in &targets {
                    if has {
                        app.send_seven_to(t, &scripts::emotes(&json, !ok));
                    } else {
                        app.send_seven_to(t, scripts::seven_retry());
                    }
                }
                if ok && has {
                    app.emotes.insert(key, (json, Instant::now()));
                }
                for t in &targets {
                    if app.wins.get(t).map(|p| p.picker_on).unwrap_or(false) {
                        app.push_picker(t);
                    }
                }
                false
            }
            Event::UserEvent(UserEvent::Update(info)) => {
                app.update = Some(info);
                app.push();
                false
            }
            Event::UserEvent(UserEvent::UpdateProgress(done, total)) => {
                let js = format!(
                    "window.setProgress({{\"state\":\"downloading\",\"done\":{},\"total\":{}}})",
                    done, total
                );
                let _ = app.ctl.view.evaluate_script(&js);
                false
            }
            Event::UserEvent(UserEvent::UpdateReady(zip_path)) => {
                #[cfg(debug_assertions)]
                {
                    let _ = std::fs::remove_file(&zip_path);
                    app.upd_busy = false;
                    let _ = app.ctl.view.evaluate_script("window.setProgress({\"state\":\"debug_done\"})");
                    false
                }
                #[cfg(all(not(debug_assertions), windows))]
                {
                    app.persist();
                    app.close_overlay();
                    let script_path = app.store.root.join("update").join("apply.cmd");
                    let app_path = std::env::current_exe().unwrap_or_default();
                    let app_name = app_path.file_name().and_then(|n| n.to_str()).unwrap_or("AllChats.exe");
                    let extract_dir = app.store.root.join("update").join("extract");
                    let log_path = app.store.root.join("update").join("update.log");
                    
                    let escape = |s: &str| s.replace('%', "%%");
                    let script = format!(
r#"@echo off
setlocal DisableDelayedExpansion
set "APP={}"
set "NAME={}"
set "ZIP={}"
set "DIR={}"
set "LOG={}"
set "NEW="
if exist "%DIR%" rmdir /s /q "%DIR%" >nul 2>&1
mkdir "%DIR%" >nul 2>&1
tar -xf "%ZIP%" -C "%DIR%" >nul 2>&1
if errorlevel 1 powershell -NoProfile -NonInteractive -Command "Expand-Archive -LiteralPath '%ZIP%' -DestinationPath '%DIR%' -Force" >nul 2>&1
for /r "%DIR%" %%F in (*.exe) do if not defined NEW set "NEW=%%F"
if not defined NEW goto fail
move /y "%NEW%" "%DIR%\%NAME%" >nul 2>&1
set "NEW=%DIR%\%NAME%"
if not exist "%NEW%" goto fail
set /a TRIES=0
:del
set /a TRIES+=1
if exist "%APP%" del /f /q "%APP%" >nul 2>&1
if not exist "%APP%" goto put
if %TRIES% GEQ 60 goto fail
powershell -NoProfile -NonInteractive -Command "Start-Sleep -Milliseconds 700" >nul 2>&1
goto del
:put
move /y "%NEW%" "%APP%" >nul 2>&1
if not exist "%APP%" goto fail
del /f /q "%ZIP%" >nul 2>&1
rmdir /s /q "%DIR%" >nul 2>&1
for %%I in ("%APP%") do cd /d "%%~dpI"
start "" "%APP%"
(goto) 2>nul & del /f /q "%~f0"
:fail
echo update failed %date% %time% >> "%LOG%"
rmdir /s /q "%DIR%" >nul 2>&1
if exist "%APP%" for %%I in ("%APP%") do cd /d "%%~dpI"
if exist "%APP%" start "" "%APP%"
(goto) 2>nul & del /f /q "%~f0"
"#,
                        escape(&app_path.display().to_string()),
                        escape(app_name),
                        escape(&zip_path.display().to_string()),
                        escape(&extract_dir.display().to_string()),
                        escape(&log_path.display().to_string())
                    );
                    
                    if let Err(_) = std::fs::write(&script_path, script) {
                        app.upd_busy = false;
                        let _ = app.ctl.view.evaluate_script("window.setProgress({\"state\":\"error\",\"msg\":\"Cannot write script\"})");
                        false
                    } else {
                        let _ = app.ctl.view.evaluate_script("window.setProgress({\"state\":\"installing\"})");
                        
                        use std::os::windows::process::CommandExt;
                        use std::process::{Command, Stdio};
                        
                        let result = Command::new("cmd.exe")
                            .creation_flags(0x08000000 | 0x00000200)
                            .raw_arg(format!("/D /C \"\"{}\"\"", script_path.display()))
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .spawn();
                        
                        match result {
                            Ok(_) => true,
                            Err(_) => {
                                app.upd_busy = false;
                                let _ = app.ctl.view.evaluate_script("window.setProgress({\"state\":\"error\",\"msg\":\"Cannot start installer\"})");
                                false
                            }
                        }
                    }
                }
                #[cfg(all(not(debug_assertions), not(windows)))]
                {
                    app.upd_busy = false;
                    let _ = app.ctl.view.evaluate_script("window.setProgress({\"state\":\"error\",\"msg\":\"Auto update is Windows only\"})");
                    false
                }
            }
            Event::UserEvent(UserEvent::UpdateFailed(msg)) => {
                app.upd_busy = false;
                if msg == "Cancelled" {
                    let upd_dir = app.store.root.join("update");
                    let _ = std::fs::remove_file(upd_dir.join("AllChats-update.zip.part"));
                    let _ = app.ctl.view.evaluate_script("window.setProgress({\"state\":\"ask\"})");
                } else {
                    let js = format!("window.setProgress({{\"state\":\"error\",\"msg\":{}}})", serde_json::to_string(&msg).unwrap_or_else(|_| "\"Error\"".to_string()));
                    let _ = app.ctl.view.evaluate_script(&js);
                }
                false
            }
            Event::UserEvent(UserEvent::Raise(from_fg)) => {
                app.raise_overlay(from_fg);
                false
            }
            Event::WindowEvent {
                window_id, event, ..
            } => app.on_window(window_id, event),
            _ => false,
        };
        if exit {
            *flow = ControlFlow::Exit;
        }
    });
}
