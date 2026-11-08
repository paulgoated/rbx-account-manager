//! Small Windows API wrappers: DPAPI, mutexes, opening URLs, message boxes, dark title bar.
use std::ffi::c_void;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, SW_SHOWNORMAL};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
}

unsafe fn take_blob(b: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let v = std::slice::from_raw_parts(b.pbData, b.cbData as usize).to_vec();
    LocalFree(b.pbData as _);
    v
}

/// Encrypt with DPAPI (scope: the current Windows user).
pub fn protect(data: &[u8], entropy: &[u8]) -> Result<Vec<u8>, String> {
    let input = blob(data);
    let ent = blob(entropy);
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
    let ok = unsafe { CryptProtectData(&input, null(), &ent, null(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out) };
    if ok == 0 {
        return Err(format!("Encryption failed (code {})", unsafe { GetLastError() }));
    }
    Ok(unsafe { take_blob(&out) })
}

pub fn unprotect(data: &[u8], entropy: &[u8]) -> Result<Vec<u8>, String> {
    let input = blob(data);
    let ent = blob(entropy);
    let ent_ptr: *const CRYPT_INTEGER_BLOB = if entropy.is_empty() { null() } else { &ent };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
    let ok = unsafe { CryptUnprotectData(&input, null_mut(), ent_ptr, null(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out) };
    if ok == 0 {
        return Err("The data file can only be opened by the Windows user who created it, on the same PC.".into());
    }
    Ok(unsafe { take_blob(&out) })
}

/// A named mutex owned by this process. Released automatically on drop.
pub struct NamedMutex(HANDLE);

impl NamedMutex {
    /// `None` if the mutex already exists (held by another process).
    pub fn acquire(name: &str) -> Option<NamedMutex> {
        let w = wide(name);
        unsafe {
            let h = CreateMutexW(null(), 1, w.as_ptr());
            if h.is_null() {
                return None;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(h);
                return None;
            }
            Some(NamedMutex(h))
        }
    }
}

impl Drop for NamedMutex {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

pub fn open_url(url: &str) -> bool {
    let op = wide("open");
    let file = wide(url);
    let r = unsafe { ShellExecuteW(null_mut(), op.as_ptr(), file.as_ptr(), null(), null(), SW_SHOWNORMAL) };
    (r as isize) > 32
}

pub fn message_box(text: &str, error: bool) {
    let t = wide(text);
    let c = wide("RBX Account Manager");
    let flags = MB_OK | if error { MB_ICONERROR } else { MB_ICONINFORMATION };
    unsafe { MessageBoxW(null_mut(), t.as_ptr(), c.as_ptr(), flags) };
}

/// PIDs of every running RobloxPlayerBeta.exe process.
pub fn roblox_pids() -> Vec<u32> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            return out;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e);
        while ok != 0 {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            let name = String::from_utf16_lossy(&e.szExeFile[..len]);
            if name.eq_ignore_ascii_case("RobloxPlayerBeta.exe") {
                out.push(e.th32ProcessID);
            }
            ok = Process32NextW(snap, &mut e);
        }
        CloseHandle(snap);
    }
    out
}

/// Hand unused memory pages back to Windows (working set trim).
/// Task Manager's "Memory" figure drops right away; pages are paged back in when needed.
pub fn trim_memory() {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

/// Force-close processes by PID. Returns how many were closed.
pub fn kill_pids(pids: &[u32]) -> usize {
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    let mut n = 0;
    for &pid in pids {
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if !h.is_null() {
                if TerminateProcess(h, 0) != 0 {
                    n += 1;
                }
                CloseHandle(h);
            }
        }
    }
    n
}

// ------------------------------------------------------------------ Roblox windows
/// A top-level Roblox Player window.
#[derive(Clone, Copy, Debug)]
pub struct RbxWindow {
    pub hwnd: isize,
    pub pid: u32,
    pub minimized: bool,
}

/// All visible top-level windows that belong to a RobloxPlayerBeta.exe process (one per process).
pub fn roblox_windows() -> Vec<RbxWindow> {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, GW_OWNER,
    };
    let pids: std::collections::HashSet<u32> = roblox_pids().into_iter().collect();
    if pids.is_empty() {
        return Vec::new();
    }
    struct Ctx {
        pids: std::collections::HashSet<u32>,
        out: Vec<RbxWindow>,
    }
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let ctx = &mut *(lp as *mut Ctx);
        if IsWindowVisible(hwnd) == 0 || !GetWindow(hwnd, GW_OWNER).is_null() || GetWindowTextLengthW(hwnd) == 0 {
            return 1;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if ctx.pids.contains(&pid) && !ctx.out.iter().any(|w| w.pid == pid) {
            ctx.out.push(RbxWindow { hwnd: hwnd as isize, pid, minimized: IsIconic(hwnd) != 0 });
        }
        1
    }
    let mut ctx = Ctx { pids, out: Vec::new() };
    unsafe { EnumWindows(Some(cb), &mut ctx as *mut Ctx as LPARAM) };
    ctx.out
}

/// Bring a window to the front (restoring it if minimized).
pub fn focus_window(h: isize) {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    let hwnd = h as windows_sys::Win32::Foundation::HWND;
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        let fg = GetForegroundWindow();
        let fg_tid = GetWindowThreadProcessId(fg, null_mut());
        let me = GetCurrentThreadId();
        let attach = fg_tid != 0 && fg_tid != me;
        if attach {
            AttachThreadInput(me, fg_tid, 1);
        }
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        if attach {
            AttachThreadInput(me, fg_tid, 0);
        }
    }
}

pub fn show_window(h: isize, show: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOW};
    unsafe { ShowWindow(h as windows_sys::Win32::Foundation::HWND, if show { SW_SHOW } else { SW_HIDE }) };
}

pub fn is_window_visible(h: isize) -> bool {
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(h as windows_sys::Win32::Foundation::HWND) != 0 }
}

pub fn foreground_window() -> isize {
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() as isize }
}

/// Seconds since the last keyboard/mouse input anywhere on the system.
pub fn user_idle_secs() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        if GetLastInputInfo(&mut li) == 0 {
            return 0;
        }
        (GetTickCount().wrapping_sub(li.dwTime) / 1000) as u64
    }
}

/// Press and release one virtual key (goes to the foreground window).
pub fn tap_key(vk: u16) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP};
    let mk = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    };
    let inputs = [mk(0), mk(KEYEVENTF_KEYUP)];
    unsafe { SendInput(2, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
}

/// Anti-AFK nudge: focus the client, tap F13 (a key no game uses), then put things back.
pub fn nudge_window(h: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, ShowWindow, SW_MINIMIZE};
    let hwnd = h as windows_sys::Win32::Foundation::HWND;
    let prev = foreground_window();
    let was_min = unsafe { IsIconic(hwnd) != 0 };
    focus_window(h);
    std::thread::sleep(std::time::Duration::from_millis(if was_min { 400 } else { 150 }));
    tap_key(0x7C); // VK_F13
    std::thread::sleep(std::time::Duration::from_millis(120));
    if was_min {
        unsafe { ShowWindow(hwnd, SW_MINIMIZE) };
    }
    if prev != 0 && prev != h {
        focus_window(prev);
    }
}

// ------------------------------------------------------------------ DWM live thumbnails
/// Register a live DWM thumbnail of `src` drawn inside `dest`. Returns the thumbnail handle.
pub fn thumb_register(dest: isize, src: isize) -> Option<isize> {
    use windows_sys::Win32::Graphics::Dwm::DwmRegisterThumbnail;
    let mut id = 0isize;
    let hr = unsafe { DwmRegisterThumbnail(dest as _, src as _, &mut id) };
    if hr == 0 && id != 0 { Some(id) } else { None }
}

pub fn thumb_unregister(t: isize) {
    unsafe { windows_sys::Win32::Graphics::Dwm::DwmUnregisterThumbnail(t) };
}

/// Size of the source window in pixels.
pub fn thumb_source_size(t: isize) -> Option<(i32, i32)> {
    let mut s = windows_sys::Win32::Foundation::SIZE { cx: 0, cy: 0 };
    let hr = unsafe { windows_sys::Win32::Graphics::Dwm::DwmQueryThumbnailSourceSize(t, &mut s) };
    if hr == 0 && s.cx > 0 && s.cy > 0 { Some((s.cx, s.cy)) } else { None }
}

/// Place the thumbnail at `rect` (left, top, right, bottom in client pixels) and show/hide it.
pub fn thumb_update(t: isize, rect: [i32; 4], visible: bool) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmUpdateThumbnailProperties, DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_SOURCECLIENTAREAONLY,
        DWM_TNP_VISIBLE,
    };
    let props = DWM_THUMBNAIL_PROPERTIES {
        dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY,
        rcDestination: RECT { left: rect[0], top: rect[1], right: rect[2], bottom: rect[3] },
        rcSource: RECT { left: 0, top: 0, right: 0, bottom: 0 },
        opacity: 255,
        fVisible: visible as i32,
        fSourceClientAreaOnly: 1,
    };
    unsafe { DwmUpdateThumbnailProperties(t, &props) };
}

/// Ask a window to close (like clicking its X).
pub fn close_window(h: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    unsafe { PostMessageW(h as _, WM_CLOSE, 0, 0) };
}

/// True if a named mutex currently exists (owned by any process).
pub fn mutex_exists(name: &str) -> bool {
    use windows_sys::Win32::System::Threading::OpenMutexW;
    let w = wide(name);
    unsafe {
        let h = OpenMutexW(0x0010_0000 /* SYNCHRONIZE */, 0, w.as_ptr());
        if h.is_null() {
            return false;
        }
        CloseHandle(h);
        true
    }
}

/// Light or dark window title bar. Attribute 20 = DWMWA_USE_IMMERSIVE_DARK_MODE (Win10 20H1+), 19 = older builds.
pub fn set_title_bar_dark(hwnd: *mut c_void, dark: bool) {
    let v: i32 = dark as i32;
    unsafe {
        if DwmSetWindowAttribute(hwnd as _, 20, &v as *const i32 as _, 4) != 0 {
            DwmSetWindowAttribute(hwnd as _, 19, &v as *const i32 as _, 4);
        }
    }
}
