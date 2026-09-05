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
    "conemu64.exe",
    "conemu.exe",
    "putty.exe",
    "warp.exe",
];

/// `true` se o caminho do exe em foco e um terminal conhecido. Puro e testavel em qualquer
/// plataforma (o `foreground_exe` que le o SO fica isolado por tras do cfg(windows)).
pub fn is_terminal_exe(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let base = lower.rsplit(['\\', '/']).next().unwrap_or(lower.as_str());
    TERMINALS.contains(&base)
}

#[cfg(windows)]
pub fn is_terminal_foreground() -> bool {
    foreground_exe()
        .map(|p| is_terminal_exe(&p))
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn is_terminal_foreground() -> bool {
    false
}

/// O processo Ember corre elevado (Administrador)?
///
/// Isto NAO e um detalhe de curiosidade: no Windows, uma app nao-elevada nunca recebe um atalho
/// global enquanto a janela em foco pertencer a um processo ELEVADO (User Interface Privilege
/// Isolation). Do lado do utilizador isso e indistinguivel de "a hotkey esta partida": carrega,
/// nao acontece nada, e nao ha erro nenhum em lado nenhum. Por isso vai para o diagnostico.
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// O fallback de select-all (Ctrl/Cmd+A quando nao havia seleccao) so e seguro onde sabemos
/// distinguir um terminal do resto. Num terminal, um Cmd+A seleciona o SCROLLBACK INTEIRO e o
/// paste que se lhe seguisse seria destrutivo.
///
/// `is_terminal_foreground` so tem implementacao no Windows: no macOS devolve sempre `false`,
/// ou seja, um Terminal.app seria tratado como uma app normal e apanhava o Cmd+A. Por isso o
/// fallback fica desligado la ate a deteccao existir (precisa do bundle id da app em primeiro
/// plano, via NSWorkspace). Ate la o macOS mantem o comportamento antigo: sem seleccao, o
/// refine diz "Select text first" em vez de arriscar. Ver `docs/macos-parity.md`.
pub const fn select_all_is_safe_here() -> bool {
    cfg!(windows)
}

/// So para diagnostico: o caminho do exe em foco (para o log perceber porque um terminal nao
/// foi ou foi classificado como tal). Nao usado no fluxo normal.
#[cfg(windows)]
pub fn debug_foreground_exe() -> Option<String> {
    foreground_exe()
}

#[cfg(not(windows))]
pub fn debug_foreground_exe() -> Option<String> {
    None
}

/// Titulo da janela em foco. Sinal (seguro, sem ler memoria de outro processo) para a deteccao
/// de contexto de projeto: muitos IDEs/terminais mostram o caminho do projeto no titulo. macOS
/// virá com o AXTitle (a permissao de Acessibilidade e ja precisa para o paste). Windows aqui.
#[cfg(windows)]
pub fn foreground_title() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return None;
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let copied = GetWindowTextW(hwnd, &mut buf);
        if copied <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..copied as usize]))
    }
}

#[cfg(not(windows))]
pub fn foreground_title() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::is_terminal_exe;

    #[test]
    fn matches_terminals_by_basename_case_insensitively() {
        assert!(is_terminal_exe(r"C:\Windows\System32\cmd.exe"));
        assert!(is_terminal_exe(
            r"C:\Program Files\WindowsApps\WindowsTerminal.exe"
        ));
        assert!(is_terminal_exe("PowerShell.EXE"));
        assert!(is_terminal_exe("/usr/bin/pwsh.exe"));
    }

    #[test]
    fn rejects_non_terminals_and_substring_traps() {
        assert!(!is_terminal_exe(r"C:\Windows\explorer.exe"));
        assert!(!is_terminal_exe(r"C:\code\Code.exe"));
        // Nao deve casar por substring: "notcmd.exe" nao e "cmd.exe".
        assert!(!is_terminal_exe(r"C:\x\notcmd.exe"));
        assert!(!is_terminal_exe(""));
    }
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

/// Identidade da janela em foco: (HWND, pid). E o alvo de um refine, capturado quando o atalho
/// dispara e verificado outra vez mesmo antes de colar.
///
/// Sem esta verificacao, uma chamada de dezenas de segundos seguida de dez de preview podia
/// acabar a colar o texto de uma app dentro de outra: entre a captura e o paste ninguem
/// perguntava se o alvo ainda era o mesmo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetSnapshot {
    pub window: (u64, u32),
    pub control: u64,
}

#[cfg(windows)]
pub fn foreground_target() -> Option<TargetSnapshot> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut gui = windows::Win32::UI::WindowsAndMessaging::GUITHREADINFO {
            cbSize: std::mem::size_of::<windows::Win32::UI::WindowsAndMessaging::GUITHREADINFO>()
                as u32,
            ..Default::default()
        };
        windows::Win32::UI::WindowsAndMessaging::GetGUIThreadInfo(0, &mut gui).ok()?;
        if pid == 0 || gui.hwndFocus.0.is_null() {
            return None;
        }
        Some(TargetSnapshot {
            window: (hwnd.0 as u64, pid),
            control: gui.hwndFocus.0 as u64,
        })
    }
}

#[cfg(not(windows))]
pub fn foreground_target() -> Option<TargetSnapshot> {
    None
}

/// A janela em foco ainda e a do inicio do ciclo? A regra e pura e testada
/// (`ember_core::selection::paste_allowed`); aqui so se le o estado do SO.
pub fn same_target(target: Option<TargetSnapshot>) -> bool {
    match (target, foreground_target()) {
        (Some(target), Some(current)) => {
            target.control != 0
                && target.control == current.control
                && ember_core::selection::paste_allowed(Some(target.window), Some(current.window))
        }
        _ => false,
    }
}
