//! Politica de atalhos: o que se pode registar, e o que o sistema operativo ja reclamou.
//! Pura e sem SO (o registo real vive no shell), como o resto de `ember-core`.
//!
//! Porque existe: ate aqui a unica defesa contra um atalho ocupado era tentar regista-lo e ver
//! se o SO recusava. No Windows chega, porque o `RegisterHotKey` falha mesmo. No macOS nao: os
//! atalhos do proprio sistema (Cmd+Espaco do Spotlight, Cmd+Tab) deixam-se registar e depois
//! ganham em silencio, portanto o utilizador gravava um atalho que nunca ia disparar e nao
//! havia nada, em lado nenhum, a dizer-lhe porque. A lista abaixo cobre os casos que magoam;
//! o aviso na UI cobre o resto, porque uma lista escrita a mao nunca esta completa.

use serde::{Deserialize, Serialize};

/// O sistema onde o atalho vai ser registado. Explicito e nao lido do ambiente para os testes
/// poderem exercer as duas plataformas a partir de qualquer maquina.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Os {
    Windows,
    MacOs,
    Other,
}

/// Veredicto sobre uma combinacao, antes de se tentar sequer registar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HotkeyVerdict {
    /// Nada a apontar aqui. Falta o teste real de registo, que o shell faz a seguir.
    Available,
    /// Reservada pelo sistema operativo. `owner` e o que a usa, para a mensagem poder dizer
    /// "o Spotlight ja usa isto" em vez de um "invalido" que nao ajuda ninguem.
    ReservedByOs { owner: String },
    /// Ja atribuida a outro atalho do proprio Ember. `slot` = "main" | "polish" | "turbo".
    UsedByEmber { slot: String },
    /// Sem tecla principal (so modificadores), ou vazia.
    Incomplete,
}

/// Atalhos que o Windows reclama para si. O `RegisterHotKey` recusa a maior parte destes com um
/// erro, mas nao todos, e chegar aqui antes da tentativa da uma mensagem melhor.
const WINDOWS_RESERVED: &[(&str, &str)] = &[
    ("ctrl+alt+delete", "Windows security screen"),
    ("ctrl+shift+escape", "Task Manager"),
    ("ctrl+escape", "Start menu"),
    ("alt+tab", "Windows app switcher"),
    ("alt+escape", "Windows window cycling"),
    ("super+l", "Windows lock screen"),
    ("super+d", "Windows show desktop"),
    ("super+tab", "Task View"),
    ("super+space", "Windows input language switch"),
    ("super+r", "Windows Run dialog"),
    ("super+e", "File Explorer"),
];

/// Atalhos que o macOS reclama para si. Ao contrario do Windows, o registo destes NAO falha: o
/// sistema simplesmente ganha, e a app fica com um atalho morto sem sinal nenhum. Esta lista e
/// a unica coisa entre o utilizador e esse silencio, e por isso e mais completa do que a do
/// Windows. Envelhece quando a Apple mexe nos defaults; ver o aviso na seccao de atalhos.
const MACOS_RESERVED: &[(&str, &str)] = &[
    ("cmdorctrl+space", "Spotlight"),
    ("cmdorctrl+alt+space", "Finder search window"),
    ("control+space", "macOS input source switch"),
    ("cmdorctrl+tab", "macOS app switcher"),
    ("cmdorctrl+alt+escape", "Force Quit"),
    ("cmdorctrl+control+q", "macOS lock screen"),
    ("cmdorctrl+shift+3", "macOS screenshot"),
    ("cmdorctrl+shift+4", "macOS screenshot"),
    ("cmdorctrl+shift+5", "macOS screenshot"),
    ("control+up", "Mission Control"),
    ("control+down", "App Expose"),
    ("control+left", "macOS space switching"),
    ("control+right", "macOS space switching"),
    ("cmdorctrl+h", "Hide application"),
    ("cmdorctrl+q", "Quit application"),
];

/// Modificadores conhecidos, em ordem CANONICA. Comparar atalhos por string crua nao funciona:
/// "Shift+CmdOrCtrl+Space" e "CmdOrCtrl+Shift+Space" sao o mesmo atalho e nunca seriam vistos
/// como iguais. Ordenar por esta tabela resolve isso.
const MODIFIER_ORDER: &[&str] = &["cmdorctrl", "control", "super", "alt", "shift"];

/// Sinonimos que as varias camadas (Tauri, teclado, nos) usam para a mesma tecla.
fn canonical_modifier(token: &str) -> Option<&'static str> {
    match token {
        "cmdorctrl" | "cmd" | "command" | "meta" => Some("cmdorctrl"),
        "ctrl" | "control" => Some("control"),
        "super" | "win" | "windows" => Some("super"),
        "alt" | "option" => Some("alt"),
        "shift" => Some("shift"),
        _ => None,
    }
}

/// Forma canonica de um acelerador: minusculas, modificadores por ordem fixa, tecla no fim.
/// Devolve `None` se nao houver tecla principal (so modificadores) ou se estiver vazio.
///
/// Nota sobre o Windows: aqui `control` e `cmdorctrl` ficam SEPARADOS de proposito, apesar de
/// no Windows serem a mesma tecla fisica. E o shell que resolve isso quando compara; manter a
/// distincao permite a lista do macOS falar de Cmd e de Control como coisas diferentes, que la
/// e o que sao.
pub fn canonical(accel: &str) -> Option<String> {
    let mut mods: Vec<&'static str> = Vec::new();
    let mut key: Option<String> = None;
    for raw in accel.split('+') {
        let token = raw.trim().to_ascii_lowercase();
        if token.is_empty() {
            continue;
        }
        match canonical_modifier(&token) {
            Some(m) => {
                if !mods.contains(&m) {
                    mods.push(m);
                }
            }
            // Duas teclas principais numa so combinacao nao existe: fica a ultima, que e o que
            // o utilizador acabou de carregar.
            None => key = Some(token),
        }
    }
    let key = key?;
    mods.sort_by_key(|m| MODIFIER_ORDER.iter().position(|x| x == m).unwrap_or(usize::MAX));
    let mut out = mods.join("+");
    if !out.is_empty() {
        out.push('+');
    }
    out.push_str(&key);
    Some(out)
}

/// No Windows, Ctrl e CmdOrCtrl sao a MESMA tecla fisica, por isso "Control+Space" e
/// "CmdOrCtrl+Space" colidem. No macOS sao teclas diferentes e nao colidem. Achatar so onde
/// deve ser achatado evita tanto o falso positivo como o falso negativo.
fn for_comparison(canon: &str, os: Os) -> String {
    match os {
        Os::MacOs => canon.to_string(),
        _ => canon.replace("control", "cmdorctrl"),
    }
}

/// Duas combinacoes sao o mesmo atalho neste sistema?
pub fn same_hotkey(a: &str, b: &str, os: Os) -> bool {
    match (canonical(a), canonical(b)) {
        (Some(x), Some(y)) => for_comparison(&x, os) == for_comparison(&y, os),
        _ => false,
    }
}

/// Avalia uma combinacao contra o SO e contra os atalhos que o Ember ja tem atribuidos.
///
/// `taken` sao os slots ja ocupados, como (slot, acelerador). O slot que esta a ser editado NAO
/// deve vir nesta lista, senao gravar a mesma combinacao que ja la estava acusava conflito
/// consigo propria.
pub fn evaluate(accel: &str, os: Os, taken: &[(&str, &str)]) -> HotkeyVerdict {
    let Some(canon) = canonical(accel) else {
        return HotkeyVerdict::Incomplete;
    };
    let mine = for_comparison(&canon, os);

    let reserved = match os {
        Os::Windows => WINDOWS_RESERVED,
        Os::MacOs => MACOS_RESERVED,
        Os::Other => &[],
    };
    for (pattern, owner) in reserved {
        if canonical(pattern).is_some_and(|p| for_comparison(&p, os) == mine) {
            return HotkeyVerdict::ReservedByOs {
                owner: (*owner).to_string(),
            };
        }
    }
    for (slot, other) in taken {
        if !other.trim().is_empty() && same_hotkey(accel, other, os) {
            return HotkeyVerdict::UsedByEmber {
                slot: (*slot).to_string(),
            };
        }
    }
    HotkeyVerdict::Available
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_ignores_the_order_the_modifiers_were_pressed_in() {
        // O utilizador pode carregar Shift antes de Ctrl. E o mesmo atalho, e comparar as
        // strings cruas dizia que nao era.
        assert_eq!(
            canonical("Shift+CmdOrCtrl+Space"),
            canonical("CmdOrCtrl+Shift+Space")
        );
        assert!(same_hotkey("Shift+CmdOrCtrl+Space", "CmdOrCtrl+Shift+Space", Os::Windows));
    }

    #[test]
    fn canonical_accepts_the_synonyms_the_layers_disagree_on() {
        // Tauri, o teclado e nos escrevemos a mesma tecla de maneiras diferentes.
        assert_eq!(canonical("Cmd+Space"), canonical("CmdOrCtrl+Space"));
        assert_eq!(canonical("Win+D"), canonical("Super+D"));
        assert_eq!(canonical("Option+A"), canonical("Alt+A"));
        assert_eq!(canonical("Ctrl+A"), canonical("Control+A"));
    }

    #[test]
    fn a_single_key_with_no_modifier_is_a_valid_hotkey() {
        // Pedido explicito: de uma tecla ate quatro. Uma tecla sozinha e legitima (F13, por
        // exemplo, existe precisamente para isto).
        assert_eq!(canonical("F13").as_deref(), Some("f13"));
        assert_eq!(canonical("Space").as_deref(), Some("space"));
        assert_eq!(evaluate("F13", Os::Windows, &[]), HotkeyVerdict::Available);
    }

    #[test]
    fn four_key_combinations_survive_canonicalisation() {
        assert_eq!(
            canonical("Shift+Alt+CmdOrCtrl+Space").as_deref(),
            Some("cmdorctrl+alt+shift+space")
        );
    }

    #[test]
    fn only_modifiers_is_incomplete_not_available() {
        // Enquanto so ha modificadores premidos ainda nao ha atalho nenhum para avaliar.
        assert_eq!(canonical("CmdOrCtrl+Shift"), None);
        assert_eq!(evaluate("CmdOrCtrl+Shift", Os::Windows, &[]), HotkeyVerdict::Incomplete);
        assert_eq!(evaluate("", Os::Windows, &[]), HotkeyVerdict::Incomplete);
    }

    #[test]
    fn windows_reserved_combinations_are_named_not_just_refused() {
        // "Invalido" nao ajuda ninguem; "o Gestor de Tarefas ja usa isto" ajuda.
        assert_eq!(
            evaluate("CmdOrCtrl+Shift+Escape", Os::Windows, &[]),
            HotkeyVerdict::ReservedByOs { owner: "Task Manager".into() }
        );
        assert_eq!(
            evaluate("Super+L", Os::Windows, &[]),
            HotkeyVerdict::ReservedByOs { owner: "Windows lock screen".into() }
        );
    }

    #[test]
    fn macos_system_shortcuts_are_caught_because_the_os_will_not_refuse_them() {
        // Este e o caso todo: no macOS o registo do Cmd+Espaco PASSA e depois o Spotlight ganha
        // em silencio. Sem esta lista, o utilizador gravava um atalho morto sem saber porque.
        assert_eq!(
            evaluate("CmdOrCtrl+Space", Os::MacOs, &[]),
            HotkeyVerdict::ReservedByOs { owner: "Spotlight".into() }
        );
        assert_eq!(
            evaluate("Control+Up", Os::MacOs, &[]),
            HotkeyVerdict::ReservedByOs { owner: "Mission Control".into() }
        );
    }

    #[test]
    fn ctrl_and_cmd_are_the_same_key_on_windows_and_different_on_macos() {
        // No Windows, Ctrl+Espaco e CmdOrCtrl+Espaco sao a mesma tecla fisica: colidem.
        assert!(same_hotkey("Control+Space", "CmdOrCtrl+Space", Os::Windows));
        // No macOS sao teclas diferentes, e tratar-lhes como iguais recusaria um atalho valido.
        assert!(!same_hotkey("Control+Space", "CmdOrCtrl+Space", Os::MacOs));
        // E por isso que o Control+Espaco do macOS (troca de teclado) nao acusa no Windows.
        assert_eq!(evaluate("Control+Space", Os::Windows, &[]), HotkeyVerdict::Available);
    }

    #[test]
    fn a_combination_already_given_to_another_ember_slot_is_caught() {
        let taken = [("main", "CmdOrCtrl+Shift+Space"), ("turbo", "CmdOrCtrl+F9")];
        assert_eq!(
            evaluate("Shift+CmdOrCtrl+Space", Os::Windows, &taken),
            HotkeyVerdict::UsedByEmber { slot: "main".into() }
        );
        assert_eq!(
            evaluate("CmdOrCtrl+F10", Os::Windows, &taken),
            HotkeyVerdict::Available
        );
    }

    #[test]
    fn the_slot_being_edited_does_not_conflict_with_itself() {
        // Regravar a MESMA combinacao no mesmo slot tem de passar. O caller tira o slot que
        // esta a editar da lista; este teste pina esse contrato.
        let taken = [("turbo", "CmdOrCtrl+F9")];
        assert_eq!(
            evaluate("CmdOrCtrl+Shift+Space", Os::Windows, &taken),
            HotkeyVerdict::Available
        );
    }

    #[test]
    fn empty_slots_never_count_as_taken() {
        let taken = [("polish", ""), ("turbo", "   ")];
        assert_eq!(
            evaluate("CmdOrCtrl+F9", Os::Windows, &taken),
            HotkeyVerdict::Available
        );
    }
}
