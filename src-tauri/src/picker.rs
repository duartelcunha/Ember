//! O picker de projetos: atalho proprio -> lista pequena ao cursor -> setas escolhem, Enter
//! confirma. So a janela e a orquestracao vivem aqui; as teclas e a geometria sao puras
//! (`ember_core::projects`) e o hook e o modo picker do `preview_hook`.
//!
//! A regra sagrada do overlay vale por inteiro aqui: a janela NUNCA recebe foco (o paste do
//! refine seguinte tem de aterrar na app do utilizador) e nenhuma tecla que nao seja do picker
//! e consumida. O indice vive no Rust; o webview so desenha o que o evento lhe disser.

use std::sync::atomic::Ordering;

use ember_core::projects as core;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

use crate::state::AppState;

const PICKER_EVENT: &str = "ember://picker";

/// Offset do canto superior-esquerdo do picker em relacao ao cursor, em px fisicos. Abaixo e a
/// direita, como um menu de contexto: o cursor nunca fica em cima da primeira linha.
const OFFSET: (i32, i32) = (14, 18);

/// Enquanto a combinacao do atalho esta premida, o Windows volta a entrega-la. Dentro desta
/// janela, uma repeticao e a MESMA pressao e nao um pedido de fechar.
const REOPEN_GRACE: std::time::Duration = std::time::Duration::from_millis(600);

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    /// `None` na primeira linha ("sem projeto").
    id: Option<String>,
    name: String,
    /// Tom `mid` da paleta, pronto a usar; a UI nao conhece a paleta.
    color: String,
    icon: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PickerState {
    rows: Vec<Row>,
    index: usize,
    /// `false` = esconder (a UI faz o fade-out).
    open: bool,
}

fn emit_state(app: &AppHandle, rows: &[Row], index: usize, open: bool) {
    let _ = app.emit_to(
        "picker",
        PICKER_EVENT,
        PickerState {
            rows: rows.to_vec(),
            index,
            open,
        },
    );
}

/// Abre o picker ao cursor. Chamado pelo atalho proprio; corre no runtime async.
pub async fn open_picker(app: AppHandle) {
    let state = app.state::<AppState>();
    // Durante um refine ha (ou vai haver) outro hook LL vivo: dois a consumir Enter ao mesmo
    // tempo e um perigo real, por isso o picker simplesmente nao abre. O atalho nao fica em
    // fila: quem quer o picker carrega outra vez depois.
    if state.busy.load(Ordering::SeqCst) {
        log::info!("picker: ignorado (refine a decorrer)");
        return;
    }
    // Segunda pressao do atalho com o picker aberto = fechar. O swap tambem serve de guarda de
    // reentrancia: nunca ha dois pickers.
    //
    // Mas so conta como segunda pressao passado o `REOPEN_GRACE`: enquanto a combinacao esta
    // fisicamente premida o Windows entrega o atalho repetido, e sem esta janela a lista fechava
    // no mesmo instante em que abria, o que se via como "o atalho nao faz nada".
    if state.picker_open.swap(true, Ordering::SeqCst) {
        let idade = state
            .picker_opened_at
            .lock()
            .ok()
            .and_then(|g| *g)
            .map(|t| t.elapsed());
        match idade {
            Some(d) if d < REOPEN_GRACE => {
                log::debug!("picker: atalho repetido {d:?} depois de abrir; ignorado");
            }
            _ => state.picker_cancel.store(true, Ordering::SeqCst),
        }
        return;
    }
    state.picker_cancel.store(false, Ordering::SeqCst);
    if let Ok(mut g) = state.picker_opened_at.lock() {
        *g = Some(std::time::Instant::now());
    }

    let cfg = crate::config::load(&app);
    if cfg.projects.is_empty() {
        state.picker_open.store(false, Ordering::SeqCst);
        log::info!("picker: sem projetos registados; nada a mostrar");
        return;
    }

    // Linha 0 e sempre "sem projeto": sticky sem saida de teclado obrigava a ir as settings so
    // para desligar, o que mata o proposito do atalho.
    let mut rows = vec![Row {
        id: None,
        name: "No project".into(),
        color: core::ACCENTS[0].mid.into(),
        icon: String::new(),
    }];
    rows.extend(cfg.projects.iter().map(|p| Row {
        id: Some(p.id.clone()),
        name: p.name.clone(),
        color: core::accent(p.accent).mid.into(),
        icon: p.icon.clone(),
    }));
    let initial = cfg
        .active_project
        .as_deref()
        .and_then(|id| rows.iter().position(|r| r.id.as_deref() == Some(id)))
        .unwrap_or(0);

    let Some(w) = crate::get_or_create_window(&app, "picker") else {
        state.picker_open.store(false, Ordering::SeqCst);
        return;
    };

    // Tamanho pela lista, posicao pelo cursor, UMA vez: um menu que segue o rato e inutilizavel.
    let (lw, lh) = core::picker_size(rows.len());
    let scale = w.scale_factor().unwrap_or(1.0);
    let _ = w.set_size(tauri::LogicalSize::new(lw, lh));
    let (pw, ph) = ((lw as f64 * scale) as i32, (lh as f64 * scale) as i32);
    let mut win = (0i32, 0i32);
    if let Ok(c) = app.cursor_position() {
        let (ax, ay, aw, ah) = crate::monitor_at_point(&w, c.x as i32, c.y as i32);
        let (x, y) = ember_core::selection::clamp_pos(
            c.x as i32 + OFFSET.0,
            c.y as i32 + OFFSET.1,
            pw,
            ph,
            ax,
            ay,
            aw,
            ah,
        );
        win = (x, y);
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
    let _ = w.set_always_on_top(true);
    // Click-through e SEM foco, como o overlay: o comentario do lib.rs:180 e a lei aqui tambem.
    let _ = w.set_ignore_cursor_events(true);
    emit_state(&app, &rows, initial, true);
    let _ = w.show();

    // Geometria em fisicos para o hook do rato saber que linha esta debaixo do ponteiro.
    //
    // Vem da posicao REAL da janela e nao da que pedimos: entre o `set_position` e o `show` o
    // gestor de janelas pode ter mexido, e uma geometria que discorda um pixel do que esta no
    // ecra faz os cliques cairem na linha errada, ou fora de todas. Se a janela ainda nao souber
    // dizer onde esta, fica a posicao pedida, que e o melhor palpite disponivel.
    let real = w.outer_position().map(|p| (p.x, p.y)).unwrap_or(win);
    let geom = crate::preview_hook::PickerGeom {
        x: real.0,
        y: real.1,
        w: pw,
        pad: (core::PICKER_PAD as f64 * scale) as i32,
        item_h: (core::PICKER_ITEM_H as f64 * scale) as i32,
        visible: rows.len().min(core::PICKER_MAX_VISIBLE),
    };
    log::info!(
        "picker: janela em ({}, {}) {}x{} fisicos (pedida ({}, {}), escala {scale}); linha {}px, pad {}px",
        geom.x,
        geom.y,
        pw,
        ph,
        win.0,
        win.1,
        geom.item_h,
        geom.pad
    );

    // O hook corre numa thread propria (o LL hook entrega na thread que instala e bombeia).
    // `on_move` chega do pump: re-emitir o estado inteiro e barato (<=25 linhas) e mais simples
    // do que um canal de deltas.
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let app2 = app.clone();
        let rows2 = rows.clone();
        std::thread::spawn(move || {
            let cancel_app = app2.clone();
            let outcome = crate::preview_hook::run_picker_blocking(
                rows2.len(),
                initial,
                geom,
                move || {
                    cancel_app
                        .state::<AppState>()
                        .picker_cancel
                        .load(Ordering::SeqCst)
                },
                |i| emit_state(&app2, &rows2, i, true),
            );
            let _ = tx.send(outcome);
        });
    }
    let outcome = rx.await.unwrap_or(crate::preview_hook::PickerOutcome::Cancelled);

    if let crate::preview_hook::PickerOutcome::Committed(i) = outcome {
        let escolhido = rows.get(i).and_then(|r| r.id.clone());
        let mut cfg = crate::config::load(&app);
        cfg.active_project = escolhido.clone();
        if let Err(e) = crate::config::save(&app, &cfg) {
            log::warn!("picker: nao consegui gravar o projeto ativo: {e}");
        }
        let cfg = crate::config::load(&app);
        crate::commands::refresh_orb_accent(&state, &cfg);
        log::info!(
            "picker: projeto ativo -> {}",
            rows.get(i).map(|r| r.name.as_str()).unwrap_or("?")
        );
    }

    // Fecho: a UI faz o fade curto e a janela esconde logo a seguir. Esconder sem avisar dava
    // um corte seco; avisar sem esconder deixava uma janela invisivel a apanhar nada.
    emit_state(&app, &rows, 0, false);
    tokio::time::sleep(std::time::Duration::from_millis(140)).await;
    let _ = w.hide();
    state.picker_open.store(false, Ordering::SeqCst);
}
