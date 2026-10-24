//! Whether Alt is held, for the overlay's details: read, never caught (the
//! game still gets the key).
//!
//! Windows does not let a program read the keyboard while a program running
//! as administrator has the focus, unless it runs as administrator too
//! (UI Privilege Isolation): Alt then reads as up. `blocked` says when that
//! is the case, so the overlay can say to click instead. Death Recap runs as
//! administrator when Flav-Toolkit does (the hub starts it), and then reads
//! Alt in an elevated game too.

/// Alt held right now. Always false where this is not built (not Windows).
pub fn held() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
        // The high bit: the key is down now.
        (unsafe { GetAsyncKeyState(VK_MENU.0 as i32) } as u16 & 0x8000) != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Alt cannot be read now: the window in front runs as administrator and
/// Death Recap does not.
pub fn blocked() -> bool {
    #[cfg(windows)]
    {
        !imp::elevated(None) && imp::foreground_pid().is_some_and(|pid| imp::elevated(Some(pid)))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Give the game the focus back (after Death Recap's settings close).
pub fn focus_game() {
    #[cfg(windows)]
    imp::focus_game();
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::{BOOL, CloseHandle, HANDLE, HWND, LPARAM};
    use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow,
    };
    use windows::core::PWSTR;

    pub fn foreground_pid() -> Option<u32> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        (pid != 0 && pid != unsafe { GetCurrentProcessId() }).then_some(pid)
    }

    /// Whether a process (this one for `None`) runs as administrator. A
    /// process whose token cannot be read from here runs above this one.
    pub fn elevated(pid: Option<u32>) -> bool {
        unsafe {
            let process = match pid {
                None => GetCurrentProcess(),
                Some(pid) => match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                    Ok(h) => h,
                    // Closed even to a query: it runs higher.
                    Err(_) => return true,
                },
            };
            let mut token = HANDLE::default();
            let opened = OpenProcessToken(process, TOKEN_QUERY, &mut token).is_ok();
            let mut elevation = TOKEN_ELEVATION::default();
            let mut len = 0u32;
            let answer = opened
                && GetTokenInformation(
                    token,
                    TokenElevation,
                    Some(&mut elevation as *mut _ as *mut _),
                    std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                    &mut len,
                )
                .is_ok();
            if opened {
                let _ = CloseHandle(token);
            }
            if pid.is_some() {
                let _ = CloseHandle(process);
            }
            match (opened, answer) {
                (true, true) => elevation.TokenIsElevated != 0,
                // Its token is closed to this process: it runs higher.
                (false, _) => pid.is_some(),
                (true, false) => false,
            }
        }
    }

    fn is_game(hwnd: HWND) -> bool {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 {
            return false;
        }
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
            let mut buf = [0u16; 512];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
            let _ = CloseHandle(h);
            ok && String::from_utf16_lossy(&buf[..len as usize])
                .rsplit(['\\', '/'])
                .next()
                .is_some_and(|n| n.eq_ignore_ascii_case("AION2.exe"))
        }
    }

    pub fn focus_game() {
        unsafe extern "system" fn find(hwnd: HWND, found: LPARAM) -> BOOL {
            if unsafe { IsWindowVisible(hwnd) }.as_bool() && is_game(hwnd) {
                unsafe { *(found.0 as *mut HWND) = hwnd };
                return BOOL(0);
            }
            BOOL(1)
        }
        let mut game = HWND::default();
        unsafe {
            let _ = EnumWindows(Some(find), LPARAM(&mut game as *mut HWND as isize));
            if !game.0.is_null() {
                let _ = SetForegroundWindow(game);
            }
        }
    }
}
