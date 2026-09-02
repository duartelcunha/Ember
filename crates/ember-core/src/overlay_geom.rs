//! Geometria da janela do overlay: onde a colocar para o cursor atual, num monitor e a uma
//! escala DPI dados.
//!
//! Vive aqui, e nao no shell, por duas razoes que custaram bugs reais:
//!
//! 1. **Multi-monitor.** O shell perguntava a escala e o tamanho a JANELA (`scale_factor()`,
//!    `outer_size()`), que descrevem o monitor onde ela ESTA, nao aquele onde o cursor esta. Ao
//!    atravessar para um ecra com DPI diferente, o Windows so corrige a janela no WM_DPICHANGED
//!    seguinte: durante esses frames os offsets eram calculados a escala errada e a orb ficava
//!    presa na fronteira. Aqui a escala e um PARAMETRO: quem chama passa a do monitor do cursor.
//! 2. **Deriva.** Havia tres arredondamentos independentes (o `pad`, cada offset da caixa de
//!    conteudo, e a divisao inteira da folga) que discordavam entre si a 125% e 150%. Aqui tudo
//!    corre em f64 e arredonda-se UMA vez, no fim.
//!
//! Tudo em px FISICOS a saida; as constantes do `Layout` sao logicas (px CSS), como no frontend.

/// Retangulo de um monitor em coordenadas fisicas do ambiente virtual do Windows. Com dois
/// ecras, o segundo tem `x` (e as vezes `y`) diferente de zero; com o ecra da esquerda como
/// secundario, `x` e negativo. Nada aqui assume origem em (0,0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
}

/// Medidas do overlay em px LOGICOS. ESPELHADAS no frontend; muda uma, muda a outra:
/// `spark` <-> `SPARK_SIZE` (Orb.tsx), `pad` <-> `p-2` (Overlay.tsx), `pill_margin_x` <->
/// `ml-10` (Pill.tsx), `win_logical` <-> `width`/`height` da janela "overlay" (tauri.conf.json).
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    /// Lado do quadrado da faisca.
    pub spark: f64,
    /// Caixa a garantir visivel na fase de orb: a faisca mais a folga do estado de retry, onde
    /// a marca cresce (ver `VARIANT` em Orb.tsx) e o halo estatico pinta alguns px para fora.
    /// Ha margem de sobra sobre o crescimento atual, de proposito: e mais barato garantir 56px
    /// visiveis do que descobrir junto a borda do ecra que faltavam dois.
    pub spark_clamp: f64,
    /// Largura reservada a direita da faisca para a etiqueta do projeto (max 120) e a legenda
    /// de retry (max 170), com as folgas entre elas.
    pub labels_w: f64,
    /// Centro visual do ponteiro em relacao ao hotspot. Uma seta padrao do Windows ocupa ~12x19
    /// para baixo e para a direita do hotspot; o meio do corpo dela cai aqui.
    ///
    /// Fixo de proposito. Ler o hotspot real (`GetCursorInfo` + `GetIconInfo`) e possivel, mas o
    /// hotspot muda entre a seta e o I-beam, e num refine passa-se de um para o outro a meio do
    /// seguimento: a orb saltaria ao mudar de forma do cursor, que e pior do que estar 2px ao
    /// lado com um cursor gigante de acessibilidade.
    pub pointer_center: (f64, f64),
    /// Padding do conteudo dentro da janela.
    pub pad: f64,
    /// Desvio lateral da pilula dentro da janela.
    pub pill_margin_x: f64,
    /// Area da pilula a garantir visivel. Tem de cobrir a mensagem mais LONGA, nao a tipica: a
    /// do clipboard com ficheiros tem 65 caracteres, e um erro de provider pode ser maior. Com
    /// 300 essas eram cortadas pela borda direita do ecra, que e precisamente quando ha algo
    /// importante a dizer.
    pub pill_box: (f64, f64),
    /// Tamanho declarado da janela do overlay.
    pub win_logical: (f64, f64),
}

pub const DEFAULT_LAYOUT: Layout = Layout {
    spark: 40.0,
    spark_clamp: 56.0,
    labels_w: 310.0,
    pointer_center: (6.0, 9.0),
    pad: 8.0,
    pill_margin_x: 40.0,
    pill_box: (460.0, 44.0),
    win_logical: (520.0, 140.0),
};

/// O que a overlay esta a mostrar, que e o que decide a caixa a manter visivel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// A brasa. `labels` diz se ha texto a direita dela (nome do projeto, legenda de retry).
    Orb { labels: bool },
    /// Uma pilula de texto (preview, sucesso, erro, hint).
    Pill,
}

/// Tamanho fisico esperado da janela para uma escala. Substitui o `outer_size()`, que na
/// travessia entre monitores com DPI diferente ainda reporta o tamanho do ecra anterior.
pub fn expected_window_physical(scale: f64, l: &Layout) -> (u32, u32) {
    (
        (l.win_logical.0 * scale).round().max(1.0) as u32,
        (l.win_logical.1 * scale).round().max(1.0) as u32,
    )
}

/// Onde esta o conteudo visivel dentro da janela, e que tamanho tem, para a fase atual. Tudo em
/// px fisicos, ainda em f64.
///
/// Existe porque havia DUAS maneiras de clampar (caixa pequena para o orb, janela inteira para a
/// pilula) e a mudanca de fase saltava de uma para a outra: ao aprovar o preview, a janela que
/// estava colocada pela caixa do orb era subitamente contida pela regra da janela inteira e a
/// pilula saltava de sitio. Ha uma regra so, e o que muda entre fases e apenas o tamanho da caixa.
fn content_box(phase: Phase, scale: f64, l: &Layout) -> (f64, f64, f64, f64) {
    let win_h = l.win_logical.1 * scale;
    let pad = l.pad * scale;
    if let Phase::Orb { labels } = phase {
        // A caixa clampada e MAIOR que a faisca (spark_clamp vs spark) porque o rotor cresce no
        // estado de retry: sem esta folga, junto a borda do ecra a orbita inchada saia por fora
        // do que garantimos visivel. O `dx` recua metade da folga para o CENTRO da caixa
        // continuar a ser o mesmo ponto, que e o que ancora a orbita no ponteiro.
        let side = l.spark_clamp * scale;
        let folga = (l.spark_clamp - l.spark) * scale / 2.0;
        // A largura leva tambem a etiqueta do projeto e a legenda de retry, que vivem A DIREITA
        // da faisca (ver Overlay.tsx). Clampar so pela faisca punha-as a comecar exatamente na
        // borda do ecra, invisiveis, e a etiqueta do projeto e a resposta a "com que contexto e
        // que este refine esta a ser feito".
        //
        // SO quando elas existem, e isto foi um defeito real: reservar-lhes 310px sempre fazia a
        // brasa descolar do ponteiro mais de 300px junto a borda direita, num refine sem projeto
        // e sem retry, onde nao havia rigorosamente nada para proteger.
        let extra = if labels { l.labels_w * scale } else { 0.0 };
        (pad - folga, (win_h - side) / 2.0, side + extra, side)
    } else {
        let (bw, bh) = (l.pill_box.0 * scale, l.pill_box.1 * scale);
        (pad + l.pill_margin_x * scale, (win_h - bh) / 2.0, bw, bh)
    }
}

/// Clampa mantendo a CAIXA VISIVEL dentro do monitor (a janela pode ficar pendurada de fora;
/// ninguem a ve, e transparente e ignora cliques). Fonte unica para o seguimento e para a saida
/// do ciclo, que e onde a divergencia dava o salto.
pub fn clamp_window(
    win_x: f64,
    win_y: f64,
    monitor: Rect,
    scale: f64,
    phase: Phase,
    l: &Layout,
) -> (i32, i32) {
    let (dx, dy, cw, ch) = content_box(phase, scale, l);
    let (cx, cy) = (win_x + dx, win_y + dy);
    let (ax, ay) = (monitor.x as f64, monitor.y as f64);
    let max_x = ax + (monitor.w as f64 - cw).max(0.0);
    let max_y = ay + (monitor.h as f64 - ch).max(0.0);
    let cx = cx.clamp(ax, max_x);
    let cy = cy.clamp(ay, max_y);
    ((cx - dx).round() as i32, (cy - dy).round() as i32)
}

/// Top-left desejado da janela do overlay para o cursor atual, ja clampado ao monitor dado.
///
/// O conteudo esta alinhado a esquerda e centrado na vertical (ver Overlay.tsx). Ancoramos o
/// BORDO ESQUERDO do conteudo (nao o centro) junto ao cursor + offset, para o conteudo crescer
/// para a direita: a pilula e larga e, centrada, cairia por cima do rato em vez de aparecer ao
/// lado como o orb.
///
/// UMA ancora para as duas fases: o centro visual do ponteiro. Antes havia duas (faisca centrada
/// no cursor, pilulas ao lado) e elas discordavam, porque a janela NAO se reposiciona quando a
/// fase muda; a pilula herdava o centro da faisca e nascia por cima do cursor.
pub fn overlay_geometry(
    cursor: (f64, f64),
    monitor: Rect,
    scale: f64,
    phase: Phase,
    l: &Layout,
) -> (i32, i32) {
    // O centro NAO e o cursor em si: o `cursor_position` devolve o hotspot, que numa seta e a
    // pontinha de cima-esquerda. `pointer_center` empurra-o para o meio do corpo da seta.
    let anchor_x = cursor.0 + (l.pointer_center.0 - l.spark / 2.0) * scale;
    let anchor_y = cursor.1 + l.pointer_center.1 * scale;
    let win_x = anchor_x - l.pad * scale;
    let win_y = anchor_y - l.win_logical.1 * scale / 2.0;
    clamp_window(win_x, win_y, monitor, scale, phase, l)
}

/// Centro fisico da faisca para uma janela colocada em `win`. Usado nos testes e em logs de
/// diagnostico: e a grandeza que tem de aterrar no ponteiro.
pub fn spark_center(win: (i32, i32), scale: f64, l: &Layout) -> (f64, f64) {
    (
        win.0 as f64 + (l.pad + l.spark / 2.0) * scale,
        win.1 as f64 + l.win_logical.1 * scale / 2.0,
    )
}

/// Monitor que contem o ponto, tipicamente o cursor.
pub fn monitor_at(px: i32, py: i32, monitors: &[Rect]) -> Option<Rect> {
    monitors
        .iter()
        .copied()
        .find(|m| px >= m.x && px < m.x + m.w && py >= m.y && py < m.y + m.h)
}

/// Monitor mais proximo do ponto, por distancia ao retangulo (0 se estiver dentro).
///
/// E o fallback quando `monitor_at` nao encontra nada, e isso acontece a serio: com um 1920x1080
/// encostado a um 2560x1440, o secundario comeca 87px mais abaixo, e a faixa entre o topo dos
/// dois ecras nao pertence a monitor nenhum. Antes caia-se no monitor da JANELA, que durante o
/// seguimento e o de ONDE ELA VEIO: o cursor passava para o outro ecra e a orb ficava colada a
/// fronteira do anterior.
pub fn nearest_monitor(px: i32, py: i32, monitors: &[Rect]) -> Option<Rect> {
    monitors.iter().copied().min_by_key(|m| {
        let dx = (m.x - px).max(0).max(px - (m.x + m.w - 1));
        let dy = (m.y - py).max(0).max(py - (m.y + m.h - 1));
        (dx as i64) * (dx as i64) + (dy as i64) * (dy as i64)
    })
}

/// Monitor a usar para o ponto: o que o contem, senao o mais proximo. O `bool` diz se veio do
/// fallback, para o shell poder logar isso (antes era invisivel).
pub fn monitor_for_point(px: i32, py: i32, monitors: &[Rect]) -> Option<(Rect, bool)> {
    match monitor_at(px, py, monitors) {
        Some(m) => Some((m, false)),
        None => nearest_monitor(px, py, monitors).map(|m| (m, true)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Layout REAL da maquina onde o bug foi reportado: primario 2560x1440 em (0,0), secundario
    // 1920x1080 em (2560, 87). O desalinhamento vertical de 87px nao e detalhe: e a faixa que
    // nao pertence a monitor nenhum e que fazia o fallback disparar.
    const PRIMARY: Rect = Rect::new(0, 0, 2560, 1440);
    const SECOND: Rect = Rect::new(2560, 87, 1920, 1080);
    const L: &Layout = &DEFAULT_LAYOUT;

    #[test]
    fn spark_center_lands_on_the_pointer_center_at_every_scale() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let cursor = (1200.0, 700.0);
            let win = overlay_geometry(cursor, PRIMARY, scale, Phase::Orb { labels: true }, L);
            let (sx, sy) = spark_center(win, scale, L);
            // A meio do ecra nada e clampado: o centro da faisca tem de cair exatamente no
            // centro visual do ponteiro, a menos do arredondamento unico.
            assert!(
                (sx - (cursor.0 + L.pointer_center.0 * scale)).abs() <= 1.0,
                "scale {scale}: x {sx}"
            );
            assert!(
                (sy - (cursor.1 + L.pointer_center.1 * scale)).abs() <= 1.0,
                "scale {scale}: y {sy}"
            );
        }
    }

    #[test]
    fn cursor_on_the_second_monitor_stays_on_the_second_monitor() {
        let win = overlay_geometry((3500.0, 600.0), SECOND, 1.0, Phase::Orb { labels: true }, L);
        assert!(
            win.0 >= SECOND.x,
            "a janela caiu para o monitor primario: {win:?}"
        );
        let (sx, _) = spark_center(win, 1.0, L);
        assert!((sx - 3506.0).abs() <= 1.0, "faisca em {sx}");
    }

    #[test]
    fn second_monitor_uses_its_own_scale_not_the_primary_one() {
        // Mesmo cursor, escalas diferentes: os offsets crescem com a escala do monitor do
        // CURSOR. Se o shell passar a escala da janela (o bug), a faisca descentra-se.
        let a = overlay_geometry((3500.0, 600.0), SECOND, 1.0, Phase::Orb { labels: true }, L);
        let b = overlay_geometry((3500.0, 600.0), SECOND, 1.5, Phase::Orb { labels: true }, L);
        assert_ne!(a, b);
        let (ax, _) = spark_center(a, 1.0, L);
        let (bx, _) = spark_center(b, 1.5, L);
        assert!((ax - 3506.0).abs() <= 1.0);
        assert!((bx - 3509.0).abs() <= 1.0); // 3500 + 6*1.5
    }

    #[test]
    fn orb_box_stays_visible_at_the_right_edge_of_the_second_monitor() {
        let scale = 1.0;
        let win = overlay_geometry(
            (4479.0, 600.0),
            SECOND,
            scale,
            Phase::Orb { labels: true },
            L,
        );
        let (dx, _, cw, _) = content_box(Phase::Orb { labels: true }, scale, L);
        let right = win.0 as f64 + dx + cw;
        assert!(
            right <= (SECOND.x + SECOND.w) as f64 + 0.5,
            "conteudo sai pela direita: {right}"
        );
    }

    #[test]
    fn pill_box_stays_visible_at_the_bottom_right_corner() {
        let scale = 1.0;
        let win = overlay_geometry((2540.0, 1430.0), PRIMARY, scale, Phase::Pill, L);
        let (dx, dy, cw, ch) = content_box(Phase::Pill, scale, L);
        assert!(win.0 as f64 + dx + cw <= (PRIMARY.x + PRIMARY.w) as f64 + 0.5);
        assert!(win.1 as f64 + dy + ch <= (PRIMARY.y + PRIMARY.h) as f64 + 0.5);
    }

    #[test]
    fn one_pixel_of_cursor_moves_the_window_by_at_most_one_pixel() {
        // Guarda contra o duplo arredondamento: com o `pad` e cada offset da caixa arredondados
        // em separado, a 125% havia saltos de 2px a cada poucos pixeis de rato.
        for scale in [1.0, 1.25, 1.5] {
            let mut prev = overlay_geometry(
                (1000.0, 500.0),
                PRIMARY,
                scale,
                Phase::Orb { labels: true },
                L,
            )
            .0;
            for i in 1..200 {
                let win = overlay_geometry(
                    (1000.0 + i as f64, 500.0),
                    PRIMARY,
                    scale,
                    Phase::Orb { labels: true },
                    L,
                )
                .0;
                let d = win - prev;
                assert!((0..=1).contains(&d), "scale {scale}: salto de {d}px");
                prev = win;
            }
        }
    }

    #[test]
    fn monitor_for_point_finds_the_gap_above_the_second_monitor() {
        let mons = [PRIMARY, SECOND];
        // (3000, 40) esta a direita do primario e ACIMA do secundario: nao pertence a nenhum.
        assert!(monitor_at(3000, 40, &mons).is_none());
        let (m, fallback) = monitor_for_point(3000, 40, &mons).unwrap();
        assert_eq!(m, SECOND);
        assert!(fallback);
    }

    #[test]
    fn monitor_for_point_prefers_containment_over_proximity() {
        let mons = [PRIMARY, SECOND];
        let (m, fallback) = monitor_for_point(3500, 600, &mons).unwrap();
        assert_eq!(m, SECOND);
        assert!(!fallback);
        let (m, _) = monitor_for_point(10, 10, &mons).unwrap();
        assert_eq!(m, PRIMARY);
    }

    #[test]
    fn monitor_for_point_on_an_empty_list_is_none() {
        assert!(monitor_for_point(0, 0, &[]).is_none());
    }

    #[test]
    fn expected_window_physical_scales_the_declared_size() {
        assert_eq!(expected_window_physical(1.0, L), (520, 140));
        assert_eq!(expected_window_physical(1.25, L), (650, 175));
        assert_eq!(expected_window_physical(1.5, L), (780, 210));
    }

    #[test]
    fn reserving_label_width_with_no_labels_tore_the_ember_off_the_pointer() {
        // O defeito: reservava-se sempre a largura das etiquetas, e junto a borda direita isso
        // empurrava a janela centenas de pixeis para a esquerda mesmo sem etiqueta nenhuma para
        // proteger. A brasa aparecia longe do rato, que e o unico sitio onde ela faz sentido.
        let cursor = (2400.0, 700.0);
        let sem = overlay_geometry(cursor, PRIMARY, 1.0, Phase::Orb { labels: false }, L);
        let com = overlay_geometry(cursor, PRIMARY, 1.0, Phase::Orb { labels: true }, L);
        let (sx, _) = spark_center(sem, 1.0, L);
        assert!(
            (sx - (cursor.0 + L.pointer_center.0)).abs() <= 1.0,
            "sem etiquetas a brasa devia estar no ponteiro, esta em {sx}"
        );
        assert!(
            com.0 <= sem.0 - 150,
            "com etiquetas TEM de recuar para as manter visiveis: sem={sem:?} com={com:?}"
        );
    }

    #[test]
    fn at_the_very_edge_the_ember_still_recedes_enough_to_stay_on_screen() {
        // O recuo nao desaparece, so deixa de ser gratuito: com o cursor colado a borda, a
        // propria brasa nao cabe e tem de vir para dentro.
        let win = overlay_geometry(
            (2556.0, 700.0),
            PRIMARY,
            1.0,
            Phase::Orb { labels: false },
            L,
        );
        let (dx, _, cw, _) = content_box(Phase::Orb { labels: false }, 1.0, L);
        assert!(win.0 as f64 + dx + cw <= (PRIMARY.x + PRIMARY.w) as f64 + 0.5);
    }

    #[test]
    fn clamp_window_is_idempotent() {
        // O ciclo de seguimento re-clampa a posicao que ele proprio ja clampou (na saida). Se
        // isto nao fosse idempotente, cada saida deslocava a pilula um bocado.
        let once = clamp_window(-500.0, -500.0, PRIMARY, 1.0, Phase::Pill, L);
        let twice = clamp_window(once.0 as f64, once.1 as f64, PRIMARY, 1.0, Phase::Pill, L);
        assert_eq!(once, twice);
    }
}
