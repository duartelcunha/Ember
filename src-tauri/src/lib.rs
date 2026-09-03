// Evita a consola extra no Windows em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config;
mod flow;
mod foreground;
mod logging;
mod models_cache;
mod oauth;
mod picker;
mod preview_hook;
mod profile;
mod project;
mod projects;
mod prompt_log;
mod providers;
mod refine_store;
mod secrets;
mod selection;
mod state;

use std::sync::atomic::Ordering;

use ember_core::model::RefineMode;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::window::Color;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Medidas do overlay (faisca, pilula, padding, tamanho da janela) vivem em
/// `ember_core::overlay_geom::DEFAULT_LAYOUT`, com os testes de geometria ao lado delas. Estao
/// ESPELHADAS no frontend: `SPARK_SIZE` (Orb.tsx), `p-2` (Overlay.tsx), `ml-10` (Pill.tsx) e o
/// `width`/`height` da janela "overlay" (tauri.conf.json). Muda uma, muda a outra, senao a
/// orbita descentra-se do ponteiro.
use ember_core::overlay_geom::{self as geom, DEFAULT_LAYOUT as LAYOUT};

/// Um monitor como o SO o descreve: retangulo completo (para saber onde o cursor esta), area
/// util (para clampar sem meter a pilula por baixo da barra de tarefas) e a ESCALA DELE.
///
/// A escala e por monitor e nao da janela de proposito. Perguntar `w.scale_factor()` era o bug:
/// isso descreve o ecra onde a janela ESTA, e o Windows so a corrige no WM_DPICHANGED seguinte.
/// Durante a travessia, os offsets sairiam a escala do ecra anterior.
struct MonitorInfo {
    full: geom::Rect,
    work: geom::Rect,
    scale: f64,
}

fn monitors_of(w: &WebviewWindow) -> Vec<MonitorInfo> {
    let Ok(list) = w.available_monitors() else {
        return Vec::new();
    };
    list.iter()
        .map(|m| {
            let p = m.position();
            let s = m.size();
            let wa = m.work_area();
            MonitorInfo {
                full: geom::Rect::new(p.x, p.y, s.width as i32, s.height as i32),
                work: geom::Rect::new(
                    wa.position.x,
                    wa.position.y,
                    wa.size.width as i32,
                    wa.size.height as i32,
                ),
                scale: m.scale_factor(),
            }
        })
        .collect()
}

/// Monitor (area util) e escala a usar para um ponto, tipicamente o cursor.
///
/// A deteccao usa o retangulo COMPLETO (o cursor pode estar por cima da barra de tarefas) e o
/// clamp usa a area util. Quando o ponto nao cai em monitor nenhum - acontece a serio: com um
/// 1920x1080 encostado a um 2560x1440 o secundario comeca 87px mais abaixo, e a faixa acima dele
/// nao pertence a ecra nenhum - vai-se ao mais PROXIMO. Antes caia-se no monitor da JANELA, que
/// durante o seguimento e o de onde ela veio: o cursor passava para o outro ecra e a orb ficava
/// colada a fronteira do anterior, que e exatamente o "a orb nao passa para o segundo monitor".
pub(crate) fn monitor_at_point(w: &WebviewWindow, px: i32, py: i32) -> (geom::Rect, f64) {
    let mons = monitors_of(w);
    let rects: Vec<geom::Rect> = mons.iter().map(|m| m.full).collect();
    match geom::monitor_for_point(px, py, &rects) {
        Some((r, from_fallback)) => {
            let idx = rects.iter().position(|m| *m == r).unwrap_or(0);
            if from_fallback {
                warn_monitor_fallback(px, py, &rects, idx);
            }
            (mons[idx].work, mons[idx].scale)
        }
        // O SO nao soube listar monitores. Ultimo recurso: o monitor da janela, com a escala
        // dela. Nao ha melhor palpite, e sem isto a janela ficava sem clamp nenhum.
        None => {
            log::warn!("overlay: available_monitors() vazio; a usar o monitor da janela");
            let r = match w.current_monitor() {
                Ok(Some(m)) => {
                    let p = m.position();
                    let s = m.size();
                    geom::Rect::new(p.x, p.y, s.width as i32, s.height as i32)
                }
                _ => geom::Rect::new(0, 0, 1920, 1080),
            };
            (r, w.scale_factor().unwrap_or(1.0))
        }
    }
}

/// Avisa (no maximo uma vez por 2s, isto corre a 120fps) que o ponto nao caiu em monitor nenhum.
/// Antes este caminho era mudo e o sintoma so aparecia no ecra.
fn warn_monitor_fallback(px: i32, py: i32, rects: &[geom::Rect], chosen: usize) {
    use std::sync::atomic::AtomicU64;
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let prev = LAST.load(Ordering::Relaxed);
    if now.saturating_sub(prev) < 2000 {
        return;
    }
    LAST.store(now, Ordering::Relaxed);
    log::warn!(
        "overlay: ponto ({px}, {py}) fora de todos os monitores {rects:?}; usado o mais proximo (idx {chosen})"
    );
}

/// Obtem (ou cria) uma janela declarada com `create:false`. NAO a mostra (o caller decide
/// posicao/foco antes de `show`, para o orb nao piscar na posicao errada).
pub(crate) fn get_or_create_window(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(label) {
        return Some(w);
    }
    let cfg = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == label)
        .cloned()?;
    let w = WebviewWindowBuilder::from_config(app, &cfg)
        .ok()?
        .build()
        .ok()?;
    // Fecho da janela settings tratado NATIVAMENTE: o X (ou Alt+F4) esconde a janela em vez de
    // a destruir, para a app continuar na tray. Feito aqui no Rust, nao no JS: o onCloseRequested
    // do lado do webview e fragil (depende do webview estar vivo e responsivo, e deixava a janela
    // presa a preto quando falhava). O evento nativo nunca falha.
    if label == "settings" {
        // Pinta o fundo nativo com a cor do tema guardado ANTES da 1a exibicao: se o tema for
        // creme, a janela nao pisca o escuro do backgroundColor default antes de o CSS aplicar.
        let theme = config::load(app).theme;
        let _ = w.set_background_color(Some(theme_bg(&theme)));

        let win = w.clone();
        w.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = win.hide();
            }
        });
    }
    Some(w)
}

/// O que a overlay mostra agora, para a geometria saber que caixa manter visivel.
fn overlay_phase(app: &AppHandle) -> geom::Phase {
    let st = app.state::<state::AppState>();
    if st.orb_visible.load(Ordering::SeqCst) {
        geom::Phase::Orb {
            labels: st.orb_labels.load(Ordering::SeqCst),
        }
    } else {
        geom::Phase::Pill
    }
}

/// Top-left desejado da janela do overlay para o cursor atual, no monitor onde o cursor esta.
///
/// So resolve o CONTEXTO (onde esta o cursor, em que monitor, a que escala); a matematica e
/// pura e vive em `ember_core::overlay_geom`, com testes para o layout de dois ecras.
fn orb_target(app: &AppHandle, w: &WebviewWindow) -> Option<((i32, i32), f64)> {
    let c = app.cursor_position().ok()?;
    let (mon, scale) = monitor_at_point(w, c.x as i32, c.y as i32);
    Some((
        geom::overlay_geometry((c.x, c.y), mon, scale, overlay_phase(app), &LAYOUT),
        scale,
    ))
}

/// Posiciona o orb junto ao cursor (snap), mostra-o sem foco e arranca o loop de seguimento.
pub(crate) fn show_orb_at_cursor(app: &AppHandle) {
    let Some(w) = get_or_create_window(app, "overlay") else {
        return;
    };
    // Cada hotkey novo comeca sempre pelo orb: marca ja aqui (sincrono), antes do loop de
    // seguimento arrancar, para o primeiro frame nao usar a caixa de conteudo da pilula
    // que possa ter ficado de um ciclo anterior.
    {
        let st = app.state::<state::AppState>();
        st.orb_visible.store(true, Ordering::SeqCst);
        st.follow_cursor.store(true, Ordering::SeqCst);
    }
    let _ = w.set_always_on_top(true);
    // Transparente sobre outras apps: nunca intercetar cliques.
    let _ = w.set_ignore_cursor_events(true);
    if let Some(((x, y), scale)) = orb_target(app, &w) {
        // Tamanho ANTES da posicao: se o ciclo anterior acabou noutro monitor, a janela ainda
        // tem o tamanho fisico da escala de la, e um clamp contra o tamanho errado punha o orb
        // ao lado do ponteiro no primeiro frame.
        apply_scale(&w, scale);
        let _ = w.set_position(PhysicalPosition::new(x, y));
        log_overlay_placement(&w, (x, y), scale);
    }
    let _ = w.show();
    // NB: nao chamamos set_focus. O paste tem de aterrar na app em foco, nao na nossa.

    // Loop de seguimento: corre enquanto o orb estiver visivel, colado ao cursor.
    //
    // A geracao aposenta o ciclo anterior. Sem isto, com dois refines sobrepostos (a pilula de
    // um ainda no ecra, o outro ja a arrancar) ficavam dois loops vivos a disputar a mesma
    // janela a 120fps, cada um com a sua suavizacao.
    let gen = app
        .state::<state::AppState>()
        .follow_gen
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { orb_follow_loop(app2, gen).await });
}

/// Poe a janela com o tamanho fisico que a escala pede. Idempotente (o tao ja redimensiona no
/// WM_DPICHANGED); esta chamada existe porque o WebView2 numa janela transparente as vezes fica
/// com a superficie do tamanho antigo depois de mudar de DPI e a pintura sai cortada.
fn apply_scale(w: &WebviewWindow, scale: f64) {
    let (ew, eh) = geom::expected_window_physical(scale, &LAYOUT);
    if let Ok(cur) = w.outer_size() {
        if cur.width == ew && cur.height == eh {
            return;
        }
    }
    let _ = w.set_size(tauri::PhysicalSize::new(ew, eh));
}

/// Uma linha por exibicao com TUDO o que decide a posicao. Sem isto, um relato de "a orb nao
/// passa para o segundo monitor" nao tinha como ser diagnosticado a posteriori: o log nao tinha
/// uma unica linha de geometria.
fn log_overlay_placement(w: &WebviewWindow, target: (i32, i32), scale: f64) {
    let mons: Vec<String> = monitors_of(w)
        .iter()
        .map(|m| {
            format!(
                "[{},{} {}x{} @{}]",
                m.full.x, m.full.y, m.full.w, m.full.h, m.scale
            )
        })
        .collect();
    log::info!(
        "overlay: mostrada em {target:?} escala {scale} (janela {:?}, monitores {})",
        w.outer_size().map(|s| (s.width, s.height)).ok(),
        mons.join(" ")
    );
}

/// Segue o cursor com suavizacao exponencial (lerp) enquanto o orb esta visivel, para um
/// arrasto fluido tipo Apple em vez de saltos. Termina quando `hide_orb` esconde. Usa um
/// `interval` a 120fps (nao `sleep`, que acumula deriva). A suavizacao usa o dt REAL via
/// `alpha = 1 - exp(-dt/tau)`: assim mantem a mesma sensacao mesmo que um tick atrase (um
/// factor fixo por frame mudava de velocidade com o frame-rate, um bug subtil de engasgo).
async fn orb_follow_loop(app: AppHandle, gen: u64) {
    let Some(w) = app.get_webview_window("overlay") else {
        return;
    };
    let mut tick = tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / 120.0));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Constante de tempo da suavizacao da PILULA do preview (a faisca segue rigida, ver
    // abaixo): cobre ~63% da distancia ao alvo a cada tau segundos. A pilula tem texto para
    // ler; desliza atras do cursor e assenta assim que ele para. Era 0.22 e ficava atras de
    // mais; 0.12 chega ao cursor num terco do tempo sem perseguir aos saltos.
    const PILL_TAU: f64 = 0.12;
    let mut current: Option<(f64, f64)> = None;
    let mut last = tokio::time::Instant::now();
    // Escala do monitor onde o cursor estava no frame anterior, para detetar a travessia entre
    // ecras com DPI diferente (o unico momento em que a janela precisa de mudar de tamanho).
    let mut last_scale = w.scale_factor().unwrap_or(1.0);

    // Reacao da estrela ao movimento: emitimos o vetor de "puxao" (cursor - estrela) para o
    // overlay, que inclina/estica a estrela na direcao do movimento. ADAPTATIVO: numa maquina
    // que aguenta os 120fps emitimos a cada frame; se comeca a atrasar, baixamos para 60 e depois
    // 30fps (menos IPC), medido pelo tempo REAL de frame suavizado. So o ritmo de emissao muda;
    // o seguimento da janela mantem-se sempre a 120fps.

    // A janela ja foi vista visivel alguma vez neste ciclo?
    //
    // Isto existe por causa de uma corrida REAL, e nao por cautela. O `show()` e chamado da
    // thread do atalho global, e o Tauri despacha-o para a thread principal: quando este ciclo
    // corre a primeira volta, a janela pode ainda nao estar visivel. A versao anterior fazia
    // `break` nesse caso, e o resultado era o pior possivel: o seguimento morria ANTES do
    // primeiro frame e a pilula ficava parada no sitio onde nasceu, para o resto do ciclo.
    // Enquanto nunca foi visivel, esperamos; so depois de a ter visto e que "invisivel" quer
    // mesmo dizer "acabou".
    let mut seen_visible = false;
    // Teto para essa espera: se a janela nunca aparecer (falha a mostrar), nao ficamos com um
    // ciclo a 120fps para sempre.
    let started = tokio::time::Instant::now();
    const SHOW_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

    loop {
        // Nasceu um ciclo de seguimento mais novo: a janela e dele.
        if app
            .state::<state::AppState>()
            .follow_gen
            .load(Ordering::SeqCst)
            != gen
        {
            return;
        }
        match w.is_visible() {
            Ok(true) => seen_visible = true,
            _ if seen_visible => break,
            _ if started.elapsed() > SHOW_GRACE => {
                log::warn!("overlay: nunca ficou visivel em {SHOW_GRACE:?}; seguimento desiste");
                break;
            }
            _ => {
                // Ainda a aparecer. Salta o frame sem desistir do ciclo.
                tick.tick().await;
                continue;
            }
        }
        // O seguimento acaba quando o ciclo acaba: nas pilulas de RESULTADO, que sao passageiras
        // e nada pedem. A do PREVIEW continua a seguir, porque essa espera uma resposta e tem de
        // estar onde a pessoa esta a olhar.
        //
        // Ao sair NAO se salta para o cursor. Antes fazia-se, e dava um salto visivel: a janela
        // vinha atras do rato com suavizacao (fica sempre um pouco atras), e o reposicionamento
        // final apagava essa distancia de uma vez. Carregar em Esc no preview via-se como a
        // pilula a mudar de sitio no instante em que respondia. Agora fica onde estava e so se
        // garante que cabe no ecra, que era a unica razao para haver reposicionamento aqui.
        if !app
            .state::<state::AppState>()
            .follow_cursor
            .load(Ordering::SeqCst)
        {
            match current {
                Some((cx, cy)) => {
                    // MESMA regra de clamp do seguimento, e nao a da janela inteira. Eram duas:
                    // a posicao vinha calculada pela caixa visivel e a saida continha a janela
                    // toda, portanto ao aprovar o preview a pilula saltava de sitio no instante
                    // em que se carregava em Enter.
                    let phase = overlay_phase(&app);
                    // O monitor e o do CURSOR. Passar aqui o canto da janela era um erro
                    // silencioso: junto a borda esquerda ou de cima, esse canto cai no monitor
                    // do lado e a pilula era clampada ao ecra errado.
                    let point = app
                        .cursor_position()
                        .map(|c| (c.x as i32, c.y as i32))
                        .unwrap_or((cx.round() as i32, cy.round() as i32));
                    let (mon, scale) = monitor_at_point(&w, point.0, point.1);
                    let (nx, ny) = geom::clamp_window(cx, cy, mon, scale, phase, &LAYOUT);
                    let _ = w.set_position(PhysicalPosition::new(nx, ny));
                }
                // Nunca chegou a haver posicao suavizada (saiu no primeiro frame): ai o alvo do
                // cursor e a unica referencia que existe.
                None => {
                    if let Some(((x, y), _)) = orb_target(&app, &w) {
                        let _ = w.set_position(PhysicalPosition::new(x, y));
                    }
                }
            }
            break;
        }
        let now = tokio::time::Instant::now();
        let dt = (now - last).as_secs_f64();
        last = now;
        if let Some(((tx, ty), scale)) = orb_target(&app, &w) {
            // Travessia de monitor com DPI diferente: a janela ainda tem o tamanho fisico do
            // ecra anterior e o WebView2 pode ficar com a superficie cortada. Redimensiona-se
            // para o tamanho que a escala nova pede e re-emite-se o estado, que forca o webview
            // a repintar. Sem isto, o orb atravessava e ficava um retangulo meio pintado.
            if (scale - last_scale).abs() > f64::EPSILON {
                log::info!(
                    "overlay: escala {last_scale} -> {scale} ao atravessar de monitor; a redimensionar"
                );
                apply_scale(&w, scale);
                crate::flow::re_emit_state(&app);
                last_scale = scale;
                // O alvo foi calculado com a escala nova mas com a janela ainda com o tamanho
                // velho: recalcula-se no frame seguinte, ja com o tamanho certo.
                current = None;
                tick.tick().await;
                continue;
            }
            let (tx, ty) = (tx as f64, ty as f64);
            let (nx, ny) = match current {
                // Primeiro frame: snap ao alvo (sem arrasto a partir do canto).
                None => (tx, ty),
                Some((cx, cy)) => {
                    if app
                        .state::<state::AppState>()
                        .orb_visible
                        .load(Ordering::SeqCst)
                    {
                        // FAISCA: colada ao cursor, sem suavizacao. O arrasto deslocava o
                        // centro da orbita e a faisca parecia nadar atras do rato; a vida
                        // visual vem da propria orbita, o seguimento so tem de estar certo.
                        (tx, ty)
                    } else {
                        let alpha = 1.0 - (-dt / PILL_TAU).exp();
                        (cx + (tx - cx) * alpha, cy + (ty - cy) * alpha)
                    }
                }
            };
            let _ = w.set_position(PhysicalPosition::new(nx.round() as i32, ny.round() as i32));
            current = Some((nx, ny));

            // (A emissao ember://orb-motion morreu com a estrela: o tilt era o unico
            // consumidor do vetor de puxao, e a faisca segue rigida. Menos um IPC por frame.)
        }
        tick.tick().await;
    }
}

pub(crate) fn hide_orb(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
}

/// Cor de fundo nativa da janela por tema (RGBA opaco). Casa com `--color-panel` do CSS de cada
/// tema, para o canvas do WebView2 estar ja da cor certa no frame zero (sem flash antes do CSS).
fn theme_bg(theme: &str) -> Color {
    match theme {
        "cream" => Color(247, 242, 233, 255), // #f7f2e9
        _ => Color(17, 16, 20, 255),          // #111014 (dark, default)
    }
}

/// Pinta o fundo nativo da janela settings com a cor do tema guardado. Chamado na criacao da
/// janela e sempre que o tema muda (set_theme), para nenhuma abertura piscar a cor do outro tema.
pub(crate) fn apply_window_theme(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let theme = config::load(app).theme;
        let _ = w.set_background_color(Some(theme_bg(&theme)));
    }
}

pub(crate) fn show_settings(app: &AppHandle) {
    // A janela ja existia? So nesse caso emitimos settings-opened, que faz o React re-animar a
    // entrada (remount por `openKey`) E recarregar o estado do Rust. Se a estamos a criar agora,
    // NAO emitimos: o React acabou de montar, ja anima sozinho e ja foi buscar os dados; um emit
    // aqui dava um segundo remount (o conteudo aparecia, desaparecia e voltava) e chegaria antes
    // de o webview ter listener.
    //
    // Com o pre-aquecimento no arranque, o caminho normal e este: a janela existe, esta escondida
    // e ja hidratada, portanto abrir e mostrar + recarregar.
    let existed = app.get_webview_window("settings").is_some();
    let Some(w) = get_or_create_window(app, "settings") else {
        // Never reported before: the window did not exist and could not be created. Silent, this
        // was indistinguishable from "it opened and you cannot see it".
        log::error!("settings: could not get or create the window");
        return;
    };
    {
        // These three calls used to have their errors dropped with `let _ =`. A failing `show()`
        // gave exactly what we saw while debugging this: no window, no clue, nothing in the log.
        // You do not discard what you need to read when things go wrong.
        if let Err(e) = w.center() {
            log::warn!("settings: center failed: {e}");
        }
        if let Err(e) = w.show() {
            log::error!("settings: show failed: {e}");
        }
        if let Err(e) = w.set_focus() {
            log::warn!("settings: set_focus failed: {e}");
        }
        log::info!(
            "settings: shown (already existed={existed}, visible={:?})",
            w.is_visible()
        );
        if existed {
            let _ = w.emit("settings-opened", ());
        }
        // Se o modo debug estiver ligado, abre ja as devtools ao abrir as settings.
        if config::load(app).debug_mode {
            w.open_devtools();
        }
    }
}

/// Timestamp atual em ms (epoch), para o cache de probes de saude dos providers.
pub(crate) fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Pre-valida os fallbacks A ENTRADA (em background, nao bloqueia o arranque): prova, antes de
/// ser preciso, se ha um fallback conhecido-bom, e escreve no cache de saude. Cumpre a regra da
/// casa: o fallback e validado a entrada, nao no momento da falha.
async fn prevalidate_providers(app: AppHandle) {
    use ember_core::model::Provider;
    let state = app.state::<state::AppState>();
    let cfg = config::load(&app);
    // A cor do projeto ativo tem de estar pronta antes do primeiro refine, senao o primeiro orb
    // do arranque sairia sem ela e so os seguintes e que tomavam a cor.
    commands::refresh_orb_accent(&state, &cfg);
    let pctx = providers::ProviderCtx {
        openai_base_url: &cfg.openai_base_url,
    };
    for provider in cfg.provider_order() {
        // Modo subscricao: nao ha chave para provar, prova-se a sessao. Renovar o token aqui e o
        // que faz com que o primeiro refine do dia nao pague a espera da renovacao.
        if provider == Provider::OpenAi && cfg.openai_auth == config::OpenAiAuth::ChatGpt {
            let probe = oauth::probe(&state).await;
            if let Ok(mut m) = state.key_checks.lock() {
                m.insert(provider, (probe.check, now_ms()));
            }
            log::info!(
                "prevalidate {provider:?} (sessao ChatGPT): {:?} ({} modelos)",
                probe.check,
                probe.models.len()
            );
            models_cache::absorb(&app, &state, provider, &probe.models);
            continue;
        }
        // Bug A: ler pelo try_get. Um cofre bloqueado (Err) nao rebenta o arranque: loga e salta.
        // O caminho do refine vai, a seu tempo, reportar KeyStore honestamente quando for preciso.
        match secrets::try_get(provider) {
            Ok(Some(key)) => {
                let probe = providers::validate(&state.http, provider, &key, &pctx).await;
                if let Ok(mut m) = state.key_checks.lock() {
                    m.insert(provider, (probe.check, now_ms()));
                }
                log::info!(
                    "prevalidate {provider:?}: {:?} ({} modelos)",
                    probe.check,
                    probe.models.len()
                );
                models_cache::absorb(&app, &state, provider, &probe.models);
            }
            Ok(None) => {}
            Err(_) => log::warn!("prevalidate {provider:?}: keyring read failed, skipping"),
        }
    }
}

/// Marca `quitting` e sai, uma so vez (guarda `swap` para o comando e o fallback de timeout
/// nao chamarem `exit` duas vezes). Chamado quando a animacao de quit termina, ou pelo fallback.
pub(crate) fn finalize_quit_now(app: &AppHandle) {
    if !app
        .state::<state::AppState>()
        .quitting
        .swap(true, Ordering::SeqCst)
    {
        // Se ha uma chamada ao modelo a decorrer, da-se-lhe um instante para acabar e gravar: ela
        // ja esta paga, e sair a meio deitava fora o resultado, que e precisamente o que este
        // trabalho todo existe para evitar. Limitado a 1.5s, porque um stream preso nao pode
        // impedir a app de fechar; a animacao de quit ja demora 1.2s, portanto quase nunca se ve.
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            const GRACE: std::time::Duration = std::time::Duration::from_millis(1500);
            let started = std::time::Instant::now();
            let mut waited = false;
            while started.elapsed() < GRACE {
                let busy = app2
                    .state::<state::AppState>()
                    .inflight
                    .lock()
                    .map(|f| !f.is_empty())
                    .unwrap_or(false);
                if !busy {
                    break;
                }
                waited = true;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            if waited {
                log::info!(
                    "quit: esperou {}ms pela chamada em curso",
                    started.elapsed().as_millis()
                );
            }
            app2.exit(0);
        });
    }
}

/// Abre/fecha as devtools da janela de settings conforme o modo debug (efeito imediato do
/// toggle). Requer a feature `devtools` do tauri, ativa tambem em release para isto funcionar.
pub(crate) fn apply_devtools(app: &AppHandle, enabled: bool) {
    if let Some(w) = app.get_webview_window("settings") {
        if enabled {
            w.open_devtools();
        } else {
            w.close_devtools();
        }
    }
}

/// (Re)regista TODOS os atalhos globais a partir da config: o principal (que usa o modo
/// escolhido nas settings) e os dois que fixam um modo.
///
/// Tudo-ou-nada. O `unregister_all` no inicio deixa a app sem atalho nenhum, por isso um
/// registo que falhe a meio deixaria o utilizador sem forma de disparar o refine, e sem
/// perceber porque. Em caso de erro, tudo o que ja tinha sido registado e desfeito e o erro
/// sobe; quem chama restaura a config anterior (ver `commands::set_hotkeys`).
///
/// Um atalho de modo VAZIO nao e um erro: quer dizer "nao registes este". Quem so quer um
/// atalho fica com um, sem arriscar conflitos com outras apps por causa de dois que nao usa.
/// O que um atalho global dispara. Nasceu quando o picker chegou: um `Option<RefineMode>` ja nao
/// chegava, porque abrir o picker nao e um modo de refinar.
#[derive(Clone, Copy)]
pub(crate) enum HotkeyAction {
    /// Refina. `None` = usa o modo das settings, lido a cada disparo.
    Refine(Option<RefineMode>),
    /// Abre o picker de projetos ao cursor.
    Picker,
}

pub(crate) fn register_hotkeys(app: &AppHandle, cfg: &config::Config) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let wanted: [(&str, HotkeyAction); 4] = [
        (cfg.hotkey.as_str(), HotkeyAction::Refine(None)),
        (
            cfg.hotkey_polish.as_str(),
            HotkeyAction::Refine(Some(RefineMode::Polish)),
        ),
        (
            cfg.hotkey_turbo.as_str(),
            HotkeyAction::Refine(Some(RefineMode::Turbo)),
        ),
        (cfg.hotkey_picker.as_str(), HotkeyAction::Picker),
    ];
    for (combo, action) in wanted {
        if combo.trim().is_empty() {
            continue;
        }
        if let Err(e) = register_one(app, combo, action) {
            let _ = gs.unregister_all();
            return Err(format!("{combo}: {e}"));
        }
    }
    Ok(())
}

/// Escolhe o primeiro atalho da lista de candidatos que o sistema aceite, e devolve-o.
///
/// Existe porque um atalho fixo por omissao nao serve: qualquer combinacao pode ja estar tomada
/// na maquina de alguem, o registo e tudo-ou-nada, e o resultado seria uma instalacao limpa a
/// arrancar com um aviso por causa de um atalho que a pessoa nem escolheu. Testar a lista custa
/// microssegundos e resolve isso de vez.
///
/// `None` se nenhum candidato estiver livre, caso em que o caller fica com o que tinha e abre as
/// settings: nesse ponto so o utilizador pode decidir.
pub(crate) fn first_free_hotkey(app: &AppHandle) -> Option<String> {
    for combo in ember_core::hotkey::DEFAULT_HOTKEY_CANDIDATES {
        if probe_hotkey_free(app, combo) {
            return Some(combo.to_string());
        }
        log::info!("first-run hotkey: {combo} ja esta ocupado, tento o seguinte");
    }
    None
}

/// O SO onde estamos, para a politica pura de atalhos (`ember_core::hotkey`).
pub(crate) fn current_os() -> ember_core::hotkey::Os {
    if cfg!(windows) {
        ember_core::hotkey::Os::Windows
    } else if cfg!(target_os = "macos") {
        ember_core::hotkey::Os::MacOs
    } else {
        ember_core::hotkey::Os::Other
    }
}

/// Tenta registar `accel` so para ver se o SO o aceita, e liberta-o logo a seguir.
///
/// E o unico teste que vale no Windows, onde o `RegisterHotKey` falha mesmo quando outra app ja
/// tem a combinacao. No macOS este teste passa quase sempre (o sistema deixa registar e depois
/// ganha em silencio), e por isso e que a lista de atalhos reservados existe: sao as duas metades
/// da mesma resposta, nenhuma chega sozinha.
///
/// Nao mexe nos atalhos ja registados: regista SO o candidato e desfaz. Se o candidato for um dos
/// nossos, o caller ja o apanhou antes de chegar aqui.
pub(crate) fn probe_hotkey_free(app: &AppHandle, accel: &str) -> bool {
    let gs = app.global_shortcut();
    match gs.register(accel) {
        Ok(()) => {
            let _ = gs.unregister(accel);
            true
        }
        Err(_) => false,
    }
}

/// Regista um atalho. `forced_mode` a `None` significa "o modo que estiver nas settings".
fn register_one(app: &AppHandle, hotkey: &str, action: HotkeyAction) -> Result<(), String> {
    let gs = app.global_shortcut();
    gs.on_shortcut(hotkey, move |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            // O picker tem um caminho proprio, muito mais curto que o do refine: sem captura,
            // sem clipboard, sem orb. As guardas (busy, reentrancia) vivem dentro dele.
            if matches!(action, HotkeyAction::Picker) {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    picker::open_picker(app).await;
                });
                return;
            }
            let HotkeyAction::Refine(forced_mode) = action else {
                return;
            };
            // Um refine a arrancar fecha o picker se ele estiver aberto: dois hooks LL vivos a
            // consumir Enter e o perigo real, e o refine tem prioridade (e o trabalho a serio).
            {
                let st = app.state::<state::AppState>();
                if st.picker_open.load(Ordering::SeqCst) {
                    st.picker_cancel.store(true, Ordering::SeqCst);
                }
            }
            // Guarda de reentrancia. Se ja houver um refine a decorrer, esta segunda tecla
            // CANCELA-o (em vez de arrancar um segundo fluxo, que corromperia o clipboard).
            let st = app.state::<state::AppState>();
            if st.busy.swap(true, Ordering::SeqCst) {
                // Ja ha um ciclo a decorrer: esta tecla DISPENSA a espera. Nao mata a chamada ao
                // modelo, que segue ate ao fim numa tarefa propria e guarda o refinado; antes
                // matava-a, o provider cobrava na mesma e o resultado ia para o lixo. O atalho
                // seguinte sobre o mesmo texto junta-se a essa chamada ou usa o que ela guardou.
                let run = st.run_seq.load(Ordering::SeqCst);
                log::info!("[run {run}] hotkey: dispensado pela segunda tecla");
                st.request_dismiss(run);
                return;
            }
            let run_id = st.run_seq.fetch_add(1, Ordering::SeqCst) + 1;
            // Este ciclo passa a ser o dono da overlay: um `hide_after` de um ciclo anterior
            // (a pilula ainda no ecra) compara com isto e deixa de lhe mexer.
            st.hide_gen.store(run_id, Ordering::SeqCst);
            let cfg = config::load(app);
            // Deteta o terminal E captura o titulo da janela (para contexto de projeto) ANTES de
            // mostrar o orb: a app em foco ainda e o alvo, o nosso orb nao rouba o foco.
            let terminal = cfg.terminal_handling && foreground::is_terminal_foreground();
            // O alvo do paste fica fixado AQUI, com a app do utilizador ainda em foco. Verifica-se
            // outra vez mesmo antes de colar: entre as duas coisas pode passar uma chamada longa
            // e um preview de dez segundos, e colar as cegas metia o texto na app errada.
            let target_hwnd = foreground::foreground_target();
            log::info!(
                "[run {run_id}] hotkey: mode={:?} terminal_handling={} exe={:?} -> terminal={}",
                forced_mode.unwrap_or(cfg.mode),
                cfg.terminal_handling,
                foreground::debug_foreground_exe(),
                terminal
            );
            let project_title = if cfg.project_context {
                foreground::foreground_title()
            } else {
                None
            };
            let opts = flow::RunOpts {
                terminal,
                timing: flow::CaptureTiming {
                    polls: cfg.capture_polls,
                    step_ms: cfg.capture_step_ms,
                    settle_ms: cfg.paste_settle_ms,
                },
                project_title,
                preview: cfg.preview_before_paste,
                select_all_fallback: cfg.select_all_fallback
                    && foreground::select_all_is_safe_here(),
                select_all_max_chars: cfg.select_all_max_chars,
                mode: forced_mode.unwrap_or(cfg.mode),
                run_id,
                target_hwnd,
            };
            show_orb_at_cursor(app);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                flow::run(app.clone(), opts).await;
                // Rede de seguranca para um caminho de saida que nao passe pelo `finish`, mas
                // SO se este ciclo ainda for o dono.
                //
                // Incondicional era um bug a serio: o `finish` liberta a guarda e so depois
                // espera pela pilula (~2s), portanto quando o `run` termina ja pode haver outro
                // ciclo a decorrer. Libertar ai punha a `false` a guarda DELE, e a tecla
                // seguinte arrancava um terceiro refine em paralelo: dois hooks LL de teclado
                // vivos ao mesmo tempo (regra 3 do CLAUDE.md) e dois a armar o clipboard.
                let st = app.state::<state::AppState>();
                let current = st.hide_gen.load(Ordering::SeqCst);
                if ember_core::may_release_guard(current, run_id) {
                    st.busy.store(false, Ordering::SeqCst);
                }
            });
        }
    })
    .map_err(|e| e.to_string())
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open_settings", "Settings").build(app)?;
    // A saida para tudo o que interrompeu um refine: dispensado, recusado no preview, clipboard
    // ocupado, janela trocada. Guardar o resultado so serve para alguma coisa se houver uma
    // maneira de o aplicar depois.
    let reapply = MenuItemBuilder::with_id("reapply_last", "Reapply last refine").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&open, &reapply, &quit])
        .build()?;
    let Some(icon) = app.default_window_icon().cloned() else {
        // Sem icone nao construimos a tray (em vez de rebentar). A app continua viva; o log
        // deixa rasto. Na pratica o icone vem sempre da config, por isso isto e defensivo.
        log::error!("tray: no default window icon, skipping tray build");
        return Ok(());
    };
    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Ember")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open_settings" => {
                show_settings(app);
            }
            "reapply_last" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move { flow::reapply_last(app).await });
            }
            "quit" => {
                if let Some(quit_anim) = get_or_create_window(app, "quit_anim") {
                    let _ = quit_anim.set_ignore_cursor_events(true);
                    let _ = quit_anim.show();
                }
                // A animacao de quit chama `finalize_quit` quando termina: a saida acopla ao
                // fim REAL da animacao, nao a um numero magico que podia divergir do duration.
                // Fallback: se a webview nao completar (falhou a carregar), forca a saida ao
                // fim de um tempo curto, para nunca ficar preso na tray sem sair.
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                    finalize_quit_now(&app);
                });
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Panic hook antes de tudo: em release a consola esta destacada, por isso sem isto um
    // panic nao deixava rasto nenhum. Grava panic + backtrace no log.
    logging::install_panic_hook();
    tauri::Builder::default()
        // single-instance TEM de ser o primeiro plugin.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Log ON ENTRY: without this there is no way to tell "the second instance never
            // reached the first one" from "it did, and showing the window failed", and those two
            // send you looking in opposite places.
            log::info!("single-instance: second instance detected, showing settings");
            show_settings(app);
        }))
        // Log logo a seguir, para captar a inicializacao dos plugins seguintes.
        .plugin(logging::plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_model,
            commands::set_openai_base_url,
            commands::set_hotkey,
            commands::set_autostart,
            commands::set_mode,
            commands::set_theme,
            commands::set_thinking,
            commands::set_terminal_handling,
            commands::set_project_context,
            commands::set_preview_before_paste,
            commands::set_capture_timing,
            commands::set_api_key,
            commands::clear_api_key,
            commands::validate_key,
            commands::list_models,
            commands::chatgpt_login,
            commands::chatgpt_logout,
            commands::set_openai_auth,
            commands::set_primary_provider,
            commands::check_hotkey,
            commands::set_gemini_model_auto,
            commands::set_select_all_fallback,
            commands::get_provider_health,
            commands::set_profile,
            commands::reload_profile,
            commands::reset_profile,
            commands::close_splash,
            commands::finalize_quit,
            commands::set_debug_mode,
            commands::set_save_prompts,
            commands::set_keep_results,
            commands::save_project,
            commands::delete_project,
            commands::set_active_project,
            commands::preview_accent,
            commands::accent_from_wheel,
            commands::scan_project_folder,
            commands::distill_project,
            commands::read_recent_logs,
            commands::reveal_log_dir,
            commands::open_repo,
            commands::open_key_console,
            commands::read_profile_file,
            commands::get_diagnostics,
        ])
        .setup(|app| {
            build_tray(app)?;
            let handle = app.handle().clone();

            // Refinados ja pagos de sessoes anteriores. Sem isto, fechar a app deitava fora
            // dinheiro gasto e o mesmo texto voltava a ser cobrado no arranque seguinte.
            if config::load(&handle).keep_results {
                let cache = refine_store::load(&handle);
                if let Ok(mut slot) = handle.state::<state::AppState>().store.lock() {
                    *slot = cache;
                }
            } else {
                refine_store::forget(&handle);
            }

            let is_install = match handle.path().app_data_dir() {
                Ok(app_dir) => {
                    let marker = app_dir.join(".installed");
                    let first_run = !marker.exists();
                    if first_run {
                        if let Err(e) = std::fs::create_dir_all(&app_dir) {
                            log::warn!("install: create_dir_all failed: {e}");
                        }
                        if let Err(e) = std::fs::write(&marker, b"") {
                            log::warn!("install: writing .installed marker failed: {e}");
                        }
                    }
                    first_run
                }
                Err(e) => {
                    log::warn!("install: app_data_dir unavailable: {e}; treating as non-install");
                    false
                }
            };

            let window_name = if is_install { "splash" } else { "startup_anim" };
            match get_or_create_window(&handle, window_name) {
                Some(anim) => {
                    let _ = anim.set_ignore_cursor_events(true);
                    let _ = anim.show();
                    log::info!("startup animation: showing '{window_name}'");
                }
                // A animacao de arranque E o sinal de vida da app: sem ela nao ha nada a dizer
                // ao utilizador que o Ember arrancou. Se a janela nao nasce, isso tem de ficar
                // no log em vez de a app arrancar muda e parecer que nao correu. A causa mais
                // comum e ja haver outra instancia (o WebView2 devolve "resource is in use").
                None => log::error!(
                    "startup animation: could not create '{window_name}'; Ember started with no                      visible sign of life (another instance running?)"
                ),
            }

            // Pre-cria a janela overlay (escondida) para o listener do orb estar pronto
            // antes do primeiro hotkey (senao o evento "refining" perde-se).
            let _ = get_or_create_window(&handle, "overlay");
            // O picker tambem: sem pre-criacao, o primeiro `ember://picker` disparava antes de o
            // webview ter listener e a primeira abertura mostrava uma caixa vazia.
            let _ = get_or_create_window(&handle, "picker");
            // As settings tambem, mas DEPOIS do arranque estar despachado: e a janela mais
            // pesada (920x640, o bundle inteiro do React) e criar-la so no clique fazia pagar
            // ali tudo de uma vez, arrancar o webview, carregar o bundle, montar e ir buscar as
            // definicoes, com a janela ja visivel a meio disso. Aquecida, abrir passa a ser um
            // `show()`. O atraso deixa o arranque da app respirar primeiro; quem clica na tray
            // nos primeiros dois segundos cai no caminho antigo (cria na hora), que continua a
            // funcionar. Nota: a janela nasce escondida (`visible: false` na config), por isso
            // isto nao pisca nada no ecra.
            {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    if h.get_webview_window("settings").is_none() {
                        let _ = get_or_create_window(&h, "settings");
                        log::info!("settings: janela pre-aquecida");
                    }
                });
            }
            // Pre-valida os fallbacks em background (nao bloqueia o arranque).
            tauri::async_runtime::spawn(prevalidate_providers(handle.clone()));
            let mut cfg = config::load(&handle);
            // Escolhe um atalho que esteja mesmo livre nesta maquina em dois casos: no primeiro
            // arranque (em vez de impor um fixo que pode ja estar ocupado), e quando o que esta
            // gravado nao pode ser registado com seguranca.
            //
            // O segundo caso nao e hipotetico: ficou gravado `"hotkey": "Enter"`, que o SO aceita
            // sem se queixar e que a partir dai rouba o Enter a toda a gente enquanto o Ember
            // estiver aberto. Uma config assim tem de ser corrigida no arranque, porque o
            // utilizador nao consegue ligar o sintoma (o Enter deixou de funcionar) a esta causa.
            let saved_verdict = ember_core::hotkey::evaluate(&cfg.hotkey, current_os(), &[]);
            let unsafe_saved = !matches!(saved_verdict, ember_core::hotkey::HotkeyVerdict::Available);
            if is_install || unsafe_saved {
                if unsafe_saved {
                    log::warn!(
                        "saved hotkey '{}' is not safe to register ({saved_verdict:?}); picking another",
                        cfg.hotkey
                    );
                }
                if let Some(free) = first_free_hotkey(&handle) {
                    if free != cfg.hotkey {
                        log::info!("hotkey chosen automatically: {free}");
                        cfg.hotkey = free;
                        if let Err(e) = config::save(&handle, &cfg) {
                            log::warn!("hotkey: could not persist the chosen one: {e}");
                        }
                    }
                }
            }
            log::info!(
                "Ember {} started (install={is_install}, debug={}, hotkey={})",
                handle.package_info().version,
                cfg.debug_mode,
                cfg.hotkey
            );
            // Reconcilia o bool de autostart com o estado real do plugin (a fonte de verdade
            // do SO). Podiam divergir (config editada a mao, entrada removida por fora); sem
            // isto, get_settings mostrava um valor possivelmente obsoleto.
            {
                use tauri_plugin_autostart::ManagerExt;
                if let Ok(actual) = handle.autolaunch().is_enabled() {
                    if actual != cfg.autostart {
                        log::info!("autostart drift: config={}, actual={actual}; syncing config", cfg.autostart);
                        let mut synced = cfg.clone();
                        synced.autostart = actual;
                        if let Err(e) = config::save(&handle, &synced) {
                            log::warn!("autostart: could not persist reconciled state: {e}");
                        }
                    }
                }
            }
            // Se o atalho guardado nao registar (ocupado por outra app, ou invalido de uma
            // versao anterior), abre as settings em vez de arrancar sem hotkey em silencio.
            if let Err(e) = register_hotkeys(&handle, &cfg) {
                // Um dos tres nao registou (ocupado por outra app, ou invalido de uma versao
                // anterior). O registo e tudo-ou-nada, por isso neste ponto a app esta SEM
                // atalho nenhum: um atalho de modo opcional em conflito teria acabado de levar
                // o principal com ele. Tenta outra vez so com o principal, que e o que torna a
                // app utilizavel, e so abre as settings se nem esse registar.
                log::warn!("hotkeys failed to register ({e}); retrying with the main one only");
                let mut only_main = cfg.clone();
                only_main.hotkey_polish.clear();
                only_main.hotkey_turbo.clear();
                match register_hotkeys(&handle, &only_main) {
                    Ok(()) => {
                        log::warn!("main hotkey is up; the per-mode ones are off until fixed");
                        show_settings(&handle);
                    }
                    Err(e2) => {
                        // Nem o principal registou: outra app tem-no. Em vez de arrancar sem
                        // atalho nenhum (a app fica inutil e sem dizer porque), procura um livre
                        // e fica com esse, exatamente como no primeiro arranque.
                        log::warn!("main hotkey '{}' also failed ({e2})", cfg.hotkey);
                        match first_free_hotkey(&handle) {
                            Some(free) => {
                                only_main.hotkey = free.clone();
                                if register_hotkeys(&handle, &only_main).is_ok() {
                                    log::warn!("main hotkey moved to '{free}'");
                                    cfg.hotkey = free;
                                    let _ = config::save(&handle, &cfg);
                                }
                            }
                            None => log::error!("no free hotkey found; Ember has none registered"),
                        }
                        show_settings(&handle);
                    }
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("erro ao construir o Ember")
        .run(|app, event| {
            // Manter o processo vivo na tray quando se fecham janelas, MAS deixar sair
            // quando o utilizador pede Quit explicitamente.
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if !app
                    .state::<state::AppState>()
                    .quitting
                    .load(Ordering::SeqCst)
                {
                    api.prevent_exit();
                }
            }
        });
}
