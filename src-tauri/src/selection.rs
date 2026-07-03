//! I/O real da captura/substituicao: enigo (input) + arboard (clipboard).
//! A logica pura vive em `ember_core::selection`.

use ember_core::selection::SelectionIo;
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};

/// Sentinela unico escrito no clipboard para detetar "nada selecionado".
pub const SENTINEL: &str = "\u{200b}__ember_capture_sentinel__\u{200b}";

/// Snapshot de um clipboard de imagem (RGBA), para restaurar depois do refine. Sem isto, um
/// ciclo de captura destruia a imagem no clipboard (a captura e text-only) e nunca a repunha.
pub struct ClipImage {
    width: usize,
    height: usize,
    bytes: Vec<u8>,
}

pub struct RealIo {
    clip: arboard::Clipboard,
    enigo: Enigo,
    /// Terminal em foco: usar Ctrl+Shift+C/V (o Ctrl+C envia SIGINT nos terminais).
    terminal: bool,
}

impl RealIo {
    pub fn new(terminal: bool) -> Result<Self, String> {
        let clip = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
        Ok(Self {
            clip,
            enigo,
            terminal,
        })
    }

    /// Snapshot do clipboard quando e uma imagem (`None` para texto ou vazio). Tirado ANTES
    /// de a captura escrever o sentinela, para a imagem poder ser reposta no fim.
    pub fn snapshot_image(&mut self) -> Option<ClipImage> {
        self.clip.get_image().ok().map(|img| ClipImage {
            width: img.width,
            height: img.height,
            bytes: img.bytes.into_owned(),
        })
    }

    /// Repoe uma imagem no clipboard (best-effort).
    pub fn restore_image(&mut self, img: &ClipImage) {
        let _ = self.clip.set_image(arboard::ImageData {
            width: img.width,
            height: img.height,
            bytes: std::borrow::Cow::Borrowed(&img.bytes),
        });
    }

    /// `true` se o clipboard tem conteudo que nao conseguimos preservar (ficheiros do
    /// Explorer, RTF, formatos proprietarios): nem texto nem imagem. Nesse caso o caller
    /// aborta em vez de destruir o clipboard do utilizador.
    pub fn has_unpreservable_content(&mut self) -> bool {
        has_unpreservable_clipboard()
    }

    /// Simula um atalho de clipboard: Ctrl(+Shift)+`key`.
    fn combo(&mut self, key: char) {
        let _ = self.enigo.key(Key::Control, Press);
        if self.terminal {
            let _ = self.enigo.key(Key::Shift, Press);
        }
        let _ = self.enigo.key(Key::Unicode(key), Click);
        if self.terminal {
            let _ = self.enigo.key(Key::Shift, Release);
        }
        let _ = self.enigo.key(Key::Control, Release);
    }
}

/// Le o estado fisico dos modificadores agora (bit alto de `GetAsyncKeyState` = premido).
/// Usado pela politica de neutralizacao para esperar a libertacao natural antes de forcar.
#[cfg(windows)]
fn physical_modifiers() -> ember_core::ModifierState {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    let down = |vk: i32| (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0;
    ember_core::ModifierState {
        ctrl: down(VK_CONTROL.0 as i32),
        shift: down(VK_SHIFT.0 as i32),
        alt: down(VK_MENU.0 as i32),
        win: down(VK_LWIN.0 as i32) || down(VK_RWIN.0 as i32),
    }
}

#[cfg(not(windows))]
fn physical_modifiers() -> ember_core::ModifierState {
    ember_core::ModifierState::default()
}

/// Ha conteudo no clipboard mas nenhum formato que saibamos preservar (texto ou bitmap)?
/// arboard nao enumera formatos, por isso vamos ao Win32. Formatos standard preservaveis:
/// CF_TEXT (1), CF_UNICODETEXT (13), CF_BITMAP (2), CF_DIB (8), CF_DIBV5 (17).
#[cfg(windows)]
fn has_unpreservable_clipboard() -> bool {
    use windows::Win32::System::DataExchange::{CountClipboardFormats, IsClipboardFormatAvailable};
    if unsafe { CountClipboardFormats() } == 0 {
        return false; // vazio: nada a perder
    }
    const PRESERVABLE: [u32; 5] = [1, 13, 2, 8, 17];
    let any_preservable = PRESERVABLE
        .iter()
        .any(|&f| unsafe { IsClipboardFormatAvailable(f).is_ok() });
    !any_preservable
}

#[cfg(not(windows))]
fn has_unpreservable_clipboard() -> bool {
    false
}

impl SelectionIo for RealIo {
    fn clip_get(&mut self) -> Option<String> {
        self.clip.get_text().ok()
    }
    fn clip_set(&mut self, s: &str) {
        let _ = self.clip.set_text(s.to_string());
    }
    fn modifiers_held(&mut self) -> ember_core::ModifierState {
        physical_modifiers()
    }
    fn release_modifiers(&mut self) {
        let _ = self.enigo.key(Key::Shift, Release);
        let _ = self.enigo.key(Key::Control, Release);
        let _ = self.enigo.key(Key::Alt, Release);
        let _ = self.enigo.key(Key::Meta, Release);
    }
    fn send_copy(&mut self) {
        self.combo('c');
    }
    fn send_paste(&mut self) {
        self.combo('v');
    }
    fn sleep_ms(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}
