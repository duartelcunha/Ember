//! I/O real da captura/substituicao: enigo (input) + arboard (clipboard).
//! A logica pura vive em `ember_core::selection`.

use ember_core::selection::SelectionIo;
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};

/// Sentinela unico escrito no clipboard para detetar "nada selecionado".
pub const SENTINEL: &str = "\u{200b}__ember_capture_sentinel__\u{200b}";

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

impl SelectionIo for RealIo {
    fn clip_get(&mut self) -> Option<String> {
        self.clip.get_text().ok()
    }
    fn clip_set(&mut self, s: &str) {
        let _ = self.clip.set_text(s.to_string());
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
