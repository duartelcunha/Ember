//! Gate de aprovacao "preview before paste": depois de refinar, espera que o utilizador
//! aprove (Enter) ou recuse (Esc) antes de colar. A captura das teclas usa um low-level
//! keyboard hook do Windows (WH_KEYBOARD_LL) que CONSOME so o Enter/Esc durante o gate: assim
//! essas teclas nao vazam para a app em foco (o Enter nao mete newline no editor) e a overlay
//! nao precisa de roubar foco (a invariante sagrada: o paste aterra na app do utilizador).
//!
//! O `unsafe` do Win32 vive todo aqui, isolado. As pecas puras (`classify_key`, `Decision`,
//! `PREVIEW_TIMEOUT`) sao cross-platform e testadas em qualquer SO.

#[cfg(windows)]
use ember_core::input::{
    classify_key, classify_watch_event, KeyVerdict, WatchVerdict, PREVIEW_TIMEOUT,
};
pub use ember_core::input::{Decision, PickerOutcome};

// Capture, application and keyboard hooks share one native input owner.
static INPUT_OWNER: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) fn input_lease() -> std::sync::MutexGuard<'static, ()> {
    INPUT_OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(windows)]
mod imp {
    use super::{classify_key, Decision, KeyVerdict, PickerOutcome, PREVIEW_TIMEOUT};
    use std::sync::atomic::{AtomicU8, Ordering};
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, MsgWaitForMultipleObjectsEx, PeekMessageW,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, HHOOK,
        KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, MWMO_INPUTAVAILABLE,
        PM_REMOVE, QS_ALLINPUT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
        WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_SYSKEYDOWN,
        WM_SYSKEYUP, WM_XBUTTONDOWN,
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
    static OWNED: AtomicU8 = AtomicU8::new(0);
    static PAGE_DELTA: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

    const IGN_ENTER: u8 = 1;
    const IGN_ESC: u8 = 2;

    fn ignore_bit(vk: u32) -> u8 {
        match vk {
            0x0D => IGN_ENTER,
            0x1B => IGN_ESC,
            0x21 => 4,
            0x22 => 8,
            _ => 0,
        }
    }

    unsafe extern "system" fn ll_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let msg = wparam.0 as u32;
            let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let vk = kb.vkCode;
            let bit = ignore_bit(vk);
            if matches!(vk, 0x21 | 0x22) {
                let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
                let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
                if up {
                    IGNORE_HELD.fetch_and(!bit, Ordering::SeqCst);
                    if OWNED.load(Ordering::SeqCst) & bit != 0 {
                        RELEASED.fetch_or(bit, Ordering::SeqCst);
                        OWNED.fetch_and(!bit, Ordering::SeqCst);
                        return LRESULT(1);
                    }
                }
                if down && OWNED.load(Ordering::SeqCst) & bit != 0 {
                    RELEASED.fetch_and(!bit, Ordering::SeqCst);
                    if HOOK_DECISION.load(Ordering::SeqCst) != 0 {
                        return LRESULT(1);
                    }
                }
                if down
                    && IGNORE_HELD.load(Ordering::SeqCst) & bit == 0
                    && HOOK_DECISION.load(Ordering::SeqCst) == 0
                {
                    OWNED.fetch_or(bit, Ordering::SeqCst);
                    RELEASED.fetch_and(!bit, Ordering::SeqCst);
                    PAGE_DELTA.fetch_add(if vk == 0x22 { 1 } else { -1 }, Ordering::SeqCst);
                    return LRESULT(1);
                }
                return CallNextHookEx(None, code, wparam, lparam);
            }

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
                    if OWNED.load(Ordering::SeqCst) & bit != 0 {
                        RELEASED.fetch_or(bit, Ordering::SeqCst);
                        OWNED.fetch_and(!bit, Ordering::SeqCst);
                        return LRESULT(1);
                    }
                }
                if is_down {
                    // Uma descida FRESCA apaga o "ja subiu" da pressao anterior desta mesma
                    // tecla. Sem isto o `RELEASED` era um trinco: num toque duplo rapido, o
                    // drain via o bit da PRIMEIRA subida, dava a tecla por largada e o hook caia
                    // com a segunda pressao ainda em baixo, despejando o auto-repeat na app.
                    if OWNED.load(Ordering::SeqCst) & bit != 0 {
                        RELEASED.fetch_and(!bit, Ordering::SeqCst);
                        return LRESULT(1);
                    }
                    if !ignoring && HOOK_DECISION.load(Ordering::SeqCst) == 0 {
                        if let KeyVerdict::Decide { decision, .. } = classify_key(vk) {
                            if HOOK_DECISION
                                .compare_exchange(
                                    0,
                                    if decision == Decision::Accept { 1 } else { 2 },
                                    Ordering::SeqCst,
                                    Ordering::SeqCst,
                                )
                                .is_ok()
                            {
                                OWNED.fetch_or(bit, Ordering::SeqCst);
                                RELEASED.fetch_and(!bit, Ordering::SeqCst);
                                return LRESULT(1);
                            }
                        }
                    }
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    // Owned input tails cannot expire while a key is still held. Other input keeps passing
    // through the pump; the next interaction waits for this owner to retire.
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
        while RELEASED.load(Ordering::SeqCst) & bit != bit {
            let mut msg = MSG::default();
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
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
    pub fn run_gate_blocking(should_cancel: impl Fn() -> bool, on_page: impl Fn(i32)) -> Decision {
        let _input_owner = super::input_lease();
        if should_cancel() {
            return Decision::Reject;
        }
        HOOK_DECISION.store(0, Ordering::SeqCst);
        OWNED.store(0, Ordering::SeqCst);
        PAGE_DELTA.store(0, Ordering::SeqCst);
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
        for (vk, bit) in [(0x21, 4), (0x22, 8)] {
            if unsafe { GetAsyncKeyState(vk) as u16 & 0x8000 != 0 } {
                held |= bit;
            }
        }
        IGNORE_HELD.store(held, Ordering::SeqCst);

        log::debug!("gate: starting");
        let hmod = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
        let hook = match unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_proc), Some(HINSTANCE(hmod.0)), 0)
        } {
            Ok(h) => h,
            // Missing confirmation must never authorize replacement.
            Err(e) => {
                log::warn!("gate: hook installation failed ({e}); application rejected");
                return Decision::Reject;
            }
        };
        let _guard = HookGuard(hook);
        let mouse = unsafe {
            SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(input_watch_mouse_proc),
                Some(HINSTANCE(hmod.0)),
                0,
            )
        };
        let Ok(mouse) = mouse else {
            return Decision::Reject;
        };
        let _mouse_guard = HookGuard(mouse);

        let mut start = std::time::Instant::now();

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
            let page_delta = PAGE_DELTA.swap(0, Ordering::SeqCst);
            if page_delta != 0 {
                on_page(page_delta);
                start = std::time::Instant::now();
            }
            match HOOK_DECISION.load(Ordering::SeqCst) {
                1 => {
                    log::info!("gate: ACCEPT (Enter consumed by hook)");
                    drain_until_released(OWNED.load(Ordering::SeqCst));
                    return Decision::Accept;
                }
                2 => {
                    log::info!("gate: REJECT (Esc consumed by hook)");
                    if OWNED.load(Ordering::SeqCst) != 0 {
                        drain_until_released(OWNED.load(Ordering::SeqCst));
                    }
                    return Decision::Reject;
                }
                _ => {}
            }
            // 3) Cancel externo (hotkey durante o preview) -> recusa.
            if should_cancel() {
                log::info!("gate: REJECT (cancelled)");
                drain_until_released(OWNED.load(Ordering::SeqCst));
                return Decision::Reject;
            }
            // 4) Prazo total -> recusa (nunca colar sem aprovacao explicita).
            if start.elapsed() >= PREVIEW_TIMEOUT {
                log::info!("gate: REJECT (timeout, no key seen)");
                drain_until_released(OWNED.load(Ordering::SeqCst));
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
    pub async fn gate(app: tauri::AppHandle, run_id: u64) -> Decision {
        use tauri::Manager;
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let d = run_gate_blocking(
                || app.state::<crate::state::AppState>().dismissed(run_id),
                |delta| crate::flow::move_preview_page(&app, run_id, delta),
            );
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
            if kb.flags.contains(LLKHF_INJECTED) || WATCH_RELEASED.load(Ordering::SeqCst) != 0 {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            // Continued editing invalidates this run, but the user's input always passes through.
            if is_down
                && kb.vkCode != 0x1B
                && matches!(classify_key(kb.vkCode), KeyVerdict::Decide { .. })
            {
                let _ = WATCH_DECIDED.compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst);
            }
            if WATCH_DECIDED.load(Ordering::SeqCst) == 2 {
                return CallNextHookEx(None, code, wparam, lparam);
            }
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

    unsafe extern "system" fn input_watch_mouse_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code == HC_ACTION as i32 {
            let event = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            if event.flags & LLMHF_INJECTED == 0
                && matches!(
                    wparam.0 as u32,
                    WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
                )
            {
                let _ = WATCH_DECIDED.compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst);
                let _ = HOOK_DECISION.compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst);
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
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    /// Instala o watcher numa thread propria (o LL hook entrega na thread que instala e bombeia)
    /// e devolve o handle. Ao apanhar um Esc fresco aciona o caminho QUE JA EXISTE
    /// (`request_dismiss` + `cancel_notify`): zero logica nova.
    ///
    /// Nota sobre o que "dispensar" quer dizer desde 2026-09: o Esc tira a espera do caminho do
    /// utilizador, mas NAO mata a chamada ao modelo. Ela segue ate ao fim numa tarefa propria e o
    /// refinado fica guardado. Antes o Esc largava o future a meio: o provider cobrava na mesma e
    /// o resultado ia para o lixo, portanto carregar em Esc custava exatamente o mesmo que
    /// esperar, sem se ficar com nada.
    pub fn spawn_esc_watcher(app: tauri::AppHandle, run_id: u64) -> EscWatcher {
        use tauri::Manager;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        let join = std::thread::spawn(move || {
            let _input_owner = super::input_lease();
            if stop2.load(Ordering::SeqCst) {
                return;
            }
            WATCH_DECIDED.store(0, Ordering::SeqCst);
            WATCH_RELEASED.store(0, Ordering::SeqCst);
            // Um Esc ja em baixo na instalacao e da app do utilizador, nao nosso: fica marcado
            // para as suas repeticoes e o seu key-up passarem, e so a proxima descida contar.
            let held = unsafe { (GetAsyncKeyState(0x1B) as u16 & 0x8000) != 0 };
            WATCH_IGNORE_HELD.store(u8::from(held), Ordering::SeqCst);

            let hmod = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
            let hook = match unsafe {
                SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(esc_watch_proc),
                    Some(HINSTANCE(hmod.0)),
                    0,
                )
            } {
                Ok(h) => h,
                Err(e) => {
                    // Sem hook nao ha Esc-cancel neste ciclo; o atalho continua a cancelar.
                    // Nunca falhar o refine por causa de observabilidade de teclado.
                    log::warn!(
                        "input-watch: hook install failed ({e}); automatic application cancelled"
                    );
                    app.state::<crate::state::AppState>()
                        .request_dismiss(run_id);
                    return;
                }
            };
            let _guard = HookGuard(hook);
            let mouse = unsafe {
                SetWindowsHookExW(
                    WH_MOUSE_LL,
                    Some(input_watch_mouse_proc),
                    Some(HINSTANCE(hmod.0)),
                    0,
                )
            };
            let Ok(mouse) = mouse else {
                app.state::<crate::state::AppState>()
                    .request_dismiss(run_id);
                return;
            };
            let _mouse_guard = HookGuard(mouse);

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
                    log::info!("[run {run_id}] input changed; automatic application cancelled");
                    app.state::<crate::state::AppState>()
                        .request_dismiss(run_id);
                }
                // Depois de decidir, o hook so vive para engolir a cauda da pressao; visto o
                // key-up, ja nao ha nada a proteger.
                let released = WATCH_RELEASED.load(Ordering::SeqCst) != 0
                    || WATCH_DECIDED.load(Ordering::SeqCst) == 2;
                if notified && released {
                    break;
                }
                // O `stop` chega do flow assim que o refine acaba, e o refine acaba precisamente
                // porque este Esc o cancelou: nesse instante a tecla costuma estar AINDA em
                // baixo. Sair aqui largava o hook e despejava a cauda (auto-repeat e key-up) na
                // app do utilizador, que e a regra que este ficheiro inteiro existe para
                // respeitar. Fica-se ate ao key-up, com teto para uma tecla presa nao pendurar
                // a thread.
                if stop2.load(Ordering::SeqCst) && (!notified || released) {
                    break;
                }
                unsafe {
                    MsgWaitForMultipleObjectsEx(None, 50, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                }
            }
        });
        EscWatcher {
            stop,
            join: Some(join),
        }
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
    /// Um clique nosso ja foi consumido e falta engolir o `button up` que vem a seguir. Sem isto
    /// a app por baixo recebia metade do clique.
    static PICKER_CLICK_TAIL: AtomicU8 = AtomicU8::new(0);
    /// Onde o ponteiro esta, para o pump levar a lista atras dele. Como em todo o resto deste
    /// hook, o callback so escreve atomicos: mexer numa janela dali dentro poria trabalho de
    /// janelas dentro do orcamento do LowLevelHooksTimeout, e o Windows despeja hooks lentos.
    static PICKER_CURSOR: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    static PICKER_FOLLOW: AtomicU8 = AtomicU8::new(0);

    /// Empacota um ponto em dois inteiros.
    fn pack(a: i32, b: i32) -> i64 {
        ((a as i64) << 32) | (b as u32 as i64)
    }
    fn unpack(v: i64) -> (i32, i32) {
        ((v >> 32) as i32, v as u32 as i32)
    }

    unsafe extern "system" fn picker_mouse_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code == HC_ACTION as i32 {
            let msg = wparam.0 as u32;
            let ms = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            // Depois de decidido, o rato deixa de mandar: a lista esta a fechar e o que vier a
            // seguir e da app por baixo.
            let a_decidir = PICKER_DECISION.load(Ordering::SeqCst) == 0;
            match msg {
                // A lista anda COM o ponteiro. Por isso e que ele nao escolhe linhas: uma janela
                // colada ao cursor nunca deixa nenhuma linha ficar debaixo dele. Nunca consome o
                // movimento; mexer o rato e trabalho da pessoa, nao nosso.
                WM_MOUSEMOVE => {
                    if a_decidir {
                        PICKER_CURSOR.store(pack(ms.pt.x, ms.pt.y), Ordering::SeqCst);
                        PICKER_FOLLOW.store(1, Ordering::SeqCst);
                    }
                }
                // Rodar percorre a lista, como as setas: com a lista colada ao cursor, a mao ja
                // esta no rato. Consome, pela mesma razao que as setas: sem isso a pagina por
                // baixo rolava ao mesmo tempo que o menu.
                WM_MOUSEWHEEL => {
                    if a_decidir {
                        // O delta vem na metade alta do `mouseData`, com sinal: para cima e
                        // positivo, e para cima na lista e um indice para tras.
                        let delta = (ms.mouseData >> 16) as u16 as i16;
                        if delta != 0 {
                            let passo = if delta > 0 { -1 } else { 1 };
                            let novo = ember_core::projects::move_index(
                                PICKER_INDEX.load(Ordering::SeqCst) as usize,
                                passo,
                                PICKER_LEN.load(Ordering::SeqCst) as usize,
                            );
                            PICKER_INDEX.store(novo as u8, Ordering::SeqCst);
                            PICKER_MOVED.store(1, Ordering::SeqCst);
                        }
                        return LRESULT(1);
                    }
                }
                WM_LBUTTONDOWN => {
                    if a_decidir {
                        // Clicar confirma a linha escolhida, seja onde for o clique: a lista esta
                        // debaixo do ponteiro e nao ha "fora" nenhum para acertar. Consome, senao
                        // o clique ia parar a app por baixo (a janela e click-through de
                        // proposito) e a escolha vinha com um efeito secundario a reboque.
                        let _ = PICKER_DECISION.compare_exchange(
                            0,
                            1,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        );
                        PICKER_CLICK_TAIL.store(1, Ordering::SeqCst);
                        return LRESULT(1);
                    }
                }
                WM_LBUTTONUP if PICKER_CLICK_TAIL.swap(0, Ordering::SeqCst) != 0 => {
                    return LRESULT(1);
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

    /// Um menu esquecido sai da frente sozinho, mas a contagem e de INATIVIDADE e nao de vida:
    /// medida desde a abertura, seis segundos matavam a lista enquanto a pessoa ainda a estava a
    /// ler pela primeira vez. Cada seta que ela carrega poe o relogio a zero.
    const PICKER_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

    /// Corre o picker (bloqueante, numa thread com pump). `on_move` e chamado no PUMP (nunca no
    /// callback) com o indice novo, para o shell emitir o evento a UI.
    /// Bombeia mensagens, com o hook AINDA INSTALADO, ate as teclas que consumimos subirem.
    ///
    /// Vale para TODAS as saidas do picker e nao so para a da decisao: fechar por Esc externo ou
    /// por inatividade com uma seta ainda premida despejava o auto-repeat dela na app, e o caret
    /// da pessoa andava sozinho depois de o menu desaparecer.
    fn drain_picker_held() {
        while PICKER_HELD.load(Ordering::SeqCst) != 0
            || PICKER_CLICK_TAIL.load(Ordering::SeqCst) != 0
        {
            let mut m = MSG::default();
            while unsafe { PeekMessageW(&mut m, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&m);
                    DispatchMessageW(&m);
                }
            }
            unsafe {
                MsgWaitForMultipleObjectsEx(None, 10, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run_picker_blocking(
        len: usize,
        initial: usize,
        should_cancel: impl Fn() -> bool,
        on_move: impl Fn(usize),
        on_follow: impl Fn(i32, i32),
    ) -> PickerOutcome {
        let _input_owner = super::input_lease();
        if should_cancel() {
            return PickerOutcome::Cancelled;
        }
        PICKER_DECISION.store(0, Ordering::SeqCst);
        PICKER_HELD.store(0, Ordering::SeqCst);
        PICKER_MOVED.store(0, Ordering::SeqCst);
        PICKER_CLICK_TAIL.store(0, Ordering::SeqCst);
        PICKER_LEN.store(len.min(u8::MAX as usize) as u8, Ordering::SeqCst);
        PICKER_INDEX.store(initial.min(len.saturating_sub(1)) as u8, Ordering::SeqCst);
        PICKER_FOLLOW.store(0, Ordering::SeqCst);
        let hmod = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
        let hook = match unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(picker_proc),
                Some(HINSTANCE(hmod.0)),
                0,
            )
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
            SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(picker_mouse_proc),
                Some(HINSTANCE(hmod.0)),
                0,
            )
        };
        let _mouse_guard = match mouse_hook {
            Ok(h) => Some(HookGuard(h)),
            Err(e) => {
                log::warn!("picker: mouse hook install failed ({e}); so teclado");
                None
            }
        };
        let mut last_activity = std::time::Instant::now();
        let mut last_follow = std::time::Instant::now();
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
                on_move(PICKER_INDEX.load(Ordering::SeqCst) as usize);
            }
            if PICKER_FOLLOW.swap(0, Ordering::SeqCst) != 0
                || last_follow.elapsed() >= std::time::Duration::from_millis(500)
            {
                last_follow = std::time::Instant::now();
                // De proposito SEM tocar no `last_activity`: mexer o rato e o que a pessoa faz o
                // dia inteiro, e um menu que se mantivesse aberto por causa disso ficava la para
                // sempre. Quem o esquecer ve-o fechar; quem o estiver a usar carrega numa tecla,
                // roda ou clica, e isso conta.
                let (x, y) = unpack(PICKER_CURSOR.load(Ordering::SeqCst));
                on_follow(x, y);
            }
            let d = PICKER_DECISION.load(Ordering::SeqCst);
            if d != 0 {
                // Antes de largar o hook, espera que as teclas consumidas subam: largar com uma
                // seta (ou o Enter) ainda em baixo despejava o auto-repeat na app.
                drain_picker_held();
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
                drain_picker_held();
                return PickerOutcome::Cancelled;
            }
            if last_activity.elapsed() >= PICKER_IDLE_TIMEOUT {
                log::info!("picker: fechado por inatividade");
                drain_picker_held();
                return PickerOutcome::Cancelled;
            }
            unsafe {
                // Curto porque a lista SEGUE o ponteiro: a cada volta deste ciclo ela avanca um
                // salto atras dele, e a 50ms (20 saltos por segundo) o movimento saia aos
                // solavancos. A volta em si e um PeekMessage e uns atomicos.
                MsgWaitForMultipleObjectsEx(None, 8, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            }
        }
    }
}

#[cfg(windows)]
pub use imp::gate;
#[cfg(windows)]
pub use imp::run_picker_blocking;
#[cfg(windows)]
#[allow(unused_imports)]
// o tipo e parte do contrato publico, mesmo que so o flow o nomeie via inferencia
pub use imp::{spawn_esc_watcher, EscWatcher};

/// Non-Windows: sem hook, sem Esc-cancel (o atalho continua a cancelar). Mesmo contrato.
#[cfg(not(windows))]
pub struct EscWatcher;
#[cfg(not(windows))]
impl EscWatcher {
    pub fn stop_and_join(self) {}
}
#[cfg(not(windows))]
pub fn spawn_esc_watcher(_app: tauri::AppHandle, _run_id: u64) -> EscWatcher {
    EscWatcher
}

/// Non-Windows: sem hook, o picker nao tem teclado. Cancela sempre.
#[cfg(not(windows))]
pub fn run_picker_blocking(
    _len: usize,
    _initial: usize,
    _should_cancel: impl Fn() -> bool,
    _on_move: impl Fn(usize),
    _on_follow: impl Fn(i32, i32),
) -> PickerOutcome {
    PickerOutcome::Cancelled
}

/// Non-Windows: nao ha hook. Ember e Windows-first; aqui degrada para o comportamento antigo
/// (cola direto), sem hook, sem descarte silencioso, sem meio-event-tap de macOS.
#[cfg(not(windows))]
pub async fn gate(_app: tauri::AppHandle, _run_id: u64) -> Decision {
    Decision::Reject
}
