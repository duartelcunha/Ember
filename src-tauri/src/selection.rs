//! I/O real da captura/substituicao: enigo (input) + arboard (clipboard).
//! A logica pura vive em `ember_core::selection`.

use ember_core::selection::SelectionIo;
use enigo::{
    Direction::{Press, Release},
    Enigo, Key, Keyboard, Settings,
};

/// Sentinela unico escrito no clipboard para detetar "nada selecionado".
pub const SENTINEL: &str = "\u{200b}__ember_capture_sentinel__\u{200b}";

/// Pausa entre eventos de tecla dentro de um atalho simulado (ver `RealIo::combo`). Curta o
/// bastante para ser impercetivel, longa o bastante para apps de input assincrono (Windows
/// Terminal) registarem os modificadores antes da tecla.
const KEY_SETTLE_MS: u64 = 12;

/// Snapshot de um clipboard de imagem (RGBA), para restaurar depois do refine. Sem isto, um
/// ciclo de captura destruia a imagem no clipboard (a captura e text-only) e nunca a repunha.
#[cfg(not(windows))]
pub struct ClipImage {
    width: usize,
    height: usize,
    bytes: Vec<u8>,
}

#[cfg(windows)]
pub type ClipImage = crate::clipboard_snapshot::Snapshot;

/// Modificador do atalho de clipboard, por SO. macOS copia/cola com Cmd (que o enigo chama
/// `Key::Meta`); Windows/Linux com Ctrl. `enigo` e `arboard` sao cross-platform, por isso so a
/// escolha da tecla e que muda entre plataformas.
#[cfg(target_os = "macos")]
fn clipboard_modifier() -> Key {
    Key::Meta
}
#[cfg(not(target_os = "macos"))]
fn clipboard_modifier() -> Key {
    Key::Control
}

/// A tecla A/C/V de um atalho de clipboard, por plataforma. No Windows usa o VIRTUAL KEY fisico
/// (`Key::Other(VK)`): VK_A=0x41, VK_C=0x43, VK_V=0x56. Um `Key::Unicode` injetaria um caractere puro que o
/// Windows Terminal nao liga aos modificadores (o atalho de copia nao dispara). Nas outras
/// plataformas o Unicode funciona com Cmd/Ctrl.
#[cfg(windows)]
fn clip_key(c: char) -> Key {
    match c {
        'a' | 'A' => Key::Other(0x41),
        'c' | 'C' => Key::Other(0x43),
        'v' | 'V' => Key::Other(0x56),
        other => Key::Unicode(other),
    }
}
#[cfg(not(windows))]
fn clip_key(c: char) -> Key {
    Key::Unicode(c)
}

pub struct RealIo {
    clip: arboard::Clipboard,
    enigo: Enigo,
    /// Terminal em foco: no Windows usa Ctrl+Shift+C/V (o Ctrl+C envia SIGINT nos terminais). No
    /// macOS o copy/paste e sempre Cmd+C/V (mesmo em terminais), por isso isto fica sempre falso
    /// la (a deteccao de terminal so corre no Windows).
    terminal: bool,
    input_failed: bool,
}

impl RealIo {
    pub fn new(terminal: bool) -> Result<Self, String> {
        let clip = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
        Ok(Self {
            clip,
            enigo,
            terminal,
            input_failed: false,
        })
    }

    /// Snapshot do clipboard quando e uma imagem (`None` para texto ou vazio). Tirado ANTES
    /// de a captura escrever o sentinela, para a imagem poder ser reposta no fim.
    pub fn input_succeeded(&self) -> bool {
        !self.input_failed
    }

    pub fn snapshot_image(&mut self) -> Option<ClipImage> {
        #[cfg(windows)]
        {
            crate::clipboard_snapshot::Snapshot::read().ok()
        }
        #[cfg(not(windows))]
        {
            self.clip.get_image().ok().map(|img| ClipImage {
                width: img.width,
                height: img.height,
                bytes: img.bytes.into_owned(),
            })
        }
    }

    pub fn restore_image(&mut self, img: &ClipImage) {
        #[cfg(windows)]
        if let Some(revision) = self.clip_revision() {
            let _ = img.restore_if_owned(revision);
        }
        #[cfg(not(windows))]
        {
            let _ = self.clip.set_image(arboard::ImageData {
                width: img.width,
                height: img.height,
                bytes: std::borrow::Cow::Borrowed(&img.bytes),
            });
        }
    }

    pub fn has_unpreservable_content(&mut self) -> bool {
        #[cfg(windows)]
        {
            crate::clipboard_snapshot::Snapshot::read().is_err()
        }
        #[cfg(not(windows))]
        {
            true
        } // Native format enumeration is required before these platforms are qualified.
    }

    /// Simula um atalho de clipboard: <modificador>(+Shift)+`key`. O modificador e Cmd no macOS,
    /// Ctrl no resto. O Shift so entra no modo terminal (so no Windows).
    ///
    /// A tecla (C/V) e enviada como VIRTUAL KEY FISICO (`clip_key`), nao como `Key::Unicode`. O
    /// enigo, com Unicode, cai num evento KEYEVENTF_UNICODE (caractere puro, VK=0) que o Windows
    /// Terminal NAO associa aos modificadores: um Ctrl+Shift+<char c> injetado nao dispara o
    /// atalho de copia (o copy manual funciona, o sintetico nao). Com o VK fisico (VK_C=0x43), o
    /// SendInput gera uma tecla real com scancode, que o terminal reconhece como Ctrl+Shift+C.
    ///
    /// Pausas curtas (`KEY_SETTLE_MS`) entre premir os modificadores, a tecla e soltar: apps de
    /// input assincrono podiam receber a tecla antes de registarem os modificadores.
    fn combo(&mut self, key: char) {
        let modifier = clipboard_modifier();
        let k = clip_key(key);
        // No terminal, settles MAIS LONGOS: uma TUI com mouse-tracking (ex. Claude Code) processa
        // input de forma assincrona e re-desenha o ecra; um combo demasiado rapido chega antes de
        // a app registar os modificadores ou perde-se a meio de um redraw. Imita melhor um Ctrl+
        // Shift+C humano (modificadores premidos ~100ms). Fora do terminal mantem-se rapido.
        let hold = if self.terminal {
            std::time::Duration::from_millis(45)
        } else {
            std::time::Duration::from_millis(KEY_SETTLE_MS)
        };
        self.input_failed |= self.enigo.key(modifier, Press).is_err();
        if self.terminal {
            self.input_failed |= self.enigo.key(Key::Shift, Press).is_err();
        }
        std::thread::sleep(hold); // modificadores assentam antes da tecla
        self.input_failed |= self.enigo.key(k, Press).is_err();
        std::thread::sleep(hold); // tecla premida com os modificadores em baixo
        self.input_failed |= self.enigo.key(k, Release).is_err();
        std::thread::sleep(hold);
        if self.terminal {
            self.input_failed |= self.enigo.key(Key::Shift, Release).is_err();
        }
        self.input_failed |= self.enigo.key(modifier, Release).is_err();
        // Settle APOS soltar tudo: da tempo a app processar o atalho e escrever o clipboard antes
        // do primeiro poll (antes nao havia pausa aqui, o poll podia ler o clipboard cedo demais).
        if self.terminal {
            std::thread::sleep(hold);
        }
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

impl SelectionIo for RealIo {
    fn clip_get(&mut self) -> Option<String> {
        self.clip.get_text().ok()
    }
    fn clip_set(&mut self, s: &str) {
        let _ = self.clip.set_text(s.to_string());
    }
    fn clip_clear(&mut self) {
        let _ = self.clip.clear();
    }
    fn clip_revision(&self) -> Option<u64> {
        #[cfg(windows)]
        {
            Some(
                unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
                    as u64,
            )
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
    fn modifiers_held(&mut self) -> ember_core::ModifierState {
        physical_modifiers()
    }
    fn release_modifiers(&mut self) {
        self.input_failed |= self.enigo.key(Key::Shift, Release).is_err();
        self.input_failed |= self.enigo.key(Key::Control, Release).is_err();
        self.input_failed |= self.enigo.key(Key::Alt, Release).is_err();
        self.input_failed |= self.enigo.key(Key::Meta, Release).is_err();
    }
    fn send_copy(&mut self) {
        self.combo('c');
    }
    fn send_select_all(&mut self) {
        // Ctrl+A (Cmd+A no macOS) SEM Shift, mesmo em terminal. Na pratica isto nunca corre em
        // terminal (`ember_core::selection::capture` corta o fallback la), mas o `combo` junta
        // Shift quando `self.terminal`, e um Ctrl+Shift+A nao e select-all em lado nenhum.
        let modifier = clipboard_modifier();
        let k = clip_key('a');
        let hold = std::time::Duration::from_millis(KEY_SETTLE_MS);
        self.input_failed |= self.enigo.key(modifier, Press).is_err();
        std::thread::sleep(hold);
        self.input_failed |= self.enigo.key(k, Press).is_err();
        std::thread::sleep(hold);
        self.input_failed |= self.enigo.key(k, Release).is_err();
        std::thread::sleep(hold);
        self.input_failed |= self.enigo.key(modifier, Release).is_err();
        // Settle antes do copy que vem a seguir: a app precisa de processar a seleccao primeiro.
        std::thread::sleep(hold);
    }
    fn send_paste(&mut self) {
        self.combo('v');
    }
    fn sleep_ms(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}
