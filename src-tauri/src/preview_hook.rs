//! Gate de aprovacao "preview before paste": depois de refinar, espera que o utilizador
//! aprove (Enter) ou recuse (Esc) antes de colar. A captura das teclas usa um low-level
//! keyboard hook do Windows (WH_KEYBOARD_LL) que CONSOME so o Enter/Esc durante o gate: assim
//! essas teclas nao vazam para a app em foco (o Enter nao mete newline no editor) e a overlay
//! nao precisa de roubar foco (a invariante sagrada: o paste aterra na app do utilizador).
//!
//! O `unsafe` do Win32 vive todo aqui, isolado. As pecas puras (`classify_key`, `Decision`,
//! `PREVIEW_TIMEOUT`) sao cross-platform e testadas em qualquer SO.

/// O que o utilizador decidiu no gate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision {
    /// Aplicar: colar o texto refinado (Enter).
    Accept,
    /// Recusar: manter o original, nao colar nada (Esc, timeout, ou hotkey durante o gate).
    Reject,
}

/// O que o hook faz com uma tecla premida durante o gate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyVerdict {
    /// Nao decide nada e a tecla segue o seu caminho normal.
    PassThrough,
    /// Decide o gate. `consume` = a tecla NAO chega a app em foco.
    Decide { decision: Decision, consume: bool },
}

/// Modificadores puros. Premir Shift ou Ctrl nao e "continuar a trabalhar", e o inicio de um
/// atalho ou de uma maiuscula: se contassem, o preview fugia mal a pessoa encostasse a um deles.
fn is_modifier(vk: u32) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12 // Shift / Ctrl / Alt genericos
            | 0x14 // Caps Lock
            | 0x5B | 0x5C // Win esquerda/direita
            | 0xA0..=0xA5 // Shift/Ctrl/Alt esquerda e direita
    )
}

/// Classificador puro: o que uma tecla premida significa no gate. Testavel em qualquer SO.
///
/// Enter e Esc respondem a pergunta e sao CONSUMIDOS, para o Enter nao meter uma newline no
/// editor por baixo. Qualquer outra tecla (que nao seja um modificador) quer dizer que a pessoa
/// seguiu em frente e ja nao esta a olhar para o preview: mantem-se o original e a pilula sai da
/// frente, mas a tecla NAO e consumida, porque ela estava a escrever na app dela e engolir-lhe um
/// caracter seria pior do que qualquer pilula esquecida.
pub fn classify_key(vk: u32) -> KeyVerdict {
    match vk {
        0x0D => KeyVerdict::Decide {
            decision: Decision::Accept,
            consume: true,
        }, // VK_RETURN
        0x1B => KeyVerdict::Decide {
            decision: Decision::Reject,
            consume: true,
        }, // VK_ESCAPE
        vk if is_modifier(vk) => KeyVerdict::PassThrough,
        _ => KeyVerdict::Decide {
            decision: Decision::Reject,
            consume: false,
        },
    }
}

// ---------------------------------------------------------------------------------------
// Watcher de Esc durante o refine (cancelar "anytime")
// ---------------------------------------------------------------------------------------

/// O que o watcher faz com um evento de teclado durante o refine. So o Esc lhe interessa; e um
/// hook muito mais estreito que o do gate, porque durante o refine o utilizador continua a
/// trabalhar na app dele e NADA do que ele escreve nos diz respeito.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WatchVerdict {
    /// Esc fresco: cancela o refine e CONSOME a tecla (o utilizador carregou para nos parar; se a
    /// tecla seguisse, a app dele tambem a recebia e fechava um dialog qualquer).
    Cancel,
    /// Cauda da pressao que ja consumimos (repeticoes e key-up): continua a consumir, senao o
    /// resto da pressao vazava para a app.
    ConsumeTail,
    /// Key-up de um Esc que ja estava premido QUANDO o watcher instalou: a pressao era da app
    /// dele, o up e dela. Passa, e a partir daqui o proximo Esc ja conta.
    ReleaseHeld,
    /// Tudo o resto, Esc herdado incluido: passa intocado.
    Pass,
}

/// Classificador puro do watcher. `ignoring_held` = o Esc ja estava em baixo na instalacao;
/// `decided` = ja consumimos um keydown fresco e estamos a engolir a cauda.
pub fn classify_watch_event(vk: u32, is_down: bool, ignoring_held: bool, decided: bool) -> WatchVerdict {
    if vk != 0x1B {
        return WatchVerdict::Pass;
    }
    if decided {
        // Depois de decidir, TUDO o que for Esc e cauda da nossa pressao ate ao key-up.
        return WatchVerdict::ConsumeTail;
    }
    if is_down {
        if ignoring_held {
            // Auto-repeat de uma pressao que comecou antes de nos: e da app dele.
            return WatchVerdict::Pass;
        }
        return WatchVerdict::Cancel;
    }
    if ignoring_held {
        return WatchVerdict::ReleaseHeld;
    }
    WatchVerdict::Pass
}

/// Prazo total do gate: se o utilizador nao responder, recusa (mantem o original). O silencio
/// nunca vira um paste.
pub const PREVIEW_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

// ---------------------------------------------------------------------------------------
// Windows: o hook real
// ---------------------------------------------------------------------------------------

#[cfg(windows)]
mod imp {
    use super::{classify_key, Decision, KeyVerdict, PREVIEW_TIMEOUT};
    use std::sync::atomic::{AtomicU8, Ordering};
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, MsgWaitForMultipleObjectsEx, PeekMessageW,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, HHOOK,
        KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT,
        WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
        WM_MOUSEMOVE, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    // O callback e um `extern "system" fn` e nao captura estado: comunica pela decisao global.
    // 0 = pendente, 1 = accept, 2 = reject.
    static HOOK_DECISION: AtomicU8 = AtomicU8::new(0);
    // Ignorar teclas ja fisicamente premidas quando o hook instala (ex.: o Enter que disparou o
    // proprio refine). So contam apos uma transicao up->down fresca. Bit por tecla: 1=Enter,2=Esc.
    static IGNORE_HELD: AtomicU8 = AtomicU8::new(0);
    // Teclas cujo key-UP o hook ja viu nesta sessao de gate. Bits iguais aos de IGNORE_HELD.
    // O gate decide no key-DOWN, mas so LARGA o hook depois de ver a tecla subir: ver
    // `drain_until_released`.
    static RELEASED: AtomicU8 = AtomicU8::new(0);

    const IGN_ENTER: u8 = 1;
    const IGN_ESC: u8 = 2;

    fn ignore_bit(vk: u32) -> u8 {
        match vk {
            0x0D => IGN_ENTER,
            0x1B => IGN_ESC,
            _ => 0,
        }
    }

    unsafe extern "system" fn ll_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let msg = wparam.0 as u32;
            let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let vk = kb.vkCode;
            let bit = ignore_bit(vk);
            // Teclas que NAO sao o Enter nem o Esc: se decidirem alguma coisa (a pessoa continuou
            // a escrever), marcam a decisao e seguem o seu caminho na mesma. Nunca se consomem, e
            // por isso este ramo cai sempre no `CallNextHookEx` la em baixo.
            if bit == 0 {
                if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
                    if let KeyVerdict::Decide { decision, .. } = classify_key(vk) {
                        let _ = HOOK_DECISION.compare_exchange(
                            0,
                            if decision == Decision::Accept { 1 } else { 2 },
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        );
                    }
                }
                return CallNextHookEx(None, code, wparam, lparam);
            }
            {
                let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
                let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
                // Uma tecla que ja estava premida na instalacao: espera pelo key-up para limpar o
                // "ignorar", so a proxima descida conta. Enquanto isso, consome na mesma (nao deve
                // vazar para a app), mas nao decide.
                let ignoring = IGNORE_HELD.load(Ordering::SeqCst) & bit != 0;
                if is_up {
                    IGNORE_HELD.fetch_and(!bit, Ordering::SeqCst);
                    RELEASED.fetch_or(bit, Ordering::SeqCst);
                    return LRESULT(1); // consome o key-up de Enter/Esc para nao deixar cauda
                }
                if is_down {
                    if !ignoring {
                        if let KeyVerdict::Decide { decision, .. } = classify_key(vk) {
                            HOOK_DECISION.store(
                                if decision == Decision::Accept { 1 } else { 2 },
                                Ordering::SeqCst,
                            );
                        }
                    }
                    return LRESULT(1); // consome: a app em foco nunca ve este Enter/Esc
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    /// Teto da espera pelo key-up da tecla da decisao. Uma tecla presa (ou um key-up que o hook
    /// nunca chega a ver) nunca pode pendurar o refine: ao fim disto seguimos na mesma.
    const RELEASE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

    /// Bombeia mensagens, com o hook AINDA INSTALADO, ate a tecla `bit` subir de verdade.
    ///
    /// Sem isto o gate devolvia `Accept` no key-DOWN do Enter e o hook caia logo a seguir: o
    /// resto daquela pressao (o key-up e, se o dedo demorasse uns milissegundos, as REPETICOES
    /// automaticas do Windows) chegava a app em foco sem ninguem a consumir. Num terminal isso
    /// era um Enter novo: o Claude Code submetia o prompt sozinho, antes de o utilizador sequer
    /// ver o texto colado. Enquanto o hook vive, o `ll_proc` engole tudo isso.
    ///
    /// Nao usa `GetAsyncKeyState`: o stream de eventos do proprio hook e a fonte da verdade (o
    /// GetAsyncKeyState ja provou mentir nesta app quando ha um hotkey global registado).
    fn drain_until_released(bit: u8) {
        let start = std::time::Instant::now();
        while RELEASED.load(Ordering::SeqCst) & bit == 0 {
            let mut msg = MSG::default();
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            if start.elapsed() >= RELEASE_TIMEOUT {
                log::warn!("gate: key-up never seen (bit={bit}); proceeding after timeout");
                return;
            }
            unsafe {
                MsgWaitForMultipleObjectsEx(None, 10, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            }
        }
        log::info!("gate: key released after {:?}", start.elapsed());
    }

    /// RAII: garante `UnhookWindowsHookEx` em todos os caminhos de saida (decisao, cancel,
    /// timeout, panic).
    struct HookGuard(HHOOK);
    impl Drop for HookGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = UnhookWindowsHookEx(self.0);
            }
        }
    }

    /// Corre o gate numa thread dedicada com message pump (o LL hook so entrega o callback na
    /// thread que instala E bombeia mensagens). Bloqueante: chamar fora do runtime tokio.
    pub fn run_gate_blocking(should_cancel: impl Fn() -> bool) -> Decision {
        HOOK_DECISION.store(0, Ordering::SeqCst);
        RELEASED.store(0, Ordering::SeqCst);
        // Marca as teclas ja premidas agora (bit alto do GetAsyncKeyState) para as ignorar ate
        // uma descida fresca. Evita um falso Accept do Enter que ainda estava em baixo.
        let mut held = 0u8;
        unsafe {
            if (GetAsyncKeyState(0x0D) as u16 & 0x8000) != 0 {
                held |= IGN_ENTER;
            }
            if (GetAsyncKeyState(0x1B) as u16 & 0x8000) != 0 {
                held |= IGN_ESC;
            }
        }
        IGNORE_HELD.store(held, Ordering::SeqCst);

        log::info!("gate: starting (held_at_install={held})");
        let hmod = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
        let hook = match unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_proc), Some(HINSTANCE(hmod.0)), 0)
        } {
            Ok(h) => h,
            // Nao conseguimos instalar o hook: degrada para colar (nunca perde um refine bom).
            Err(e) => {
                log::warn!("gate: HOOK INSTALL FAILED ({e}); pasting without approval");
                return Decision::Accept;
            }
        };
        let _guard = HookGuard(hook);
        let start = std::time::Instant::now();

        loop {
            // 1) Bombeia mensagens: serve o callback do LL hook.
            let mut msg = MSG::default();
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            // 2) Decisao vinda do callback? Antes de largar o hook, espera o key-up REAL da
            //    tecla premida: enquanto o dedo estiver em baixo, o hook tem de continuar a
            //    engolir as repeticoes automaticas, senao vazam para a app (num terminal, um
            //    Enter vazado submete o prompt sozinho).
            match HOOK_DECISION.load(Ordering::SeqCst) {
                1 => {
                    log::info!("gate: ACCEPT (Enter consumed by hook)");
                    drain_until_released(IGN_ENTER);
                    return Decision::Accept;
                }
                2 => {
                    log::info!("gate: REJECT (Esc consumed by hook)");
                    drain_until_released(IGN_ESC);
                    return Decision::Reject;
                }
                _ => {}
            }
            // 3) Cancel externo (hotkey durante o preview) -> recusa.
            if should_cancel() {
                log::info!("gate: REJECT (cancelled)");
                return Decision::Reject;
            }
            // 4) Prazo total -> recusa (nunca colar sem aprovacao explicita).
            if start.elapsed() >= PREVIEW_TIMEOUT {
                log::info!("gate: REJECT (timeout, no key seen)");
                return Decision::Reject;
            }
            // 5) Espera eficiente: acorda ja no input (Enter/Esc imediato), senao 50ms para
            //    re-checar cancel/prazo. Mantem o LL hook responsivo (callback trivial, nunca
            //    estoura o LowLevelHooksTimeout ~300ms).
            unsafe {
                MsgWaitForMultipleObjectsEx(None, 50, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            }
        }
        // _guard cai aqui -> UnhookWindowsHookEx
    }

    /// Wrapper async: spawna a thread do gate e espera o resultado por oneshot (race-free).
    pub async fn gate(app: tauri::AppHandle) -> Decision {
        use tauri::Manager;
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let d = run_gate_blocking(|| {
                app.state::<crate::state::AppState>()
                    .cancel
                    .load(Ordering::SeqCst)
            });
            let _ = tx.send(d); // se o lado async caiu (app a sair), o send falha inofensivamente
        });
        rx.await.unwrap_or(Decision::Reject)
    }

    // -----------------------------------------------------------------------------------
    // Watcher de Esc durante o refine
    // -----------------------------------------------------------------------------------
    //
    // Estado PROPRIO, separado do gate de proposito: partilhar os atomicos convidava um modo a
    // pisar o estado do outro. O flow garante que nunca ha dois hooks vivos ao mesmo tempo
    // (`stop_and_join` antes de instalar o gate), e estes estaticos garantem que, mesmo que essa
    // garantia falhe um dia, um nao corrompe as decisoes do outro.
    static WATCH_DECIDED: AtomicU8 = AtomicU8::new(0); // 0 = a ouvir, 1 = Esc consumido
    static WATCH_RELEASED: AtomicU8 = AtomicU8::new(0);
    static WATCH_IGNORE_HELD: AtomicU8 = AtomicU8::new(0);

    unsafe extern "system" fn esc_watch_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let msg = wparam.0 as u32;
            let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            if is_down || is_up {
                let verdict = super::classify_watch_event(
                    kb.vkCode,
                    is_down,
                    WATCH_IGNORE_HELD.load(Ordering::SeqCst) != 0,
                    WATCH_DECIDED.load(Ordering::SeqCst) != 0,
                );
                match verdict {
                    super::WatchVerdict::Cancel => {
                        WATCH_DECIDED.store(1, Ordering::SeqCst);
                        return LRESULT(1);
                    }
                    super::WatchVerdict::ConsumeTail => {
                        if is_up {
                            WATCH_RELEASED.store(1, Ordering::SeqCst);
                        }
                        return LRESULT(1);
                    }
                    super::WatchVerdict::ReleaseHeld => {
                        WATCH_IGNORE_HELD.store(0, Ordering::SeqCst);
                        // Passa: o up pertence a pressao da app dele.
                    }
                    super::WatchVerdict::Pass => {}
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    /// Handle do watcher. `stop_and_join` para o hook E espera que ele caia, e e OBRIGATORIO
    /// antes de instalar o gate do preview: dois hooks LL vivos a consumir Esc davam o Esc do
    /// preview engolido pelo watcher. O `Drop` cobre os returns precoces do flow (so para, sem
    /// join; nesses caminhos nenhum segundo hook e instalado).
    pub struct EscWatcher {
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        join: Option<std::thread::JoinHandle<()>>,
    }

    impl EscWatcher {
        pub fn stop_and_join(mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(j) = self.join.take() {
                // O pump acorda a cada <=50ms; este join e curto e garante a ordem hook-a-hook.
                let _ = j.join();
            }
        }
    }

    impl Drop for EscWatcher {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
        }
    }

    /// Instala o watcher numa thread propria (o LL hook entrega na thread que instala e bombeia)
    /// e devolve o handle. Ao apanhar um Esc fresco aciona o caminho de cancelamento QUE JA
    /// EXISTE (`state.cancel` + `cancel_notify`): zero logica nova de cancelar.
    pub fn spawn_esc_watcher(app: tauri::AppHandle) -> EscWatcher {
        use tauri::Manager;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        let join = std::thread::spawn(move || {
            WATCH_DECIDED.store(0, Ordering::SeqCst);
            WATCH_RELEASED.store(0, Ordering::SeqCst);
            // Um Esc ja em baixo na instalacao e da app do utilizador, nao nosso: fica marcado
            // para as suas repeticoes e o seu key-up passarem, e so a proxima descida contar.
            let held = unsafe { (GetAsyncKeyState(0x1B) as u16 & 0x8000) != 0 };
            WATCH_IGNORE_HELD.store(u8::from(held), Ordering::SeqCst);

            let hmod = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
            let hook = match unsafe {
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(esc_watch_proc), Some(HINSTANCE(hmod.0)), 0)
            } {
                Ok(h) => h,
                Err(e) => {
                    // Sem hook nao ha Esc-cancel neste ciclo; o atalho continua a cancelar.
                    // Nunca falhar o refine por causa de observabilidade de teclado.
                    log::warn!("esc-watch: hook install failed ({e}); Esc won't cancel this cycle");
                    return;
                }
            };
            let _guard = HookGuard(hook);
            let mut notified = false;
            loop {
                let mut msg = MSG::default();
                while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                    unsafe {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                if WATCH_DECIDED.load(Ordering::SeqCst) != 0 && !notified {
                    notified = true;
                    log::info!("esc-watch: Esc consumido, a cancelar o refine");
                    let st = app.state::<crate::state::AppState>();
                    st.cancel.store(true, Ordering::SeqCst);
                    st.cancel_notify.notify_waiters();
                }
                // Depois de decidir, o hook so vive para engolir a cauda da pressao; visto o
                // key-up, ja nao ha nada a proteger.
                if notified && WATCH_RELEASED.load(Ordering::SeqCst) != 0 {
                    break;
                }
                if stop2.load(Ordering::SeqCst) {
                    break;
                }
                unsafe {
                    MsgWaitForMultipleObjectsEx(None, 50, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                }
            }
        });
        EscWatcher { stop, join: Some(join) }
    }

    // -----------------------------------------------------------------------------------
    // Modo picker: setas navegam uma lista, Enter confirma, sem nunca roubar o foco
    // -----------------------------------------------------------------------------------
    //
    // Estado proprio outra vez (nem do gate nem do watcher): o flow garante um hook de cada vez,
    // e os estaticos separados garantem que um modo nunca le decisoes do outro.
    //
    // O INDICE vive aqui, no Rust, e nao no webview: a janela do picker nunca tem foco (regra
    // sagrada: o paste tem de aterrar na app do utilizador), logo nunca ha keydown no DOM dela.
    static PICKER_INDEX: AtomicU8 = AtomicU8::new(0);
    static PICKER_LEN: AtomicU8 = AtomicU8::new(0);
    /// 0 = a navegar, 1 = commit, 2 = cancel, 3 = dismiss (tecla alheia, nao consumida).
    static PICKER_DECISION: AtomicU8 = AtomicU8::new(0);
    /// Bitmask das teclas CONSUMIDAS que estao fisicamente em baixo agora. O hook so pode cair
    /// quando isto chegar a zero: largar o hook com uma seta ainda premida despejava o
    /// auto-repeat dela na app (o caret andava sozinho depois de o menu fechar).
    static PICKER_HELD: AtomicU8 = AtomicU8::new(0);
    /// Houve movimento por emitir? O pump le e emite o evento; o callback nunca emite (tem de
    /// ser trivial para nunca encostar ao LowLevelHooksTimeout).
    static PICKER_MOVED: AtomicU8 = AtomicU8::new(0);
    /// Geometria da janela em pixeis FISICOS, para o hook do rato saber que linha esta debaixo do
    /// ponteiro. Escrita uma vez antes de instalar os hooks e so lida a partir dai.
    static PICKER_GEOM: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    static PICKER_GEOM2: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    /// Um clique nosso ja foi consumido e falta engolir o `button up` que vem a seguir. Sem isto
    /// a app por baixo recebia metade do clique.
    static PICKER_CLICK_TAIL: AtomicU8 = AtomicU8::new(0);

    /// Empacota a geometria em dois inteiros: (x, y) e (largura, primeira linha visivel).
    fn pack(a: i32, b: i32) -> i64 {
        ((a as i64) << 32) | (b as u32 as i64)
    }
    fn unpack(v: i64) -> (i32, i32) {
        ((v >> 32) as i32, v as u32 as i32)
    }

    /// Padding e altura de linha em fisicos, derivados no momento da leitura a partir da escala
    /// guardada nos bits baixos. Mantidos em constantes porque a UI espelha os mesmos numeros.
    static PICKER_PAD_PX: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
    static PICKER_ITEM_PX: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
    static PICKER_VISIBLE: AtomicU8 = AtomicU8::new(0);

    unsafe extern "system" fn picker_mouse_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code == HC_ACTION as i32 {
            let msg = wparam.0 as u32;
            let ms = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            let (win_x, win_y) = unpack(PICKER_GEOM.load(Ordering::SeqCst));
            let (win_w, first) = unpack(PICKER_GEOM2.load(Ordering::SeqCst));
            let linha = ember_core::projects::picker_row_at(
                ms.pt.x,
                ms.pt.y,
                win_x,
                win_y,
                win_w,
                PICKER_PAD_PX.load(Ordering::SeqCst),
                PICKER_ITEM_PX.load(Ordering::SeqCst),
                PICKER_VISIBLE.load(Ordering::SeqCst) as usize,
                first as usize,
            );
            match msg {
                WM_MOUSEMOVE => {
                    // Passar por cima destaca, como em qualquer menu. Nunca consome o movimento.
                    if let Some(i) = linha {
                        if PICKER_DECISION.load(Ordering::SeqCst) == 0
                            && PICKER_INDEX.load(Ordering::SeqCst) != i as u8
                        {
                            PICKER_INDEX.store(i as u8, Ordering::SeqCst);
                            PICKER_MOVED.store(1, Ordering::SeqCst);
                        }
                    }
                }
                WM_LBUTTONDOWN => {
                    log::debug!(
                        "picker: clique em ({}, {}) -> linha {linha:?} (janela {win_x},{win_y} {win_w}px)",
                        ms.pt.x,
                        ms.pt.y
                    );
                    if let Some(i) = linha {
                        // Clique DENTRO da lista: escolhe. Consome, senao o clique ia parar a
                        // app por baixo (a janela e click-through de proposito).
                        PICKER_INDEX.store(i as u8, Ordering::SeqCst);
                        let _ = PICKER_DECISION.compare_exchange(
                            0,
                            1,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        );
                        PICKER_CLICK_TAIL.store(1, Ordering::SeqCst);
                        return LRESULT(1);
                    }
                    // Clique FORA: fecha sem escolher, e o clique segue para a app. Era dela.
                    let _ = PICKER_DECISION.compare_exchange(
                        0,
                        2,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    );
                }
                WM_LBUTTONUP => {
                    if PICKER_CLICK_TAIL.swap(0, Ordering::SeqCst) != 0 {
                        return LRESULT(1); // cauda do clique que ja consumimos
                    }
                }
                _ => {}
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }


    fn picker_key_bit(vk: u32) -> u8 {
        match vk {
            0x26 => 1,  // Up
            0x28 => 2,  // Down
            0x25 => 4,  // Left
            0x27 => 8,  // Right
            0x0D => 16, // Enter
            0x09 => 32, // Tab
            0x1B => 64, // Esc
            _ => 0,
        }
    }

    unsafe extern "system" fn picker_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let msg = wparam.0 as u32;
            let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let vk = kb.vkCode;
            let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            if is_down || is_up {
                use ember_core::projects::{classify_picker_key, move_index, PickerVerdict};
                let verdict = classify_picker_key(vk);
                if is_down {
                    // Cada tecla que o picker VE fica registada. A primeira utilizacao real
                    // acabou com a lista oito segundos aberta sem nada acontecer, e sem isto nao
                    // havia como distinguir "nao carregou" de "carregou e o hook nao viu".
                    log::debug!("picker: tecla vk={vk:#x} -> {verdict:?}");
                }
                match verdict {
                    PickerVerdict::Move(delta) => {
                        let bit = picker_key_bit(vk);
                        if is_down {
                            PICKER_HELD.fetch_or(bit, Ordering::SeqCst);
                            // Auto-repeat de uma seta premida conta como movimentos seguidos:
                            // e o gesto normal de "segurar Down para descer a lista".
                            if PICKER_DECISION.load(Ordering::SeqCst) == 0 {
                                let len = PICKER_LEN.load(Ordering::SeqCst) as usize;
                                let atual = PICKER_INDEX.load(Ordering::SeqCst) as usize;
                                let novo = move_index(atual, delta, len);
                                PICKER_INDEX.store(novo as u8, Ordering::SeqCst);
                                PICKER_MOVED.store(1, Ordering::SeqCst);
                            }
                        } else {
                            PICKER_HELD.fetch_and(!bit, Ordering::SeqCst);
                        }
                        return LRESULT(1); // consumida: o caret da app nao anda com o menu
                    }
                    v @ (PickerVerdict::Commit | PickerVerdict::Cancel) => {
                        let bit = picker_key_bit(vk);
                        if is_down {
                            PICKER_HELD.fetch_or(bit, Ordering::SeqCst);
                            let d = if v == PickerVerdict::Commit { 1 } else { 2 };
                            let _ = PICKER_DECISION.compare_exchange(
                                0,
                                d,
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                            );
                        } else {
                            PICKER_HELD.fetch_and(!bit, Ordering::SeqCst);
                        }
                        return LRESULT(1);
                    }
                    PickerVerdict::Ignore => {}
                    PickerVerdict::DismissWithoutConsuming => {
                        if is_down {
                            let _ = PICKER_DECISION.compare_exchange(
                                0,
                                3,
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                            );
                        }
                        // NAO consome: a tecla era do utilizador e segue para a app dele. E a
                        // mesma regra pinada no teste do gate, e e inegociavel.
                    }
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    /// O que o picker devolveu.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum PickerOutcome {
        /// Enter/Tab no indice dado.
        Committed(usize),
        /// Esc, tecla alheia, timeout, ou cancelamento externo.
        Cancelled,
    }

    /// Um menu esquecido sai da frente sozinho, mas a contagem e de INATIVIDADE e nao de vida:
    /// medida desde a abertura, seis segundos matavam a lista enquanto a pessoa ainda a estava a
    /// ler pela primeira vez. Cada seta que ela carrega poe o relogio a zero.
    const PICKER_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

    /// Corre o picker (bloqueante, numa thread com pump). `on_move` e chamado no PUMP (nunca no
    /// callback) com o indice novo, para o shell emitir o evento a UI.
    /// Geometria da janela do picker em pixeis FISICOS, que e a unidade em que o hook do rato
    /// recebe o ponteiro.
    #[derive(Clone, Copy)]
    pub struct PickerGeom {
        pub x: i32,
        pub y: i32,
        pub w: i32,
        pub pad: i32,
        pub item_h: i32,
        pub visible: usize,
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run_picker_blocking(
        len: usize,
        initial: usize,
        geom: PickerGeom,
        should_cancel: impl Fn() -> bool,
        on_move: impl Fn(usize),
    ) -> PickerOutcome {
        PICKER_DECISION.store(0, Ordering::SeqCst);
        PICKER_HELD.store(0, Ordering::SeqCst);
        PICKER_MOVED.store(0, Ordering::SeqCst);
        PICKER_CLICK_TAIL.store(0, Ordering::SeqCst);
        PICKER_LEN.store(len.min(u8::MAX as usize) as u8, Ordering::SeqCst);
        PICKER_INDEX.store(initial.min(len.saturating_sub(1)) as u8, Ordering::SeqCst);
        PICKER_GEOM.store(pack(geom.x, geom.y), Ordering::SeqCst);
        PICKER_GEOM2.store(pack(geom.w, 0), Ordering::SeqCst);
        PICKER_PAD_PX.store(geom.pad, Ordering::SeqCst);
        PICKER_ITEM_PX.store(geom.item_h, Ordering::SeqCst);
        PICKER_VISIBLE.store(geom.visible.min(u8::MAX as usize) as u8, Ordering::SeqCst);

        let hmod = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
        let hook = match unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(picker_proc), Some(HINSTANCE(hmod.0)), 0)
        } {
            Ok(h) => h,
            // Um picker que nao consegue ouvir o teclado nao pode ficar no ecra a fingir que
            // ouve, e muito menos confirmar seja o que for: cancela.
            Err(e) => {
                log::warn!("picker: hook install failed ({e})");
                return PickerOutcome::Cancelled;
            }
        };
        let _guard = HookGuard(hook);
        // O rato e o segundo caminho, e nao um extra: um menu que aparece debaixo do ponteiro e
        // clicado, nao percorrido a setas. Se ESTE nao instalar, a lista continua a servir pelo
        // teclado, por isso a falha aqui e um aviso e nao um cancelamento.
        let mouse_hook = unsafe {
            SetWindowsHookExW(WH_MOUSE_LL, Some(picker_mouse_proc), Some(HINSTANCE(hmod.0)), 0)
        };
        let _mouse_guard = match mouse_hook {
            Ok(h) => Some(HookGuard(h)),
            Err(e) => {
                log::warn!("picker: mouse hook install failed ({e}); so teclado");
                None
            }
        };
        let mut last_activity = std::time::Instant::now();
        log::info!("picker: aberto ({len} linhas, indice inicial {initial})");

        loop {
            let mut msg = MSG::default();
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            if PICKER_MOVED.swap(0, Ordering::SeqCst) != 0 {
                last_activity = std::time::Instant::now();
                let i = PICKER_INDEX.load(Ordering::SeqCst) as usize;
                // A lista desliza quando ha mais linhas do que cabem; o rato tem de apontar para
                // as linhas que estao MESMO a ser mostradas. Mesma conta que a UI faz.
                let first = i
                    .saturating_sub(geom.visible.saturating_sub(1))
                    .min(len.saturating_sub(geom.visible));
                PICKER_GEOM2.store(pack(geom.w, first as i32), Ordering::SeqCst);
                on_move(i);
            }
            let d = PICKER_DECISION.load(Ordering::SeqCst);
            if d != 0 {
                // Antes de largar o hook, espera que as teclas consumidas subam: largar com uma
                // seta (ou o Enter) ainda em baixo despejava o auto-repeat na app.
                let drain_start = std::time::Instant::now();
                while PICKER_HELD.load(Ordering::SeqCst) != 0
                    && drain_start.elapsed() < RELEASE_TIMEOUT
                {
                    let mut m2 = MSG::default();
                    while unsafe { PeekMessageW(&mut m2, None, 0, 0, PM_REMOVE) }.as_bool() {
                        unsafe {
                            let _ = TranslateMessage(&m2);
                            DispatchMessageW(&m2);
                        }
                    }
                    unsafe {
                        MsgWaitForMultipleObjectsEx(None, 10, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                    }
                }
                return match d {
                    1 => {
                        let i = PICKER_INDEX.load(Ordering::SeqCst) as usize;
                        log::info!("picker: Enter no indice {i}");
                        PickerOutcome::Committed(i)
                    }
                    2 => {
                        log::info!("picker: Esc");
                        PickerOutcome::Cancelled
                    }
                    _ => {
                        log::info!("picker: fechado por uma tecla alheia (nao consumida)");
                        PickerOutcome::Cancelled
                    }
                };
            }
            if should_cancel() {
                log::info!("picker: cancelado de fora (atalho ou refine)");
                return PickerOutcome::Cancelled;
            }
            if last_activity.elapsed() >= PICKER_IDLE_TIMEOUT {
                log::info!("picker: fechado por inatividade");
                return PickerOutcome::Cancelled;
            }
            unsafe {
                MsgWaitForMultipleObjectsEx(None, 50, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            }
        }
    }
}

#[cfg(windows)]
pub use imp::gate;
#[cfg(windows)]
pub use imp::{run_picker_blocking, PickerGeom, PickerOutcome};
#[cfg(windows)]
#[allow(unused_imports)] // o tipo e parte do contrato publico, mesmo que so o flow o nomeie via inferencia
pub use imp::{spawn_esc_watcher, EscWatcher};

/// Non-Windows: sem hook, sem Esc-cancel (o atalho continua a cancelar). Mesmo contrato.
#[cfg(not(windows))]
pub struct EscWatcher;
#[cfg(not(windows))]
impl EscWatcher {
    pub fn stop_and_join(self) {}
}
#[cfg(not(windows))]
pub fn spawn_esc_watcher(_app: tauri::AppHandle) -> EscWatcher {
    EscWatcher
}

/// Non-Windows: sem hook, o picker nao tem teclado. Cancela sempre.
#[cfg(not(windows))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickerOutcome {
    Committed(usize),
    Cancelled,
}
#[cfg(not(windows))]
#[derive(Clone, Copy)]
pub struct PickerGeom {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub pad: i32,
    pub item_h: i32,
    pub visible: usize,
}
#[cfg(not(windows))]
pub fn run_picker_blocking(
    _len: usize,
    _initial: usize,
    _geom: PickerGeom,
    _should_cancel: impl Fn() -> bool,
    _on_move: impl Fn(usize),
) -> PickerOutcome {
    PickerOutcome::Cancelled
}

/// Non-Windows: nao ha hook. Ember e Windows-first; aqui degrada para o comportamento antigo
/// (cola direto), sem hook, sem descarte silencioso, sem meio-event-tap de macOS.
#[cfg(not(windows))]
pub async fn gate(_app: tauri::AppHandle) -> Decision {
    Decision::Accept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_and_esc_answer_the_question_and_never_reach_the_app() {
        assert_eq!(
            classify_key(0x0D),
            KeyVerdict::Decide { decision: Decision::Accept, consume: true }
        );
        assert_eq!(
            classify_key(0x1B),
            KeyVerdict::Decide { decision: Decision::Reject, consume: true }
        );
    }

    #[test]
    fn typing_on_means_keep_the_original_without_eating_the_keystroke() {
        // Quem continua a escrever ja respondeu: nao esta a olhar para o preview. Sem isto, a
        // pilula ficava pendurada ate ao timeout, no meio do ecra, a pedir uma resposta que ja
        // ninguem ia dar.
        //
        // `consume: false` e a parte que nao pode falhar: a tecla era dele, ia para a app dele.
        // Engolir-lhe um caracter para fechar uma pilula nossa seria trocar um incomodo por um
        // bug de escrita, que e muito pior.
        for vk in [0x41 /* A */, 0x20 /* Space */, 0x08 /* Backspace */, 0x09 /* Tab */] {
            assert_eq!(
                classify_key(vk),
                KeyVerdict::Decide { decision: Decision::Reject, consume: false },
                "vk {vk:#x} devia manter o original sem consumir a tecla"
            );
        }
    }

    #[test]
    fn watch_a_fresh_esc_cancels_and_is_consumed_with_its_tail() {
        // Esc fresco durante o refine: cancela. E dai em diante tudo o que for Esc e cauda da
        // NOSSA pressao (repeticoes, key-up) e continua consumido, senao vazava para a app.
        assert_eq!(classify_watch_event(0x1B, true, false, false), WatchVerdict::Cancel);
        assert_eq!(classify_watch_event(0x1B, true, false, true), WatchVerdict::ConsumeTail);
        assert_eq!(classify_watch_event(0x1B, false, false, true), WatchVerdict::ConsumeTail);
    }

    #[test]
    fn watch_an_esc_held_from_before_belongs_to_the_users_app() {
        // O Esc ja estava em baixo quando o watcher instalou: essa pressao era para a app dele.
        // As repeticoes passam, o key-up passa (e limpa o "herdado"), e so a descida SEGUINTE
        // conta como cancelamento.
        assert_eq!(classify_watch_event(0x1B, true, true, false), WatchVerdict::Pass);
        assert_eq!(classify_watch_event(0x1B, false, true, false), WatchVerdict::ReleaseHeld);
    }

    #[test]
    fn watch_ignores_every_other_key_entirely() {
        // O watcher e mais estreito que o gate: durante o refine o utilizador continua a
        // trabalhar, e NADA do que ele escreve nos diz respeito. Enter incluido.
        for vk in [0x0D, 0x41, 0x20, 0x25, 0x26, 0x10, 0x11] {
            assert_eq!(
                classify_watch_event(vk, true, false, false),
                WatchVerdict::Pass,
                "vk {vk:#x} nao e assunto do watcher"
            );
        }
    }

    #[test]
    fn modifiers_alone_do_not_dismiss_the_preview() {
        // Encostar ao Shift para escrever uma maiuscula, ou ao Ctrl a caminho de um atalho, nao e
        // "segui em frente". Se contasse, o preview fugia antes de a pessoa acabar o gesto.
        for vk in [0x10, 0x11, 0x12, 0x14, 0x5B, 0xA0, 0xA2, 0xA5] {
            assert_eq!(
                classify_key(vk),
                KeyVerdict::PassThrough,
                "vk {vk:#x} e um modificador e nao devia decidir nada"
            );
        }
    }
}
