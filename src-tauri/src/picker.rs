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
/// Evento proprio para a posicao, separado do estado: com a lista colada ao ponteiro isto vai
/// dezenas de vezes por segundo, e mandar as linhas todas outra vez de cada vez era pagar a lista
/// inteira para mover dois numeros.
const PICKER_AT_EVENT: &str = "ember://picker-at";

/// Offset do canto superior-esquerdo do picker em relacao ao cursor, em px fisicos. Abaixo e a
/// direita, como um menu de contexto: o cursor nunca fica em cima da primeira linha. A lista
/// mantem este offset enquanto o rato andar, portanto e tambem a distancia a que ela o segue.
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
    /// Preenchido so no fecho por escolha: a UI usa-o para fechar com a linha escolhida a
    /// crescer, em vez de a lista desaparecer como se nada tivesse acontecido.
    chosen: Option<usize>,
}

/// A posicao da lista dentro da janela, em px CSS.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PickerAt {
    x: f64,
    y: f64,
}

fn emit_state(app: &AppHandle, rows: &[Row], index: usize, open: bool, chosen: Option<usize>) {
    let _ = app.emit_to(
        "picker",
        PICKER_EVENT,
        PickerState {
            rows: rows.to_vec(),
            index,
            open,
            chosen,
        },
    );
}

fn emit_at(app: &AppHandle, (x, y): (f64, f64)) {
    let _ = app.emit_to("picker", PICKER_AT_EVENT, PickerAt { x, y });
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
    // A limpeza do sinal de fecho vem ANTES de publicar `picker_open`. Ao contrario, havia uma
    // janela em que um refine a arrancar via a lista como aberta, pedia-lhe que fechasse, e nos
    // apagavamos esse pedido logo a seguir: ficavam o hook do picker e o do refine vivos ao
    // mesmo tempo, os dois a comer o Enter. Na segunda pressao do atalho o pedido de fecho e
    // reposto mais abaixo, portanto limpar aqui nao perde nada.
    state.picker_cancel.store(false, Ordering::SeqCst);
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
    if let Ok(mut g) = state.picker_opened_at.lock() {
        *g = Some(std::time::Instant::now());
    }
    // Segunda verificacao do refine, agora que `picker_open` ja esta publicado: se um comecou
    // entre a primeira e esta, e ele que manda (e o trabalho a serio) e a lista nem chega a
    // aparecer. Sem isto, a corrida so era estreita, nao inexistente.
    if state.busy.load(Ordering::SeqCst) {
        state.picker_open.store(false, Ordering::SeqCst);
        log::info!("picker: ignorado (refine arrancou entretanto)");
        return;
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

    // A JANELA cobre o monitor inteiro e fica quieta; quem anda com o rato e a lista, la dentro,
    // por `transform`. O caminho obvio (arrastar a janela) foi tentado e range: a bolha tem
    // `backdrop-filter`, e mover uma janela com desfoque obriga o compositor a re-amostrar o fundo
    // a cada frame. E o mesmo aviso que ja estava escrito no CSS, para a bolha do preview.
    //
    // A lista tambem nao aponta linhas com o ponteiro: colada ao cursor, nenhuma linha fica
    // debaixo dele. Quem escolhe sao as setas, a roda e o Enter (ou o clique, que confirma).
    let (lw, lh) = core::picker_size(rows.len());
    let cursor = app.cursor_position().ok().map(|c| (c.x as i32, c.y as i32));
    // O monitor e a escala vem DO CURSOR e nao da janela: e onde a lista vai aparecer, e num
    // setup com dois DPIs a janela lembrava-se do ecra da vez anterior.
    let (area, scale) = match cursor {
        Some((x, y)) => crate::monitor_at_point(&w, x, y),
        None => (
            crate::monitor_at_point(&w, 0, 0).0,
            w.scale_factor().unwrap_or(1.0),
        ),
    };
    let _ = w.set_size(tauri::PhysicalSize::new(area.w as u32, area.h as u32));
    let _ = w.set_position(PhysicalPosition::new(area.x, area.y));
    let at = |c: (i32, i32), a: crate::geom::Rect, sc: f64| {
        core::picker_pill_pos(c, OFFSET, (a.x, a.y, a.w, a.h), sc, (lw, lh))
    };
    let pill = cursor.map(|c| at(c, area, scale)).unwrap_or((0.0, 0.0));
    let _ = w.set_always_on_top(true);
    // Click-through e SEM foco, como o overlay: o comentario do lib.rs:180 e a lei aqui tambem.
    let _ = w.set_ignore_cursor_events(true);
    emit_state(&app, &rows, initial, true, None);
    emit_at(&app, pill);
    let _ = w.show();
    log::info!(
        "picker: janela sobre o monitor ({}, {}) {}x{} escala {scale}; lista em ({:.0}, {:.0}) css",
        area.x,
        area.y,
        area.w,
        area.h,
        pill.0,
        pill.1
    );

    // O hook corre numa thread propria (o LL hook entrega na thread que instala e bombeia).
    // `on_move` chega do pump: re-emitir o estado inteiro e barato (<=25 linhas) e mais simples
    // do que um canal de deltas.
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let app2 = app.clone();
        let rows2 = rows.clone();
        let follow_app = app.clone();
        let follow_win = w.clone();
        // O ecra que a janela esta a cobrir agora. Serve para o caminho quente nao ter de
        // perguntar ao SO por monitores a cada movimento do rato: enquanto o ponteiro andar
        // dentro desta area, mover a lista e uma conta e um evento.
        let screen = std::sync::Arc::new(std::sync::Mutex::new((area, scale)));
        std::thread::spawn(move || {
            let cancel_app = app2.clone();
            // Seguir o cursor. O hook do rato so escreve onde ele esta; a conta e feita aqui, no
            // pump, e o resultado vai para a UI como dois numeros. Nao ha trabalho de janelas
            // neste caminho, e e por isso que ele e suave.
            let on_follow = move |x: i32, y: i32| {
                let (area, scale) = match screen.lock() {
                    Ok(g) => *g,
                    Err(_) => return,
                };
                if x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h {
                    emit_at(
                        &follow_app,
                        core::picker_pill_pos(
                            (x, y),
                            OFFSET,
                            (area.x, area.y, area.w, area.h),
                            scale,
                            (lw, lh),
                        ),
                    );
                    return;
                }
                // O ponteiro mudou de ecra, e a janela tem de o seguir. Isto e trabalho de
                // janelas, logo corre na thread principal, e acontece uma vez por travessia e nao
                // uma vez por movimento.
                let w = follow_win.clone();
                let app3 = follow_app.clone();
                let screen3 = screen.clone();
                let _ = follow_app.run_on_main_thread(move || {
                    let (a, sc) = crate::monitor_at_point(&w, x, y);
                    let _ = w.set_size(tauri::PhysicalSize::new(a.w as u32, a.h as u32));
                    let _ = w.set_position(PhysicalPosition::new(a.x, a.y));
                    if let Ok(mut g) = screen3.lock() {
                        *g = (a, sc);
                    }
                    emit_at(
                        &app3,
                        core::picker_pill_pos((x, y), OFFSET, (a.x, a.y, a.w, a.h), sc, (lw, lh)),
                    );
                    log::debug!("picker: a lista mudou para o monitor ({}, {})", a.x, a.y);
                });
            };
            let outcome = crate::preview_hook::run_picker_blocking(
                rows2.len(),
                initial,
                move || {
                    cancel_app
                        .state::<AppState>()
                        .picker_cancel
                        .load(Ordering::SeqCst)
                },
                |i| emit_state(&app2, &rows2, i, true, None),
                on_follow,
            );
            let _ = tx.send(outcome);
        });
    }
    let outcome = rx
        .await
        .unwrap_or(crate::preview_hook::PickerOutcome::Cancelled);

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

    // Fecho: a UI anima e a janela esconde-se logo a seguir. Esconder sem avisar dava um corte
    // seco; avisar sem esconder deixava uma janela invisivel a apanhar nada. A escolha leva mais
    // tempo do que a desistencia de proposito: e a confirmacao de que a lista fez alguma coisa, e
    // e o unico sitio onde ela aparece (a janela nao tem foco, nao ha toast, nao ha nada).
    let escolhido = match outcome {
        crate::preview_hook::PickerOutcome::Committed(i) => Some(i),
        _ => None,
    };
    emit_state(&app, &rows, escolhido.unwrap_or(0), false, escolhido);
    let espera = if escolhido.is_some() { 380 } else { 140 };
    tokio::time::sleep(std::time::Duration::from_millis(espera)).await;
    let _ = w.hide();
    state.picker_open.store(false, Ordering::SeqCst);
}
