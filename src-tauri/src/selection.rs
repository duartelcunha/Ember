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
pub struct ClipImage {
    width: usize,
    height: usize,
    bytes: Vec<u8>,
}

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

/// A tecla C/V de um atalho de clipboard, por plataforma. No Windows usa o VIRTUAL KEY fisico
/// (`Key::Other(VK)`): VK_C=0x43, VK_V=0x56. Um `Key::Unicode` injetaria um caractere puro que o
/// Windows Terminal nao liga aos modificadores (o atalho de copia nao dispara). Nas outras
/// plataformas o Unicode funciona com Cmd/Ctrl.
#[cfg(windows)]
fn clip_key(c: char) -> Key {
    match c {
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
        let _ = self.enigo.key(modifier, Press);
        if self.terminal {
            let _ = self.enigo.key(Key::Shift, Press);
        }
        std::thread::sleep(hold); // modificadores assentam antes da tecla
        let _ = self.enigo.key(k, Press);
        std::thread::sleep(hold); // tecla premida com os modificadores em baixo
        let _ = self.enigo.key(k, Release);
        std::thread::sleep(hold);
        if self.terminal {
            let _ = self.enigo.key(Key::Shift, Release);
        }
        let _ = self.enigo.key(modifier, Release);
        // Settle APOS soltar tudo: da tempo a app processar o atalho e escrever o clipboard antes
        // do primeiro poll (antes nao havia pausa aqui, o poll podia ler o clipboard cedo demais).
        if self.terminal {
            std::thread::sleep(hold);
        }
    }

    /// Pequena pausa para o input assentar entre eventos de tecla (ver `combo`).
    fn settle(&mut self) {
        std::thread::sleep(std::time::Duration::from_millis(KEY_SETTLE_MS));
    }

    /// Limpa a linha de input atual antes de colar o refinado: End (garante o cursor no fim da
    /// linha logica) e Ctrl+U (o "kill-to-start" do readline: apaga da posicao do cursor ate ao
    /// inicio da linha), o que remove a linha toda. So usado no modo terminal.
    ///
    /// Porque nao Shift+Home + paste (o que se fazia antes): a seleccao feita com o RATO num
    /// terminal e uma seleccao de nivel-terminal, invisivel ao widget de input da app (ex. Claude
    /// Code). Um Shift+Home nao e honrado la como seleccao editavel, por isso o paste caia no
    /// inicio da linha e o texto original ficava (o refinado era so PREPENDido). O Ctrl+U apaga
    /// fisicamente os caracteres sem depender de a app renderizar/consumir uma seleccao. Isto
    /// substitui a LINHA INTEIRA: acerta o caso dominante (refinar o prompt todo). Substituir so
    /// parte de uma seleccao de rato nao e fiavel num terminal (nao sabemos onde no buffer
    /// editavel esta o trecho), por isso nao se tenta.
    fn clear_input_line(&mut self) {
        let _ = self.enigo.key(Key::End, Press);
        let _ = self.enigo.key(Key::End, Release);
        self.settle();
        let _ = self.enigo.key(Key::Control, Press);
        self.settle();
        // VK_U fisico (0x55), pelo mesmo motivo que o clip_key usa VK fisicos: um Key::Unicode
        // cairia num evento KEYEVENTF_UNICODE que o terminal nao liga ao Ctrl.
        let _ = self.enigo.key(Key::Other(0x55), Press);
        let _ = self.enigo.key(Key::Other(0x55), Release);
        self.settle();
        let _ = self.enigo.key(Key::Control, Release);
        self.settle();
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
        // No terminal, a "seleccao" de rato nao e editavel (so serve para copiar): um paste
        // simples inseria o refinado A SEGUIR ao texto original em vez de o substituir. Antes de
        // colar, limpamos a LINHA DE INPUT atual (End -> Ctrl+U) para o paste a substituir.
        // Funciona no caso tipico: refinar o prompt todo que se esta a escrever.
        if self.terminal {
            self.clear_input_line();
        }
        self.combo('v');
    }
    fn sleep_ms(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}
