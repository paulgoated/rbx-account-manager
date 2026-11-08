//! System tray icon with a small menu (Show, Anti-AFK, Stop all Roblox, Quit).
//! Runs its own Win32 message loop on a background thread.
use crate::monitor::{TrayCmd, AFK_ON};
use crate::win::wide;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Main app window (set once it exists).
pub static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
/// Set when the user picks Quit, so closing isn't turned into "hide to tray".
pub static QUIT: AtomicBool = AtomicBool::new(false);
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static CMD: OnceLock<Mutex<Sender<TrayCmd>>> = OnceLock::new();

const WM_TRAY: u32 = WM_APP + 1;

fn send(cmd: TrayCmd) {
    if let Some(tx) = CMD.get() {
        let _ = tx.lock().unwrap().send(cmd);
    }
}

pub fn show_main() {
    let h = MAIN_HWND.load(Ordering::Relaxed);
    if h != 0 {
        crate::win::show_window(h, true);
        crate::win::focus_window(h);
    }
}

fn quit() {
    QUIT.store(true, Ordering::Relaxed);
    let h = MAIN_HWND.load(Ordering::Relaxed);
    if h != 0 {
        crate::win::show_window(h, true);
        crate::win::close_window(h);
    }
}

unsafe fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid
}

unsafe fn show_menu(hwnd: HWND) {
    let m = CreatePopupMenu();
    let items: [(u32, usize, &str); 6] = [
        (MF_STRING, 1, "Show RBX Account Manager"),
        (MF_SEPARATOR, 0, ""),
        (MF_STRING | if AFK_ON.load(Ordering::Relaxed) { MF_CHECKED } else { 0 }, 2, "Anti-AFK"),
        (MF_STRING, 3, "Stop all Roblox"),
        (MF_SEPARATOR, 0, ""),
        (MF_STRING, 4, "Quit"),
    ];
    for (flags, id, text) in items {
        let w = wide(text);
        AppendMenuW(m, flags, id, if text.is_empty() { null() } else { w.as_ptr() });
    }
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, null());
    DestroyMenu(m);
    match cmd {
        1 => show_main(),
        2 => send(TrayCmd::ToggleAfk),
        3 => send(TrayCmd::StopAll),
        4 => quit(),
        _ => {}
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            let ev = (lp as u32) & 0xFFFF;
            if ev == WM_LBUTTONUP || ev == WM_LBUTTONDBLCLK {
                show_main();
            } else if ev == WM_RBUTTONUP || ev == WM_CONTEXTMENU {
                show_menu(hwnd);
            }
            0
        }
        WM_DESTROY => {
            let nid = icon_data(hwnd);
            Shell_NotifyIconW(NIM_DELETE, &nid);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub fn start(cmd: Sender<TrayCmd>) {
    let _ = CMD.set(Mutex::new(cmd));
    std::thread::spawn(|| unsafe {
        let hinst = GetModuleHandleW(null());
        let class = wide("RbxAccountManagerTray");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
            hIconSm: null_mut(),
        };
        RegisterClassExW(&wc);
        let title = wide("RBX Account Manager tray");
        let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW, class.as_ptr(), title.as_ptr(), WS_POPUP, 0, 0, 0, 0, null_mut(), null_mut(), hinst, null());
        if hwnd.is_null() {
            return;
        }
        TRAY_HWND.store(hwnd as isize, Ordering::Relaxed);

        // icon embedded in the .exe (resource id 1), or the default app icon for dev builds
        let mut icon = LoadImageW(hinst, 1 as _, IMAGE_ICON, 16, 16, 0) as HICON;
        if icon.is_null() {
            icon = LoadIconW(null_mut(), IDI_APPLICATION);
        }
        let mut nid = icon_data(hwnd);
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = icon;
        let tip = wide("RBX Account Manager");
        for (i, c) in tip.iter().take(nid.szTip.len() - 1).enumerate() {
            nid.szTip[i] = *c;
        }
        Shell_NotifyIconW(NIM_ADD, &nid);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

/// Remove the tray icon (call on exit).
pub fn stop() {
    let h = TRAY_HWND.load(Ordering::Relaxed);
    if h != 0 {
        unsafe { PostMessageW(h as HWND, WM_CLOSE, 0, 0) };
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
