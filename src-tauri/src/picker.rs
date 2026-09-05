//! O picker de projetos: atalho proprio -> lista pequena ao cursor -> setas escolhem, Enter
//! confirma. So a janela e a orquestracao vivem aqui; as teclas e a geometria sao puras
//! (`ember_core::projects`) e o hook e o modo picker do `preview_hook`.
//!
//! A regra sagrada do overlay vale por inteiro aqui: a janela NUNCA recebe foco (o paste do
//! refine seguinte tem de aterrar na app do utilizador) e nenhuma tecla que nao seja do picker
//! e consumida. O indice vive no Rust; o webview so desenha o que o evento lhe disser.

use std::sync::atomic::Ordering;

use ember_core::projects as core;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

const PICKER_EVENT: &str = "ember://picker";
/// Evento proprio para a posicao, separado do estado: com a lista colada ao ponteiro isto vai
/// dezenas de vezes por segundo, e mandar as linhas todas outra vez de cada vez era pagar a lista
/// inteira para mover dois numeros.
const PICKER_AT_EVENT: &str = "ember://picker-at";

/// Enquanto a combinacao do atalho esta premida, o Windows volta a entrega-la. Dentro desta
/// janela, uma repeticao e a MESMA pressao e nao um pedido de fechar.
const REOPEN_GRACE: std::time::Duration = std::time::Duration::from_millis(600);

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    /// `None` na primeira linha ("sem projeto").
    id: Option<String>,
    automatic: bool,
    name: String,
    /// Tom `mid` da paleta, pronto a usar; a UI nao conhece a paleta.
    color: String,
    icon: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PickerState {
    sequence: u64,
    rows: Vec<Row>,
    index: usize,
    /// `false` = esconder (a UI faz o fade-out).
    open: bool,
    /// Preenchido so no fecho por escolha: a UI usa-o para fechar com a linha escolhida a
    /// crescer, em vez de a lista desaparecer como se nada tivesse acontecido.
    chosen: Option<usize>,
}

/// A posicao da lista dentro da janela, em px CSS.
fn emit_state(app: &AppHandle, rows: &[Row], index: usize, open: bool, chosen: Option<usize>) {
    let state = app.state::<AppState>();
    let payload = serde_json::to_value(PickerState {
        sequence: state.event_seq.fetch_add(1, Ordering::SeqCst) + 1,
        rows: rows.to_vec(),
        index,
        open,
        chosen,
    })
    .expect("Picker state contains only serializable fields");
    if let Ok(mut slot) = state.picker_state.lock() {
        *slot = Some(payload.clone());
    }
    let _ = app.emit_to("picker", PICKER_EVENT, payload);
}

/// Abre o picker ao cursor. Chamado pelo atalho proprio; corre no runtime async.
pub async fn open_picker(app: AppHandle) {
    let state = app.state::<AppState>();
    // Durante um refine ha (ou vai haver) outro hook LL vivo: dois a consumir Enter ao mesmo
    // tempo e um perigo real, por isso o picker simplesmente nao abre. O atalho nao fica em
    // fila: quem quer o picker carrega outra vez depois.
    if state.is_busy() {
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
    // Segunda verificacao do refine, agora que `picker_open` ja esta publicado: se um comecou
    // entre a primeira e esta, e ele que manda (e o trabalho a serio) e a lista nem chega a
    // aparecer. Sem isto, a corrida so era estreita, nao inexistente.
    if state.is_busy() {
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
        automatic: false,
        name: "No project".into(),
        color: core::ACCENTS[0].mid.into(),
        icon: String::new(),
    }];
    rows.push(Row {
        id: None,
        automatic: true,
        name: "Auto: registered projects".into(),
        color: core::ACCENTS[0].mid.into(),
        icon: "sparkle".into(),
    });
    rows.extend(cfg.projects.iter().map(|p| Row {
        id: Some(p.id.clone()),
        automatic: false,
        name: p.name.clone(),
        color: core::resolve_accent(p).mid,
        icon: p.icon.clone(),
    }));
    let initial = cfg
        .active_project
        .as_deref()
        .and_then(|id| rows.iter().position(|r| r.id.as_deref() == Some(id)))
        .unwrap_or(usize::from(cfg.project_context));

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
    let surface = std::sync::Mutex::new(crate::floating::Surface::new(
        app.clone(),
        w.clone(),
        PICKER_AT_EVENT,
    ));
    if let Ok(mut follower) = surface.lock() {
        follower.follow();
    }
    let _ = w.set_always_on_top(true);
    let _ = w.set_ignore_cursor_events(true);
    emit_state(&app, &rows, initial, true, None);
    let _ = w.show();

    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let app2 = app.clone();
        let rows2 = rows.clone();
        std::thread::spawn(move || {
            let cancel_app = app2.clone();
            let on_follow = move |_: i32, _: i32| {
                if let Ok(mut follower) = surface.lock() {
                    follower.follow();
                }
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
    let mut outcome = rx
        .await
        .unwrap_or(crate::preview_hook::PickerOutcome::Cancelled);

    if let crate::preview_hook::PickerOutcome::Committed(i) = outcome {
        let escolhido = rows.get(i).and_then(|r| r.id.clone());
        let mut cfg = crate::config::load(&app);
        cfg.active_project = escolhido.clone();
        cfg.project_context = rows.get(i).is_some_and(|row| row.automatic);
        if let Err(e) = crate::config::save(&app, &cfg) {
            log::warn!("picker: configuration save failed: {e}");
            outcome = crate::preview_hook::PickerOutcome::Cancelled;
        }
        let cfg = crate::config::load(&app);
        crate::commands::refresh_orb_accent(&state, &cfg);
        log::debug!("picker: selection transaction completed");
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

#[tauri::command]
pub fn picker_snapshot(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<Option<serde_json::Value>, String> {
    if window.label() != "picker" {
        return Err("Picker only".into());
    }
    Ok(state
        .picker_state
        .lock()
        .map_err(|_| "Picker state unavailable")?
        .clone())
}
