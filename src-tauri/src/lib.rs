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
mod prompt_log;
mod profile;
mod project;
mod projects;
mod providers;
mod secrets;
mod selection;
mod state;

use std::sync::atomic::Ordering;

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::window::Color;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewWindow, WebviewWindowBuilder, Emitter};
use tauri_plugin_autostart::MacosLauncher;
use ember_core::model::RefineMode;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Lado do quadrado da faisca, em px logicos. ESPELHADO em `SPARK_SIZE` (Orb.tsx); muda um,
/// muda o outro, senao a orbita descentra-se do ponteiro.
const SPARK_SIZE: f64 = 40.0;
/// Caixa a garantir visivel: a faisca mais a folga do estado de retry, onde o rotor cresce
/// ~32% (ver `VARIANT` em Orb.tsx).
const SPARK_CLAMP: f64 = 56.0;
/// Centro visual do ponteiro em relacao ao hotspot, em px logicos. Uma seta padrao do Windows
/// ocupa ~12x19 para baixo e para a direita do hotspot; o meio do corpo dela cai aqui.
const POINTER_CENTER: (f64, f64) = (6.0, 9.0);

/// Caixa da PILULA dentro da janela, em px logicos: o desvio lateral (espelhado no `ml-[34px]`
/// do Pill.tsx, la `ml-10`) e um tamanho generoso que cobre a frase mais longa ("Enter to apply · Esc to
/// keep original"). Serve para clampar pela pilula VISIVEL em vez de pela janela inteira.
const PILL_MARGIN_X: f64 = 40.0;
const PILL_BOX: (f64, f64) = (300.0, 40.0);

/// Onde esta o conteudo visivel dentro da janela, e que tamanho tem, para a fase atual. Tudo em
/// px fisicos.
///
/// Existe porque havia DUAS maneiras de clampar (caixa pequena para o orb, janela inteira para
/// a pilula) e a mudanca de fase saltava de uma para a outra: ao aprovar o preview, a janela
/// que estava colocada pela caixa do orb era subitamente contida pela regra da janela inteira e
/// a pilula saltava de sitio. Agora ha uma regra so, e o que muda entre fases e apenas o
/// tamanho da caixa.
fn content_box(is_orb: bool, wh: i32, pad: i32, scale: f64) -> (i32, i32, i32, i32) {
    let px = |v: f64| (v * scale).round() as i32;
    if is_orb {
        // A caixa clampada e MAIOR que a faisca (SPARK_CLAMP vs SPARK_SIZE) porque o rotor
        // cresce no estado de retry: sem esta folga, junto a borda do ecra a orbita inchada
        // saia por fora do que garantimos visivel. O `dx` recua metade da folga para o CENTRO
        // da caixa continuar a ser o mesmo ponto, que e o que ancora a orbita no ponteiro.
        let side = px(SPARK_CLAMP);
        let folga = (px(SPARK_CLAMP) - px(SPARK_SIZE)) / 2;
        (pad - folga, (wh - side) / 2, side, side)
    } else {
        let (bw, bh) = PILL_BOX;
        let (bw, bh) = (px(bw), px(bh));
        (pad + px(PILL_MARGIN_X), (wh - bh) / 2, bw, bh)
    }
}

/// Clampa a janela ao monitor mantendo a CAIXA VISIVEL dentro do ecra (a janela pode ficar
/// pendurada de fora; ninguem a ve, e transparente e ignora cliques). Fonte unica para o
/// seguimento e para a saida do ciclo, que e onde a divergencia dava o salto.
fn clamp_visible(
    w: &WebviewWindow,
    is_orb: bool,
    win_x: i32,
    win_y: i32,
    cursor: (i32, i32),
) -> (i32, i32) {
    let wh = match w.outer_size() {
        Ok(s) => s.height as i32,
        Err(_) => OVERLAY_FALLBACK_SIZE.1,
    };
    let scale = w.scale_factor().unwrap_or(1.0);
    let pad = (8.0 * scale).round() as i32;
    let (dx, dy, cw, ch) = content_box(is_orb, wh, pad, scale);
    let (ax, ay, aw, ah) = monitor_at_point(w, cursor.0, cursor.1);
    ember_core::selection::clamp_window_for_content(win_x, win_y, dx, dy, cw, ch, ax, ay, aw, ah)
}

/// Tamanho da janela do overlay, espelhando a declaracao em `tauri.conf.json` (label
/// "overlay"). So usado como fallback se `w.outer_size()` falhar (raro); nomeado para nao
/// ter o mesmo par de numeros duplicado sem explicacao em dois ficheiros.
/// Espelha `width`/`height` da janela `overlay` no tauri.conf.json. So e usado quando o SO nao
/// consegue dizer o tamanho real; divergir daria um clamping errado junto as bordas do ecra.
const OVERLAY_FALLBACK_SIZE: (i32, i32) = (520, 140);

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
    let w = WebviewWindowBuilder::from_config(app, &cfg).ok()?.build().ok()?;
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

/// Geometria do monitor atual da janela (para clampar o orb ao ecra).
fn monitor_work_area(w: &WebviewWindow) -> (i32, i32, i32, i32) {
    if let Ok(Some(mon)) = w.current_monitor() {
        let p = mon.position();
        let s = mon.size();
        (p.x, p.y, s.width as i32, s.height as i32)
    } else {
        (0, 0, 1920, 1080)
    }
}

/// Geometria do monitor que contem o ponto (px,py), tipicamente o cursor. Ao contrario
/// de `monitor_work_area`, nao depende de onde a janela esta agora, por isso o orb
/// consegue atravessar para outro ecra em vez de ficar preso na borda do monitor de
/// origem quando o cursor muda de ecra a meio do seguimento.
pub(crate) fn monitor_at_point(w: &WebviewWindow, px: i32, py: i32) -> (i32, i32, i32, i32) {
    let monitors: Vec<(i32, i32, i32, i32)> = w
        .available_monitors()
        .map(|ms| {
            ms.iter()
                .map(|m| {
                    let p = m.position();
                    let s = m.size();
                    (p.x, p.y, s.width as i32, s.height as i32)
                })
                .collect()
        })
        .unwrap_or_default();
    ember_core::selection::monitor_containing(px, py, &monitors)
        .unwrap_or_else(|| monitor_work_area(w))
}

/// Top-left desejado da janela do overlay para o cursor atual. O conteudo esta alinhado a
/// esquerda e centrado na vertical (ver Overlay.tsx), com o padding `p-2` (8px logicos) a
/// separar do canto. Ancoramos o BORDO ESQUERDO do conteudo (nao o centro) junto ao cursor
/// + offset, para o conteudo crescer para a direita: a pilula e larga e, centrada, cairia
/// por cima do rato em vez de aparecer ao lado como o orb.
fn orb_target(app: &AppHandle, w: &WebviewWindow) -> Option<(i32, i32)> {
    let c = app.cursor_position().ok()?;
    let (_, wh) = match w.outer_size() {
        Ok(s) => (s.width as i32, s.height as i32),
        Err(_) => OVERLAY_FALLBACK_SIZE,
    };
    let scale = w.scale_factor().unwrap_or(1.0);
    let pad = (8.0 * scale).round() as i32;
    let is_orb = app
        .state::<state::AppState>()
        .orb_visible
        .load(Ordering::SeqCst);
    // UMA ancora para as duas fases: o centro visual do ponteiro. Antes havia duas (faisca
    // centrada no cursor, pilulas ao lado) e elas discordavam, porque a janela NAO se
    // reposiciona quando a fase muda (mexe-la ai dava o salto visivel que se via ao carregar
    // em Esc). A pilula herdava entao o centro da faisca e nascia por cima do cursor. Agora a
    // janela fica onde esta e e o CSS da pilula que a afasta para o lado (ver Pill.tsx), o que
    // tambem mantem o morph coerente: a faisca colapsa no ponteiro e a pilula abre a direita.
    //
    // O centro NAO e o cursor em si: o `cursor_position` devolve o hotspot, que numa seta e a
    // pontinha de cima-esquerda. `POINTER_CENTER` empurra-o para o meio do corpo da seta
    // (~12x19 logicos a 100%), senao o anel abracava so a ponta.
    let (dx, dy) = POINTER_CENTER;
    let anchor_x = c.x as i32 + ((dx - SPARK_SIZE / 2.0) * scale).round() as i32;
    let anchor_y = c.y as i32 + (dy * scale).round() as i32;
    let win_x = anchor_x - pad;
    let win_y = anchor_y - wh / 2;
    Some(clamp_visible(
        w,
        is_orb,
        win_x,
        win_y,
        (c.x as i32, c.y as i32),
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
    if let Some((x, y)) = orb_target(app, &w) {
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
    let _ = w.show();
    // NB: nao chamamos set_focus. O paste tem de aterrar na app em foco, nao na nossa.

    // Loop de seguimento: corre enquanto o orb estiver visivel, colado ao cursor.
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { orb_follow_loop(app2).await });
}

/// Segue o cursor com suavizacao exponencial (lerp) enquanto o orb esta visivel, para um
/// arrasto fluido tipo Apple em vez de saltos. Termina quando `hide_orb` esconde. Usa um
/// `interval` a 120fps (nao `sleep`, que acumula deriva). A suavizacao usa o dt REAL via
/// `alpha = 1 - exp(-dt/tau)`: assim mantem a mesma sensacao mesmo que um tick atrase (um
/// factor fixo por frame mudava de velocidade com o frame-rate, um bug subtil de engasgo).
async fn orb_follow_loop(app: AppHandle) {
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
                    // MESMA regra de clamp do seguimento (`clamp_visible`), e nao a da janela
                    // inteira. Eram duas: a posicao vinha calculada pela caixa visivel e a
                    // saida continha a janela toda, portanto ao aprovar o preview a pilula
                    // saltava de sitio no instante em que se carregava em Enter.
                    let is_orb = app
                        .state::<state::AppState>()
                        .orb_visible
                        .load(Ordering::SeqCst);
                    let (nx, ny) = clamp_visible(
                        &w,
                        is_orb,
                        cx.round() as i32,
                        cy.round() as i32,
                        (cx.round() as i32, cy.round() as i32),
                    );
                    let _ = w.set_position(PhysicalPosition::new(nx, ny));
                }
                // Nunca chegou a haver posicao suavizada (saiu no primeiro frame): ai o alvo do
                // cursor e a unica referencia que existe.
                None => {
                    if let Some((x, y)) = orb_target(&app, &w) {
                        let _ = w.set_position(PhysicalPosition::new(x, y));
                    }
                }
            }
            break;
        }
        let now = tokio::time::Instant::now();
        let dt = (now - last).as_secs_f64();
        last = now;
        if let Some((tx, ty)) = orb_target(&app, &w) {
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
    if let Some(w) = get_or_create_window(app, "settings") {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
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
        app.exit(0);
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
        (cfg.hotkey_polish.as_str(), HotkeyAction::Refine(Some(RefineMode::Polish))),
        (cfg.hotkey_turbo.as_str(), HotkeyAction::Refine(Some(RefineMode::Turbo))),
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
fn register_one(
    app: &AppHandle,
    hotkey: &str,
    action: HotkeyAction,
) -> Result<(), String> {
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
                st.cancel.store(true, Ordering::SeqCst);
                st.cancel_notify.notify_waiters();
                return;
            }
            // Arranque limpo: sem cancelamento pendente de um ciclo anterior.
            st.cancel.store(false, Ordering::SeqCst);
            let cfg = config::load(app);
            // Deteta o terminal E captura o titulo da janela (para contexto de projeto) ANTES de
            // mostrar o orb: a app em foco ainda e o alvo, o nosso orb nao rouba o foco.
            let terminal = cfg.terminal_handling && foreground::is_terminal_foreground();
            log::info!(
                "hotkey: mode={:?} terminal_handling={} exe={:?} -> terminal={}",
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
            };
            show_orb_at_cursor(app);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                flow::run(app.clone(), opts).await;
                // Liberta a guarda so no fim do ciclo (o orb ja foi escondido dentro de run):
                // ate aqui, o hide_after deste ciclo nao pode ser pisado por outra tecla.
                app.state::<state::AppState>()
                    .busy
                    .store(false, Ordering::SeqCst);
            });
        }
    })
    .map_err(|e| e.to_string())
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open_settings", "Settings").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&open, &quit]).build()?;
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
            commands::save_project,
            commands::delete_project,
            commands::set_active_project,
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
