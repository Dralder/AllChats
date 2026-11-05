#[cfg(windows)]
use {
    std::sync::atomic::{AtomicBool, AtomicU64, Ordering},
    std::sync::Mutex,
    tao::event_loop::EventLoopProxy,
    windows_sys::Win32::{
        Foundation::HWND,
        UI::{
            Accessibility::{SetWinEventHook, HWINEVENTHOOK},
            WindowsAndMessaging::{
                GetClassNameW, GetForegroundWindow, GetWindow, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos,
                GWL_EXSTYLE, GW_HWNDPREV, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
                EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WS_EX_NOACTIVATE, WS_EX_TOPMOST,
            },
        },
    },
};

#[cfg(windows)]
static PROXY: Mutex<Option<EventLoopProxy<crate::UserEvent>>> = Mutex::new(None);
#[cfg(windows)]
static ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static LAST_CB: AtomicU64 = AtomicU64::new(0);

#[cfg(windows)]
unsafe extern "system" fn winevent_proc(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    let last = LAST_CB.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 100 {
        return;
    }
    LAST_CB.store(now, Ordering::Relaxed);
    if let Some(p) = PROXY.lock().unwrap().as_ref() {
        let _ = p.send_event(crate::UserEvent::Raise(true));
    }
}

#[cfg(windows)]
pub fn init(proxy: EventLoopProxy<crate::UserEvent>) {
    *PROXY.lock().unwrap() = Some(proxy.clone());
    unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(winevent_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
    }
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(2000));
            if ACTIVE.load(Ordering::Relaxed) {
                if let Some(p) = PROXY.lock().unwrap().as_ref() {
                    let _ = p.send_event(crate::UserEvent::Raise(false));
                }
            }
        }
    });
}

#[cfg(windows)]
pub fn set_active(on: bool) {
    ACTIVE.store(on, Ordering::Relaxed);
}

#[cfg(windows)]
pub fn apply(hwnd: isize, from_foreground_event: bool) {
    unsafe {
        let hwnd = hwnd as HWND;
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if ex_style & (WS_EX_NOACTIVATE as isize) == 0 {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_NOACTIVATE as isize);
        }
        let fg = GetForegroundWindow();
        if fg.is_null() || fg == hwnd {
            return;
        }
        let fg_ex = GetWindowLongPtrW(fg, GWL_EXSTYLE);
        if fg_ex & (WS_EX_TOPMOST as isize) == 0 {
            return;
        }
        let prev = GetWindow(hwnd, GW_HWNDPREV);
        let already_top = prev.is_null();
        if already_top {
            return;
        }
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER);
        if cfg!(debug_assertions) && (from_foreground_event || !already_top) {
            let mut buf = [0u16; 256];
            let len = GetClassNameW(fg, buf.as_mut_ptr(), buf.len() as i32);
            let class = String::from_utf16_lossy(&buf[..len as usize]);
            eprintln!("[AC-TOP] fg class {}, fg topmost true, overlay already on top {}, raised true", class, already_top);
        }
    }
}

#[cfg(not(windows))]
pub fn init(_proxy: tao::event_loop::EventLoopProxy<crate::UserEvent>) {}

#[cfg(not(windows))]
pub fn set_active(_on: bool) {}

#[cfg(not(windows))]
pub fn apply(_hwnd: isize, _from_foreground_event: bool) {}
