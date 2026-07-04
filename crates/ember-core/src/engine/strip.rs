//! Limpeza estrutural do output do modelo. SO estrutural: desembrulha uma unica fence exterior
//! que o modelo adicionou, tira aspas exteriores e marcadores ecoados. Nada de remocao de
//! preambulo por regex multilingue (o preambulo sairia na lingua do output, uma regex inglesa
//! nao o apanharia e arriscava comer uma primeira linha legitima). Puro.

use crate::prompt::{INPUT_CLOSE, INPUT_OPEN};

/// Limpa ruido estrutural do texto cru do modelo. `input_was_single_fence`: se o proprio input
/// era uma fence, um output em fence e legitimo e nao se desembrulha.
pub fn strip_structural(raw: &str, input_was_single_fence: bool) -> String {
    // Marcadores ecoados, em qualquer sitio.
    let mut s = raw.replace(INPUT_OPEN, "").replace(INPUT_CLOSE, "");
    s = s.trim().to_string();
    if !input_was_single_fence {
        if let Some(inner) = unwrap_single_fence(&s) {
            s = inner.trim().to_string();
        }
    }
    unwrap_wrapping_quotes(&s)
}

/// Se o texto INTEIRO e uma unica fence (primeira linha abre, ultima fecha, so essas duas
/// fences), devolve o conteudo interior. Senao `None` (prosa + fence interna nao se toca).
fn unwrap_single_fence(s: &str) -> Option<String> {
    let lines: Vec<&str> = s.lines().collect();
    if lines.len() < 2 {
        return None;
    }
    let first_is_fence = lines[0].trim_start().starts_with("```");
    let last_is_fence = lines[lines.len() - 1].trim() == "```";
    if !first_is_fence || !last_is_fence {
        return None;
    }
    let fence_count = lines
        .iter()
        .filter(|l| l.trim_start().starts_with("```"))
        .count();
    if fence_count != 2 {
        return None;
    }
    Some(lines[1..lines.len() - 1].join("\n"))
}

/// Tira UM par de aspas exteriores (", ', ou `) que envolva o texto todo, so se a mesma aspa
/// nao voltar a aparecer dentro (senao era conteudo real, nao um wrap).
fn unwrap_wrapping_quotes(s: &str) -> String {
    let first = s.chars().next();
    let last = s.chars().last();
    if let (Some(f), Some(l)) = (first, last) {
        if f == l && matches!(f, '"' | '\'' | '`') && s.chars().count() >= 2 {
            let inner = &s[f.len_utf8()..s.len() - l.len_utf8()];
            if !inner.contains(f) {
                return inner.trim().to_string();
            }
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_single_outer_fence() {
        assert_eq!(strip_structural("```\nhello\n```", false), "hello");
        assert_eq!(strip_structural("```json\n{\"a\":1}\n```", false), "{\"a\":1}");
    }

    #[test]
    fn keeps_fence_when_input_was_a_fence() {
        assert_eq!(strip_structural("```\nhello\n```", true), "```\nhello\n```");
    }

    #[test]
    fn does_not_strip_prose_with_an_internal_fence() {
        // Primeira linha e prosa: nao ha fence exterior a desembrulhar.
        let s = "Here is code:\n```\nx=1\n```";
        assert_eq!(strip_structural(s, false), s);
    }

    #[test]
    fn strips_wrapping_quotes_only_when_clean() {
        assert_eq!(strip_structural("\"Hello there\"", false), "Hello there");
        // Aspas que reaparecem dentro sao conteudo: nao se tira.
        let s = "\"a\" and \"b\"";
        assert_eq!(strip_structural(s, false), s);
    }

    #[test]
    fn strips_echoed_markers() {
        assert_eq!(
            strip_structural("[EMBER_INPUT]\nHello\n[/EMBER_INPUT]", false),
            "Hello"
        );
    }

    #[test]
    fn leaves_a_legitimate_first_line_alone() {
        let s = "Summarize the following report and list three risks.";
        assert_eq!(strip_structural(s, false), s);
    }
}
