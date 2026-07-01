//! Deteta se a app em foco e um terminal. Os terminais usam Ctrl+Shift+C/V (e o
//! Ctrl+C envia SIGINT), por isso a captura/substituicao tem de usar essas teclas.

/// Apps tratados como terminal (basename do exe, lowercase). Code.exe fica de fora de
/// proposito: o editor do VS Code copia com Ctrl+C, e o terminal integrado tambem
/// copia com Ctrl+C quando ha seleccao no Windows.
const TERMINALS: &[&str] = &[
    "windowsterminal.exe",
    "openconsole.exe",
    "conhost.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "wezterm-gui.exe",
    "wezterm.exe",
    "alacritty.exe",
    "mintty.exe",
    "kitty.exe",
    "hyper.exe",
    "tabby.exe",
];

#[cfg(windows)]
pub fn is_terminal_foreground() -> bool {
    foreground_exe()
        .map(|p| {
            let lower = p.to_ascii_lowercase();
            let base = lower.rsplit(['\\', '/']).next().unwrap_or(lower.as_str());
            TERMINALS.contains(&base)
        })
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn is_terminal_foreground() -> bool {
    false
}

#[cfg(windows)]
fn foreground_exe() -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let res = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        res.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}
