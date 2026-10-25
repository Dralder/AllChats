#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod scripts;
mod seven;
mod store;
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

use store::{Item, Profile, Settings, Store};

const UI_HTML: &str = include_str!("ui.html");
const ICON_BYTES: &[u8] = include_bytes!("../allchats.ico");

#[cfg(windows)]
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion,OptimizationHints,MediaRouter,Translate --enable-features=RemoveRedirectionBitmap --disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows --disable-smooth-scrolling --disable-background-networking --disable-breakpad --no-pings";

type Target = EventLoopWindowTarget<UserEvent>;

enum UserEvent {
    Ui(String),
    Web(String),
    PreviewLoaded,
    OverlayLoaded,
    Emotes(String, String),
    Update(update::Info),
    UpdateProgress(u64, u64),
    UpdateReady(PathBuf),
    UpdateFailed(String),
}

struct Win {
    view: WebView,
    window: Window,
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

fn make_ctx(store: &Store, id: &str) -> WebContext {
    WebContext::new(Some(store.data_dir(id)))
}

struct App {
    store: Store,
    profiles: Vec<Profile>,
    cur: usize,
    cands: Vec<Item>,
    emotes: Option<(String, String)>,
    fetching: Option<String>,
    proxy: EventLoopProxy<UserEvent>,
    ctl: Win,
    preview: Option<Win>,
    overlay: Option<Win>,
    last_geo: Instant,
    prof_ctx: WebContext,
    update: Option<update::Info>,
    upd_busy: bool,
    upd_cancel: Arc<AtomicBool>,
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
        let ids: Vec<String> = profiles.iter().map(|p| p.id.clone()).collect();
        store.cleanup(&ids);

        let upd_dir = store.root.join("update");
        let _ = std::fs::remove_file(upd_dir.join("AllChats-update.zip"));
        let _ = std::fs::remove_file(upd_dir.join("AllChats-update.zip.part"));
        let _ = std::fs::remove_dir_all(upd_dir.join("extract"));

        let prof_ctx = make_ctx(&store, &profiles[cur].id);

        let window = base_window()
            .with_title("AllChats")
            .with_decorations(false)
            .with_resizable(false)
            .with_inner_size(LogicalSize::new(400.0, 750.0))
            .build(target)
            .expect("window");

        let p = proxy.clone();
        let mut ctl_ctx = WebContext::new(Some(store.ui_dir()));
        let builder = WebViewBuilder::with_web_context(&mut ctl_ctx)
            .with_html(UI_HTML)
            .with_background_color((32, 32, 32, 255))
            .with_ipc_handler(move |req: Request<String>| {
                let _ = p.send_event(UserEvent::Ui(req.into_body()));
            });
        let view = build_view(builder, &window);

        Self {
            store,
            profiles,
            cur,
            cands: Vec::new(),
            emotes: None,
            fetching: None,
            proxy,
            ctl: Win { view, window },
            preview: None,
            overlay: None,
            last_geo: Instant::now(),
            prof_ctx,
            update: None,
            upd_busy: false,
            upd_cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn start_if_url(&mut self, t: &Target) {
        if !self.profile().url.trim().is_empty() {
            self.open_overlay(t);
        }
    }

    fn profile(&self) -> &Profile {
        &self.profiles[self.cur]
    }

    fn profile_mut(&mut self) -> &mut Profile {
        &mut self.profiles[self.cur]
    }

    fn persist(&self) {
        self.store.save_profile(self.profile());
        self.store.save_settings(&Settings {
            last_profile: self.profile().name.clone(),
        });
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
            "opacity": p.opacity,
            "hide_border": p.hide_border,
            "auto_bg": p.auto_bg,
            "seven_tv": p.seven_tv,
            "zoom": p.zoom,
            "conn_check": p.conn_check,
            "bgs": names(&bgs),
            "cands": names(&self.cand_view()),
            "overlay": self.overlay.is_some(),
            "preview": self.preview.is_some(),
            "update": update_val,
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
        scripts::apply(&p.hidden, &p.backgrounds, p.opacity, p.auto_bg, p.seven_tv, p.hide_border)
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

    fn send_emotes(&self, json: &str) {
        let js = scripts::emotes(json);
        if let Some(o) = &self.overlay {
            let _ = o.view.evaluate_script(&js);
        }
        if let Some(w) = &self.preview {
            let _ = w.view.evaluate_script(&js);
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

    fn close_overlay(&mut self) {
        if self.overlay.is_some() {
            self.persist();
            self.overlay = None;
        }
    }

    fn open_overlay(&mut self, t: &Target) {
        if self.overlay.is_some() {
            return;
        }
        let p = self.profile().clone();
        let url = norm(&p.url);
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
        if p.hide_border {
            let _ = window.set_ignore_cursor_events(true);
        }
        let script = scripts::overlay(&p.hidden, &p.backgrounds, p.opacity, p.auto_bg, p.seven_tv, p.hide_border);
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
        self.overlay = Some(Win { view, window });
    }

    fn open_preview(&mut self, t: &Target) {
        if let Some(w) = &self.preview {
            w.window.set_focus();
            return;
        }
        let p = self.profile().clone();
        let url = norm(&p.url);
        let script = scripts::preview(&p.hidden, &p.backgrounds, p.opacity, p.auto_bg, p.seven_tv);
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
    }

    fn switch(&mut self, idx: usize, t: &Target) {
        let running = self.overlay.is_some();
        self.close_overlay();
        self.preview = None;
        self.persist();
        self.cur = idx;
        self.cands.clear();
        self.prof_ctx = make_ctx(&self.store, &self.profiles[idx].id);
        self.store.save_settings(&Settings {
            last_profile: self.profile().name.clone(),
        });
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
        let mut url_changed = false;
        if let Some(u) = v["url"].as_str() {
            let u = u.trim().to_string();
            if u != self.profile().url {
                self.profile_mut().url = u;
                url_changed = true;
            }
        }
        if let Some(o) = v["opacity"].as_f64() {
            self.profile_mut().opacity = o.clamp(0.0, 1.0);
            self.apply_all();
        }
        if let Some(z) = v["zoom"].as_f64() {
            self.profile_mut().zoom = z.clamp(0.5, 2.0);
            self.apply_zoom();
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
                let _ = o.view.evaluate_script(&format!("window.__acEdit&&window.__acEdit({})", !b));
            }
        }
        if url_changed {
            let url = norm(&self.profile().url);
            if let Some(o) = &self.overlay {
                let _ = o.view.load_url(&url);
            }
            if let Some(p) = &self.preview {
                let _ = p.view.load_url(&url);
            }
        }
        if commit {
            self.persist();
        }
    }

    fn handle_ui(&mut self, msg: &str, t: &Target) -> bool {
        let v: Value = match serde_json::from_str(msg) {
            Ok(v) => v,
            Err(_) => return false,
        };
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
                self.close_overlay();
                self.persist();
                return true;
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
                let name = self.unique_name();
                let p = Profile::named(&name);
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
                    self.cands.clear();
                    let old = self.profile().clone();
                    self.store.delete_profile(&old.name);
                    self.profiles.remove(self.cur);
                    self.cur = self.cur.saturating_sub(1);
                    self.prof_ctx = make_ctx(&self.store, &self.profiles[self.cur].id);
                    self.store.delete_data_later(&old.id);
                    self.store.save_settings(&Settings {
                        last_profile: self.profile().name.clone(),
                    });
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
            "stop" => self.close_overlay(),
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
                self.persist();
                self.apply_overlay();
            }
            "cands" => {
                self.cands = serde_json::from_value(v["list"].clone()).unwrap_or_default();
                self.push();
            }
            "7tv_id" => self.seven_id(&v),
            "win" => {
                let kind = v["kind"].as_str().unwrap_or("");
                let op = v["op"].as_str().unwrap_or("");
                if op == "close" {
                    if kind == "overlay" {
                        self.close_overlay();
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
        if !self.profile().seven_tv {
            return;
        }
        let platform = v["platform"].as_str();
        let ident = match (&v["id"], &v["login"]) {
            (Value::String(s), _) => s.clone(),
            (Value::Number(n), _) => n.to_string(),
            (_, Value::String(s)) => s.clone(),
            _ => return,
        };
        let key = if let Some(p) = platform {
            format!("{}:{}", p, ident)
        } else {
            format!("kick:{}", ident)
        };
        if let Some((cached_key, json)) = &self.emotes {
            if *cached_key == key {
                let json = json.clone();
                self.send_emotes(&json);
                return;
            }
        }
        if self.fetching.as_deref() == Some(key.as_str()) {
            return;
        }
        self.fetching = Some(key.clone());
        let proxy = self.proxy.clone();
        let plat = platform.unwrap_or("kick").to_string();
        std::thread::spawn(move || {
            let json = seven::fetch(&plat, &ident);
            let _ = proxy.send_event(UserEvent::Emotes(key, json));
        });
    }

    fn geo_save(&mut self) {
        if self.last_geo.elapsed() > Duration::from_secs(1) {
            self.last_geo = Instant::now();
            self.store.save_profile(self.profile());
        }
    }

    fn on_window(&mut self, id: WindowId, ev: WindowEvent) -> bool {
        if id == self.ctl.window.id() {
            if let WindowEvent::CloseRequested = ev {
                self.close_overlay();
                self.persist();
                return true;
            }
            return false;
        }
        if self.preview.as_ref().map(|w| w.window.id()) == Some(id) {
            match ev {
                WindowEvent::CloseRequested => {
                    self.preview = None;
                    self.push();
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
                }
                _ => {}
            }
            return false;
        }
        if self.overlay.as_ref().map(|w| w.window.id()) == Some(id) {
            match ev {
                WindowEvent::CloseRequested => {
                    self.close_overlay();
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
    let mut app = App::new(&event_loop, proxy.clone());
    app.start_if_url(&event_loop);

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
            Event::UserEvent(UserEvent::Emotes(key, json)) => {
                app.fetching = None;
                app.send_emotes(&json);
                app.emotes = Some((key, json));
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
