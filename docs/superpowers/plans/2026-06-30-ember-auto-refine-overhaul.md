# Ember Auto-Refine In-Place + Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Ember invisible and instant: press the hotkey with text selected in any app, an orb loads next to the cursor, and the selection is refined (Gemini→Claude) and replaced in-place automatically; plus a new abstract logo, a full English UI, a premium Settings restyle, and a typo sweep.

**Architecture:** The pure capture/replace sequencing (clipboard-sentinel technique) lives in `ember-core` behind a `SelectionIo` trait, fully unit-tested with fakes. The `src-tauri` shell provides `RealIo` (enigo + arboard) and orchestrates the hotkey → capture → refine → replace flow on the existing resilient refine pipeline. The frontend overlay is rewritten to render cursor-anchored states (orb / success / error / hint); Settings is restyled and translated to English.

**Tech Stack:** Rust, Tauri v2, enigo (input simulation), arboard (clipboard), React 19 + Vite + Tailwind v4 + Motion, Phosphor icons.

---

## File Structure

**ember-core (pure, no OS deps):**
- Create `crates/ember-core/src/selection.rs` — `SelectionIo` trait, `Captured`, `capture`, `replace`, `restore`, `clamp_pos`; unit tests with fakes.
- Modify `crates/ember-core/src/lib.rs` — `pub mod selection;`.

**src-tauri (shell / I/O):**
- Create `src-tauri/src/selection.rs` — `RealIo` (arboard + enigo) implementing `ember_core::selection::SelectionIo`; `cursor_xy()`; timing constants.
- Create `src-tauri/src/flow.rs` — `run_refine_flow()` and `refine_text()` orchestration.
- Modify `src-tauri/src/lib.rs` — hotkey rewrite, `show_orb_at_cursor`/`hide_orb`, English tray, register new modules, drop manual/preview commands from the handler.
- Modify `src-tauri/src/commands.rs` — remove `submit_manual`/`retry_refinement`/`accept_refinement`/`reject_refinement`/`copy_refinement`; English `friendly_error`.
- Modify `src-tauri/src/state.rs` — remove `Pending`.
- Modify `src-tauri/Cargo.toml` — add `enigo`, `arboard`.
- Modify `src-tauri/tauri.conf.json` — overlay window → small (260x100).
- Modify `src-tauri/capabilities/overlay.json` — drop clipboard-manager perm.

**Frontend:**
- Modify `src/overlay/types.ts` — phases `refining|success|error|hint|hidden`; trim controller.
- Modify `src/overlay/useOverlayController.ts` — reduce to a state listener.
- Modify `src/overlay/Overlay.tsx` — render Orb / Pill per phase.
- Create `src/overlay/Pill.tsx` — small glass pill for hint/error/success.
- Delete `src/overlay/Bubble.tsx`.
- Create `src/components/Logo.tsx` — "Ember arc" SVG mark.
- Modify `src/settings/Settings.tsx` — English + restyle + Logo.
- Modify `src/styles/globals.css` — pill styles.

**Icons:**
- Render Logo → 1024px PNG → `npm run tauri icon`.

---

## Task 1: Pure selection sequencing in ember-core (TDD)

**Files:**
- Create: `crates/ember-core/src/selection.rs`
- Modify: `crates/ember-core/src/lib.rs`
- Test: inline `#[cfg(test)]` in `selection.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/ember-core/src/selection.rs`:

```rust
//! Sequenciamento puro de captura/substituicao de seleccao (clipboard-sentinel).
//! Sem SO nem rede: o I/O real (enigo/arboard) vive no shell src-tauri.

/// Abstrai o I/O necessario para capturar/substituir a seleccao.
pub trait SelectionIo {
    fn clip_get(&mut self) -> Option<String>;
    fn clip_set(&mut self, s: &str);
    /// Liberta modificadores fisicos do hotkey (Ctrl/Shift/Alt) antes de simular.
    fn release_modifiers(&mut self);
    fn send_copy(&mut self);
    fn send_paste(&mut self);
    fn sleep_ms(&mut self, ms: u64);
}

/// Resultado da captura: `text` = seleccao (None se nada selecionado);
/// `saved` = clipboard original a restaurar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    pub text: Option<String>,
    pub saved: Option<String>,
}

/// Captura a seleccao sem destruir o clipboard: guarda o original, escreve um
/// sentinela, simula Ctrl+C e faz poll. Se o clipboard continuar = sentinela,
/// nada foi selecionado (`text == None`).
pub fn capture(io: &mut impl SelectionIo, sentinel: &str, polls: u32, step_ms: u64) -> Captured {
    let saved = io.clip_get();
    io.release_modifiers();
    io.sleep_ms(step_ms);
    io.clip_set(sentinel);
    io.send_copy();
    let mut text = None;
    for _ in 0..polls {
        io.sleep_ms(step_ms);
        match io.clip_get() {
            Some(t) if t != sentinel => {
                text = Some(t);
                break;
            }
            _ => {}
        }
    }
    Captured { text, saved }
}

/// Substitui a seleccao: poe o refinado no clipboard, simula Ctrl+V, espera o
/// paste assentar e restaura o clipboard original.
pub fn replace(io: &mut impl SelectionIo, refined: &str, saved: &Option<String>, settle_ms: u64) {
    io.clip_set(refined);
    io.send_paste();
    io.sleep_ms(settle_ms);
    restore(io, saved);
}

/// Restaura o clipboard original (best-effort: so texto).
pub fn restore(io: &mut impl SelectionIo, saved: &Option<String>) {
    if let Some(s) = saved {
        io.clip_set(s);
    }
}

/// Clampa a posicao (x,y) de uma janela wxh a uma area de trabalho, para o orb
/// nunca sair do ecra.
pub fn clamp_pos(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    area_x: i32,
    area_y: i32,
    area_w: i32,
    area_h: i32,
) -> (i32, i32) {
    let max_x = area_x + (area_w - w).max(0);
    let max_y = area_y + (area_h - h).max(0);
    (x.clamp(area_x, max_x), y.clamp(area_y, max_y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeIo {
        clipboard: Option<String>,
        /// O que o "SO" copiaria com Ctrl+C (None = nada selecionado).
        selection: Option<String>,
        pasted: Option<String>,
    }

    impl SelectionIo for FakeIo {
        fn clip_get(&mut self) -> Option<String> {
            self.clipboard.clone()
        }
        fn clip_set(&mut self, s: &str) {
            self.clipboard = Some(s.to_string());
        }
        fn release_modifiers(&mut self) {}
        fn send_copy(&mut self) {
            if let Some(sel) = &self.selection {
                self.clipboard = Some(sel.clone());
            }
        }
        fn send_paste(&mut self) {
            self.pasted = self.clipboard.clone();
        }
        fn sleep_ms(&mut self, _ms: u64) {}
    }

    const SENT: &str = "__ember_sentinel__";

    #[test]
    fn captures_selected_text() {
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            selection: Some("hello world".into()),
            ..Default::default()
        };
        let c = capture(&mut io, SENT, 5, 1);
        assert_eq!(c.text, Some("hello world".into()));
        assert_eq!(c.saved, Some("old".into()));
    }

    #[test]
    fn empty_when_nothing_selected() {
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            selection: None,
            ..Default::default()
        };
        let c = capture(&mut io, SENT, 5, 1);
        assert_eq!(c.text, None);
        assert_eq!(c.saved, Some("old".into()));
    }

    #[test]
    fn replace_pastes_refined_and_restores_clipboard() {
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            selection: Some("hi".into()),
            ..Default::default()
        };
        let c = capture(&mut io, SENT, 5, 1);
        replace(&mut io, "REFINED", &c.saved, 1);
        assert_eq!(io.pasted, Some("REFINED".into()));
        assert_eq!(io.clipboard, Some("old".into()));
    }

    #[test]
    fn restore_with_none_saved_does_not_panic() {
        let mut io = FakeIo {
            clipboard: Some(SENT.into()),
            ..Default::default()
        };
        restore(&mut io, &None);
        // clipboard inalterado (best-effort): nao rebenta.
        assert_eq!(io.clipboard, Some(SENT.into()));
    }

    #[test]
    fn clamp_keeps_window_on_screen() {
        // cursor perto do canto inferior-direito: a janela e empurrada para dentro.
        assert_eq!(clamp_pos(1910, 1070, 260, 100, 0, 0, 1920, 1080), (1660, 980));
        // dentro: inalterado.
        assert_eq!(clamp_pos(100, 100, 260, 100, 0, 0, 1920, 1080), (100, 100));
    }
}
```

- [ ] **Step 2: Register the module**

Modify `crates/ember-core/src/lib.rs` — add alongside the other `pub mod` lines:

```rust
pub mod selection;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p ember-core selection`
Expected: 5 tests pass (`captures_selected_text`, `empty_when_nothing_selected`, `replace_pastes_refined_and_restores_clipboard`, `restore_with_none_saved_does_not_panic`, `clamp_keeps_window_on_screen`).

- [ ] **Step 4: Commit (skip if not a git repo)**

```bash
git add crates/ember-core/src/selection.rs crates/ember-core/src/lib.rs
git commit -m "feat(core): pure clipboard-sentinel selection sequencing"
```

---

## Task 2: RealIo (enigo + arboard) in src-tauri

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/selection.rs`

- [ ] **Step 1: Add dependencies**

Modify `src-tauri/Cargo.toml` — under `[dependencies]`, replace the trailing comment line with:

```toml
# Adapters nativos do loop in-place.
enigo = "0.3"
arboard = "3"
```

- [ ] **Step 2: Implement RealIo + cursor**

Create `src-tauri/src/selection.rs`:

```rust
//! I/O real da captura/substituicao: enigo (input) + arboard (clipboard).
//! A logica pura vive em `ember_core::selection`.

use ember_core::selection::SelectionIo;
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Mouse, Settings,
};

/// Sentinela unico escrito no clipboard para detetar "nada selecionado".
pub const SENTINEL: &str = "\u{200b}__ember_capture_sentinel__\u{200b}";
pub const POLLS: u32 = 30;
pub const STEP_MS: u64 = 10;
pub const PASTE_SETTLE_MS: u64 = 90;

pub struct RealIo {
    clip: arboard::Clipboard,
    enigo: Enigo,
}

impl RealIo {
    pub fn new() -> Result<Self, String> {
        let clip = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
        Ok(Self { clip, enigo })
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
        let _ = self.enigo.key(Key::Control, Press);
        let _ = self.enigo.key(Key::Unicode('c'), Click);
        let _ = self.enigo.key(Key::Control, Release);
    }
    fn send_paste(&mut self) {
        let _ = self.enigo.key(Key::Control, Press);
        let _ = self.enigo.key(Key::Unicode('v'), Click);
        let _ = self.enigo.key(Key::Control, Release);
    }
    fn sleep_ms(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

/// Posicao global do cursor (pixels fisicos). None se indisponivel.
pub fn cursor_xy() -> Option<(i32, i32)> {
    let enigo = Enigo::new(&Settings::default()).ok()?;
    enigo.location().ok()
}
```

- [ ] **Step 3: Register the module**

Modify `src-tauri/src/lib.rs` — add near the other `mod` declarations:

```rust
mod selection;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p ember`
Expected: compiles. If enigo's API differs (version drift), check current enigo docs via context7 (`resolve-library-id` → `query-docs` "enigo keyboard key Direction") and adjust the `Key`/`Direction` calls; the trait method shapes (`clip_get`, etc.) stay the same.

- [ ] **Step 5: Commit (skip if not a git repo)**

```bash
git add src-tauri/Cargo.toml src-tauri/src/selection.rs src-tauri/src/lib.rs
git commit -m "feat(shell): RealIo (enigo+arboard) + cursor position"
```

---

## Task 3: Orchestration flow (capture → refine → replace)

**Files:**
- Create: `src-tauri/src/flow.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Slim down state**

Modify `src-tauri/src/state.rs` — remove the `Pending` struct and the `pending` field. Result:

```rust
//! Estado partilhado da app (managed state do Tauri).

use reqwest::Client;
use std::time::Duration;

pub struct AppState {
    /// Um unico `reqwest::Client` partilhado (pool de conexoes interno).
    pub http: Client,
}

impl AppState {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { http }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Reduce commands.rs to settings + a shared refine helper**

Modify `src-tauri/src/commands.rs`:

1. Remove the `Fluxo de refinamento` section's command functions: `submit_manual`, `retry_refinement`, `accept_refinement`, `reject_refinement`, `copy_refinement`, the `do_refine` fn, the `StatePayload`/`emit_state` block, and the now-unused imports (`Pending`, `ClipboardExt`, `Emitter`, `State` if unused).
2. Translate `friendly_error` to English and make it `pub(crate)`.
3. Add a `pub(crate)` `refine_text` helper that returns the refined text + provider name (used by `flow.rs`).

Replace the entire `Fluxo de refinamento` section (from `fn friendly_error` to end of file) with:

```rust
// ---------------------------------------------------------------------------------------
// Refine helper (chamado pelo loop nativo em flow.rs)
// ---------------------------------------------------------------------------------------

pub(crate) fn friendly_error(e: &ember_core::CoreError) -> String {
    use ember_core::CoreError::*;
    match e {
        NoProvidersConfigured => "No API key set. Opening settings…".into(),
        Auth => "Invalid API key. Check settings.".into(),
        ContentPolicy => "Blocked by the provider's content policy.".into(),
        AllProvidersFailed => "Providers failed (network or limits). Try again.".into(),
        _ => "Couldn't refine. Try again.".into(),
    }
}

/// Refina `input` com a chain Gemini->Claude. Devolve (texto, provider) ou CoreError.
pub(crate) async fn refine_text(
    app: &AppHandle,
    state: &AppState,
    input: &str,
) -> Result<(String, String), ember_core::CoreError> {
    use ember_core::model::Provider;
    use ember_core::prompt::build_llm_request;
    use ember_core::retry::RetryConfig;

    let cfg = config::load(app);
    let mut chain: Vec<(Provider, String)> = Vec::new();
    if let Some(k) = secrets::get(Provider::Gemini) {
        chain.push((Provider::Gemini, k));
    }
    if let Some(k) = secrets::get(Provider::Claude) {
        chain.push((Provider::Claude, k));
    }
    if chain.is_empty() {
        return Err(ember_core::CoreError::NoProvidersConfigured);
    }

    let resolved = profile::resolve(app, cfg.profile_override.as_deref(), cfg.ignore_claude_md);
    let req = build_llm_request(input, &resolved.profile, &cfg.gemini_model, cfg.mode);
    let rcfg = RetryConfig {
        provider_count: chain.len(),
        ..RetryConfig::default()
    };
    let resp = providers::refine(
        &state.http,
        &rcfg,
        &chain,
        &req,
        &cfg.gemini_model,
        &cfg.claude_model,
    )
    .await?;
    Ok((resp.text, resp.provider.display_name().to_string()))
}
```

Keep the top imports needed by the settings commands; ensure `use ember_core::model::{ProfileSource, Provider};` and `use crate::{config, profile, providers, secrets};` remain, and `use crate::state::AppState;` (drop `Pending`).

- [ ] **Step 3: Write the flow orchestration**

Create `src-tauri/src/flow.rs`:

```rust
//! Loop nativo: hotkey -> orb no cursor -> capturar seleccao -> refinar -> substituir.

use tauri::{AppHandle, Emitter, Manager};

use crate::selection::{self, RealIo, PASTE_SETTLE_MS, POLLS, SENTINEL, STEP_MS};
use crate::state::AppState;
use crate::{commands, show_settings};
use ember_core::selection as seq;

const STATE_EVENT: &str = "ember://state";

fn emit(app: &AppHandle, phase: &str, message: Option<String>, provider: Option<String>) {
    let _ = app.emit_to(
        "overlay",
        STATE_EVENT,
        serde_json::json!({ "phase": phase, "message": message, "provider": provider }),
    );
}

/// Bloqueante: cria RealIo, captura a seleccao, devolve (texto, clipboard_original).
fn blocking_capture() -> Result<seq::Captured, String> {
    let mut io = RealIo::new()?;
    Ok(seq::capture(&mut io, SENTINEL, POLLS, STEP_MS))
}

/// Bloqueante: substitui a seleccao pelo refinado e restaura o clipboard.
fn blocking_replace(refined: String, saved: Option<String>) -> Result<(), String> {
    let mut io = RealIo::new()?;
    seq::replace(&mut io, &refined, &saved, PASTE_SETTLE_MS);
    Ok(())
}

/// Bloqueante: restaura o clipboard original (ramos de erro/hint).
fn blocking_restore(saved: Option<String>) -> Result<(), String> {
    let mut io = RealIo::new()?;
    seq::restore(&mut io, &saved);
    Ok(())
}

/// Orquestra todo o fluxo. Chamado a partir do callback do hotkey.
pub async fn run(app: AppHandle) {
    emit(&app, "refining", None, None);

    let captured = match tauri::async_runtime::spawn_blocking(blocking_capture).await {
        Ok(Ok(c)) => c,
        _ => {
            emit(&app, "error", Some("Couldn't read the selection.".into()), None);
            hide_after(&app, 1400).await;
            return;
        }
    };

    let saved = captured.saved.clone();

    let Some(selected) = captured.text else {
        // Nada selecionado: restaura clipboard, hint subtil.
        let s = saved.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || blocking_restore(s)).await;
        emit(&app, "hint", Some("Select text first".into()), None);
        hide_after(&app, 1400).await;
        return;
    };

    let state = app.state::<AppState>();
    match commands::refine_text(&app, &state, &selected).await {
        Ok((refined, provider)) => {
            let s = saved.clone();
            let r = refined.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || blocking_replace(r, s)).await;
            emit(&app, "success", None, Some(provider));
            hide_after(&app, 650).await;
        }
        Err(e) => {
            let s = saved.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || blocking_restore(s)).await;
            let msg = commands::friendly_error(&e);
            if matches!(e, ember_core::CoreError::NoProvidersConfigured) {
                show_settings(&app);
            }
            emit(&app, "error", Some(msg), None);
            hide_after(&app, 1600).await;
        }
    }
}

async fn hide_after(app: &AppHandle, ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    crate::hide_orb(app);
}
```

- [ ] **Step 4: Rewrite lib.rs hotkey + window helpers + English tray**

Modify `src-tauri/src/lib.rs`:

1. Add `mod flow;` and `mod selection;` with the other modules.
2. Replace `show_overlay_manual` / `hide_overlay` with `show_orb_at_cursor` / `hide_orb`, and make `show_settings` `pub(crate)`.
3. Change `register_hotkey` to spawn `flow::run`.
4. Translate the tray labels to English.

Concrete replacements:

```rust
// (substitui show_overlay_manual e hide_overlay)

/// Posiciona o orb junto ao cursor (sem roubar foco) e mostra-o.
pub(crate) fn show_orb_at_cursor(app: &AppHandle) {
    let Some(w) = show_window(app, "overlay") else { return };
    let _ = w.set_always_on_top(true);
    if let Some((cx, cy)) = selection::cursor_xy() {
        let (ww, wh) = match w.outer_size() {
            Ok(s) => (s.width as i32, s.height as i32),
            Err(_) => (260, 100),
        };
        let (ax, ay, aw, ah) = monitor_work_area(&w);
        let (x, y) = ember_core::selection::clamp_pos(cx + 16, cy + 18, ww, wh, ax, ay, aw, ah);
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
    }
    let _ = w.show();
    // NB: nao chamamos set_focus — o paste tem de ir para a app em foco.
}

pub(crate) fn hide_orb(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
}

fn monitor_work_area(w: &WebviewWindow) -> (i32, i32, i32, i32) {
    if let Ok(Some(mon)) = w.current_monitor() {
        let p = mon.position();
        let s = mon.size();
        (p.x, p.y, s.width as i32, s.height as i32)
    } else {
        (0, 0, 1920, 1080)
    }
}
```

`register_hotkey` body becomes:

```rust
pub(crate) fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.on_shortcut(hotkey, move |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            show_orb_at_cursor(app);
            let app = app.clone();
            tauri::async_runtime::spawn(async move { flow::run(app).await });
        }
    })
    .map_err(|e| e.to_string())
}
```

Tray labels → English in `build_tray`:

```rust
    let open = MenuItemBuilder::with_id("open_settings", "Settings").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
```

Make `show_settings` `pub(crate)`:

```rust
pub(crate) fn show_settings(app: &AppHandle) {
```

Remove the `Emitter` import if `lib.rs` no longer emits directly, and remove the now-unused `STATE_EVENT`/`emit_to` usage in `show_overlay_manual` (deleted). Update the `invoke_handler!` macro: remove `commands::submit_manual`, `commands::retry_refinement`, `commands::accept_refinement`, `commands::reject_refinement`, `commands::copy_refinement` (keep all settings commands).

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p ember`
Expected: compiles with no errors (warnings about unused imports are acceptable; clean them).

- [ ] **Step 6: Commit (skip if not a git repo)**

```bash
git add src-tauri/src/flow.rs src-tauri/src/commands.rs src-tauri/src/state.rs src-tauri/src/lib.rs
git commit -m "feat(shell): auto-refine-in-place flow on hotkey"
```

---

## Task 4: Overlay window + capabilities

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/capabilities/overlay.json`

- [ ] **Step 1: Shrink the overlay window**

Modify `src-tauri/tauri.conf.json` — the `overlay` window object: set `"width": 260, "height": 100` (keep `create:false`, `transparent:true`, `decorations:false`, `alwaysOnTop:true`, `skipTaskbar:true`, `shadow:false`, `focus:false`, `resizable:false`, `visible:false`).

- [ ] **Step 2: Trim overlay capability**

Modify `src-tauri/capabilities/overlay.json` — remove `"clipboard-manager:default"` (clipboard now via arboard). Result permissions: `["core:default", "global-shortcut:default", "positioner:default"]`.

- [ ] **Step 3: Verify config parses**

Run: `cargo build -p ember`
Expected: compiles (tauri-build re-reads the config; no permission-resolution errors).

- [ ] **Step 4: Commit (skip if not a git repo)**

```bash
git add src-tauri/tauri.conf.json src-tauri/capabilities/overlay.json
git commit -m "chore(shell): small cursor overlay window + trimmed caps"
```

---

## Task 5: Frontend overlay rewrite (cursor-anchored states)

**Files:**
- Modify: `src/overlay/types.ts`
- Modify: `src/overlay/useOverlayController.ts`
- Modify: `src/overlay/Overlay.tsx`
- Create: `src/overlay/Pill.tsx`
- Modify: `src/styles/globals.css`
- Delete: `src/overlay/Bubble.tsx`

- [ ] **Step 1: New state contract**

Replace `src/overlay/types.ts` with:

```ts
/** Estado do overlay junto ao cursor. */

export type OverlayPhase = "hidden" | "refining" | "success" | "error" | "hint";

export interface OverlayState {
  phase: OverlayPhase;
  /** Mensagem (fase error/hint). */
  message?: string | null;
  /** Provider usado ("Gemini"/"Claude"), fase success. */
  provider?: string | null;
}

/** Evento emitido pelo nucleo Rust com o novo estado do overlay. */
export const STATE_EVENT = "ember://state";
```

- [ ] **Step 2: Reduce the controller to a listener**

Replace `src/overlay/useOverlayController.ts` with:

```ts
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { STATE_EVENT, type OverlayState } from "./types";

/** Ouve o evento de estado do nucleo Rust. Sem accoes: o fluxo e automatico. */
export function useOverlayState(): OverlayState {
  const [state, setState] = useState<OverlayState>({ phase: "hidden" });
  useEffect(() => {
    const unlisten = listen<OverlayState>(STATE_EVENT, (e) => setState(e.payload));
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);
  return state;
}
```

- [ ] **Step 3: Create the Pill component**

Create `src/overlay/Pill.tsx`:

```tsx
import { m } from "motion/react";
import { WarningCircle, Cursor, Check } from "@phosphor-icons/react";

type Kind = "error" | "hint" | "success";

const ICON = {
  error: <WarningCircle weight="fill" size={16} />,
  hint: <Cursor weight="fill" size={16} />,
  success: <Check weight="bold" size={16} />,
};

/** Pilha de feedback junto ao cursor (erro/hint/sucesso). */
export function Pill({ kind, text }: { kind: Kind; text: string }) {
  const color =
    kind === "error"
      ? "var(--color-error)"
      : kind === "success"
        ? "var(--color-success)"
        : "var(--color-fg-muted)";
  return (
    <m.div
      layoutId="refiner-surface"
      className="ember-bubble flex items-center gap-2 px-3 py-2"
      style={{ borderRadius: 14, color }}
      initial={{ opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, scale: 0.92 }}
    >
      <span className="shrink-0" style={{ color }}>
        {ICON[kind]}
      </span>
      <span className="text-xs text-fg">{text}</span>
    </m.div>
  );
}
```

- [ ] **Step 4: Rewrite Overlay.tsx**

Replace `src/overlay/Overlay.tsx` with:

```tsx
import { AnimatePresence, domAnimation, LazyMotion, MotionConfig } from "motion/react";
import { useOverlayState } from "./useOverlayController";
import { Orb } from "./Orb";
import { Pill } from "./Pill";

/** Raiz do overlay junto ao cursor: orb (refining) ou pilha (success/error/hint). */
export function Overlay() {
  const s = useOverlayState();
  return (
    <LazyMotion features={domAnimation} strict>
      <MotionConfig reducedMotion="user">
        <div className="grid h-screen place-items-center p-2">
          <AnimatePresence mode="popLayout">
            {s.phase === "refining" && <Orb key="orb" />}
            {s.phase === "success" && <Pill key="ok" kind="success" text="Refined" />}
            {s.phase === "error" && (
              <Pill key="err" kind="error" text={s.message ?? "Something went wrong."} />
            )}
            {s.phase === "hint" && (
              <Pill key="hint" kind="hint" text={s.message ?? "Select text first"} />
            )}
          </AnimatePresence>
        </div>
      </MotionConfig>
    </LazyMotion>
  );
}
```

- [ ] **Step 5: Delete the old Bubble**

Run: `rm "src/overlay/Bubble.tsx"`

- [ ] **Step 6: Verify the build (typecheck)**

Run: `npm run build`
Expected: `tsc` passes (no references to deleted `Bubble`/old controller/types), vite builds overlay + settings bundles.

- [ ] **Step 7: Commit (skip if not a git repo)**

```bash
git add src/overlay src/styles/globals.css
git commit -m "feat(overlay): cursor-anchored orb/success/error/hint states"
```

---

## Task 6: "Ember arc" logo component

**Files:**
- Create: `src/components/Logo.tsx`

- [ ] **Step 1: Create the SVG mark**

Create `src/components/Logo.tsx`:

```tsx
/** "Ember arc": core com glow + arco orbital offset. Mark abstrato da marca. */
export function Logo({ size = 32 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" fill="none" aria-label="Ember">
      <defs>
        <radialGradient id="ember-core" cx="38%" cy="32%" r="75%">
          <stop offset="0%" stopColor="#ffe6be" />
          <stop offset="35%" stopColor="#ff9a3d" />
          <stop offset="70%" stopColor="#ff6a00" />
          <stop offset="100%" stopColor="#d9510a" />
        </radialGradient>
        <linearGradient id="ember-arc" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="#ffb066" />
          <stop offset="100%" stopColor="#ff6a00" stopOpacity="0.2" />
        </linearGradient>
        <filter id="ember-glow" x="-50%" y="-50%" width="200%" height="200%">
          <feGaussianBlur stdDeviation="2.2" result="b" />
          <feMerge>
            <feMergeNode in="b" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>
      <path
        d="M50 16 A22 22 0 1 1 20 13"
        stroke="url(#ember-arc)"
        strokeWidth="4"
        strokeLinecap="round"
        filter="url(#ember-glow)"
      />
      <circle cx="32" cy="34" r="12.5" fill="url(#ember-core)" filter="url(#ember-glow)" />
    </svg>
  );
}
```

- [ ] **Step 2: Verify it renders (live)**

The dev server is running; import `Logo` in `Settings.tsx` (Task 7) and check the header in the Settings window. Adjust arc sweep/offset/stroke live until it reads abstract and premium.

- [ ] **Step 3: Commit (skip if not a git repo)**

```bash
git add src/components/Logo.tsx
git commit -m "feat(ui): Ember arc logo mark"
```

---

## Task 7: Settings in English + premium restyle

**Files:**
- Modify: `src/settings/Settings.tsx`

- [ ] **Step 1: Translate + restyle + use Logo**

Modify `src/settings/Settings.tsx`:

1. Header: replace the `.ember-orb` div with `<Logo size={34} />`; title stays "Ember"; subtitle → `"Refine your prompts in the moment, in any app."`.
2. Tabs: `Providers`, `Shortcut`, `Profile`, `Appearance`, `About`.
3. Providers blurb → English: `"BYOK: bring your own keys. Gemini is primary; Claude is the fallback (different families fail for different reasons). Keys live in the Windows Credential Manager, never in plain text."`.
4. `ProviderConfig` titles/subtitles → `"Gemini (primary)"` / `"Fast, with a generous free tier."` and `"Claude (fallback)"` / `"Optional. Kicks in when Gemini fails, or for max quality."`; labels `"API key"`, placeholder `saved ? "•••••••• (saved)" : "paste your key"`, button `"Save"`, `"Model"`.
5. Toasts → English: `` `${title} key is valid and saved.` `` / `` `${title} key saved, but validation failed.` `` / `"Couldn't save the key (app not running?)."` / `` `${title} model updated.` ``.
6. Shortcut tab: section `"Global shortcut"` / `"The combo that summons Ember in any app."`, button `"Apply"`, toasts `"Shortcut updated."` / `"Couldn't apply the shortcut."`; section `"Startup"` / `"Launch Ember automatically with Windows."`, label `"Start with Windows"`.
7. Profile tab: `sourceLabel` → `{ claude_md: "auto-detected from CLAUDE.md", user_edited: "edited by you", default: "built-in quality profile" }`; section `"Personalization profile"` / `` `Current source: ${sourceLabel[...]}.` ``; placeholder `"Your style and tone preferences (language, rules like 'no em-dashes'…)."`; buttons `"Save"`, `"Reload from CLAUDE.md"`, `"Reset to default"`; toasts `"Profile saved."` / `"Couldn't save."` / `"Reloaded from CLAUDE.md."` / `"Couldn't reload."` / `"Reset to default."` / `"Couldn't reset."`.
8. Appearance tab: section `"Appearance"` / `"Premium dark theme. Respects the system's reduced-motion setting."`; body `"Ember uses a dark, glassy theme with orange as the accent. More theme options coming later."`.
9. About tab: section `"Ember 0.1.0"`; body `"In-the-moment prompt refiner for any app. Gemini primary + Claude fallback, guided by your profile. Built with Tauri."`.
10. Restyle pass: header `gap-3` with logo; increase container breathing (`py-12`), tab content spacing, card borders use `--border-subtle`; add a subtle fade-in on the main container: wrap content in a `motion` div with `initial={{opacity:0, y:6}} animate={{opacity:1, y:0}}`.

Add import: `import { Logo } from "@/components/Logo";` (and `import { m } from "motion/react";` if adding the fade).

- [ ] **Step 2: Verify (typecheck + live)**

Run: `npm run build`
Expected: `tsc` passes. Then check the live Settings window: all English, logo in header, tidy spacing.

- [ ] **Step 3: Commit (skip if not a git repo)**

```bash
git add src/settings/Settings.tsx
git commit -m "feat(settings): English copy + premium restyle + logo"
```

---

## Task 8: Copy/typo sweep + remaining strings

**Files:**
- Modify: any file with remaining user-facing PT strings (grep-driven)
- Modify: `README.md` (optional polish)

- [ ] **Step 1: Find remaining user-facing PT strings**

Run a grep for likely leftovers (toasts, placeholders, labels):

Use Grep for: `Guardar|Cancelar|Refinar|Definicoes|Sair|Aparencia|Atalho|Perfil|nao foi|Recarregar|Repor|cola a tua` across `src/` and `src-tauri/src/`.
Expected after Tasks 3/7: only code comments remain (PT comments are fine). Fix any user-facing string still in PT.

- [ ] **Step 2: Typo pass on new English copy**

Re-read the English strings added in Tasks 3 and 7; fix typos and awkward phrasing. Confirm the tray ("Settings"/"Quit"), error messages, and About text are clean.

- [ ] **Step 3: Verify**

Run: `npm run build` and `cargo build -p ember`
Expected: both succeed.

- [ ] **Step 4: Commit (skip if not a git repo)**

```bash
git add -A
git commit -m "chore: English copy + typo sweep"
```

---

## Task 9: Regenerate app/tray icons from the new logo

**Files:**
- Create (temp): `scripts/render-logo.mjs`
- Modify: `src-tauri/icons/*` (generated), `public/` favicon if present

- [ ] **Step 1: Add a one-off SVG→PNG renderer**

Add devDep: `npm i -D @resvg/resvg-js`.

Create `scripts/render-logo.mjs` that writes the same "Ember arc" SVG (mirror of `Logo.tsx`, on a transparent 1024×1024 canvas with padding) and renders `logo-1024.png`:

```js
import { Resvg } from "@resvg/resvg-js";
import { writeFileSync } from "node:fs";

const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 64 64">
  <defs>
    <radialGradient id="c" cx="38%" cy="32%" r="75%">
      <stop offset="0%" stop-color="#ffe6be"/><stop offset="35%" stop-color="#ff9a3d"/>
      <stop offset="70%" stop-color="#ff6a00"/><stop offset="100%" stop-color="#d9510a"/>
    </radialGradient>
    <linearGradient id="a" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#ffb066"/><stop offset="100%" stop-color="#ff6a00" stop-opacity="0.2"/>
    </linearGradient>
  </defs>
  <path d="M50 16 A22 22 0 1 1 20 13" stroke="url(#a)" stroke-width="4" stroke-linecap="round" fill="none"/>
  <circle cx="32" cy="34" r="12.5" fill="url(#c)"/>
</svg>`;

const png = new Resvg(svg, { fitTo: { mode: "width", value: 1024 } }).render().asPng();
writeFileSync("logo-1024.png", png);
console.log("wrote logo-1024.png");
```

- [ ] **Step 2: Render + regenerate icons**

Run: `node scripts/render-logo.mjs`
Then: `npm run tauri icon logo-1024.png`
Expected: `src-tauri/icons/` is regenerated (32x32.png, 128x128.png, 128x128@2x.png, icon.ico, icon.icns, plus Square/Store logos). The tray uses `default_window_icon()` → now the new mark.

- [ ] **Step 3: Verify in app**

Restart `npm run tauri dev`; confirm the tray icon and Settings window icon show the new mark.

- [ ] **Step 4: Commit (skip if not a git repo)**

```bash
git add scripts/render-logo.mjs package.json package-lock.json src-tauri/icons logo-1024.png
git commit -m "feat(brand): regenerate app/tray icons from Ember arc mark"
```

---

## Task 10: End-to-end manual verification (run skill)

**Files:** none (verification only)

- [ ] **Step 1: Launch**

Run: `npm run tauri dev` (already running with HMR; restart only if Rust changed and didn't auto-rebuild).
Expected: builds, tray icon appears, no panics after `Running ember.exe`.

- [ ] **Step 2: Happy path**

In Notepad (or any editor), type and select a short prompt (e.g. `make me a tweet about cats`). With a Gemini key set in Settings, press `Ctrl+Shift+Space`.
Expected: orb appears beside the cursor (loading), then the selected text is replaced in-place by the refined version; a brief "Refined" flash; clipboard unchanged afterward (paste elsewhere to confirm your prior clipboard is intact).

- [ ] **Step 3: No-selection path**

Click into an empty area (nothing selected), press the hotkey.
Expected: small "Select text first" pill near the cursor, then it fades. Clipboard intact.

- [ ] **Step 4: No-key path**

Clear keys in Settings (or test before adding any), select text, press the hotkey.
Expected: error pill "No API key set. Opening settings…" and the Settings window opens. Clipboard intact.

- [ ] **Step 5: Settings + brand check**

Open Settings from the tray; confirm: everything English, new logo in header and tray, tidy premium layout, no typos.

- [ ] **Step 6: Final report**

Summarize what works, any rough edges, and (only if non-trivial setup was needed) recommend `/run-skill-generator` to capture the launch as a project skill.

---

## Self-Review

**Spec coverage:** Auto-refine flow (Tasks 1-4) ✓; no-selection hint (Task 3 + 5) ✓; fully-automatic replace (Tasks 1,3) ✓; cursor-anchored orb (Tasks 3,4,5) ✓; clipboard save/restore incl. error branches (Tasks 1,3) ✓; modifier release + sentinel timing (Tasks 1,2) ✓; English UI incl. tray + errors (Tasks 3,7,8) ✓; premium Settings restyle (Task 7) ✓; Ember arc logo (Task 6) ✓; icon regeneration (Task 9) ✓; typo sweep (Task 8) ✓; tests for the pure sequencing (Task 1) ✓; manual verification (Task 10) ✓. Risk mitigations (focus, DPI/multimonitor clamp, AV) are covered by `focused(false)` + `clamp_pos` + the legitimate input pattern.

**Placeholder scan:** No TBD/TODO; every code step shows real code. The only deferred detail is live visual tuning of the logo/restyle, which is inherently iterative and has a concrete starting implementation.

**Type consistency:** `SelectionIo`/`Captured`/`capture`/`replace`/`restore`/`clamp_pos` names match across Tasks 1-3; `STATE_EVENT`/`OverlayState`/`OverlayPhase` consistent between `types.ts`, `useOverlayController.ts`, `Overlay.tsx`; `refine_text`/`friendly_error` defined in Task 3 commands.rs and consumed in flow.rs; `show_orb_at_cursor`/`hide_orb`/`show_settings` defined and called consistently.

**Risk note (enigo version):** the enigo API (`Key`, `Direction`, `Keyboard`, `Mouse` traits) is written for 0.2+; if the resolved version differs, verify via context7 and adjust call sites in `src-tauri/src/selection.rs` only.
