//! Guardas fail-safe: so DECIDEM recusar colar, nunca reescrevem. Um Polish quase-identico e
//! sucesso, nao falha (nao ha guarda de "no-op"). Puro.

use super::mask::SpanTable;

/// `true` se o texto e efetivamente vazio (so espacos/quebras). Depois de limpar, um output
/// assim nunca deve ser colado por cima da seleccao.
pub fn is_effectively_empty(s: &str) -> bool {
    s.trim().is_empty()
}

/// `true` se TODOS os tokens de span continuam presentes no output do modelo. Um token em falta
/// significa que o modelo deitou fora (ou mutou) um pedaco de codigo/URL: nao da para restaurar
/// intacto, por isso degrada em vez de colar codigo partido.
pub fn check_preservation(table: &SpanTable, output: &str) -> bool {
    table.tokens().all(|t| output.contains(t))
}

#[cfg(test)]
mod tests {
    use super::super::mask::{mask, scan_spans};
    use super::*;

    #[test]
    fn empty_and_whitespace_are_effectively_empty() {
        assert!(is_effectively_empty(""));
        assert!(is_effectively_empty("   \n\t "));
        assert!(!is_effectively_empty("x"));
    }

    #[test]
    fn preservation_passes_when_all_tokens_present() {
        let input = "run https://e.com/a and ```code```";
        let (_, table) = mask(input, &scan_spans(input));
        let out: String = table.tokens().collect::<Vec<_>>().join(" kept ");
        assert!(check_preservation(&table, &out));
    }

    #[test]
    fn preservation_fails_when_a_token_is_dropped() {
        let input = "run https://e.com/a now";
        let (_, table) = mask(input, &scan_spans(input));
        assert!(!check_preservation(&table, "run the command now"));
    }

    #[test]
    fn preservation_trivially_passes_with_no_spans() {
        let table = SpanTable::default();
        assert!(check_preservation(&table, "any prose is fine"));
    }
}
