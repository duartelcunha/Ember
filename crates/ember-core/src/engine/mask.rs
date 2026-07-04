//! Mascara os pedacos que o modelo nao deve tocar (blocos de codigo em fence e URLs de esquema
//! completo) por tokens opacos `{{EMBER_SPAN_n}}`, e desmascara no fim. So estes dois tipos:
//! mascarar caminhos, codigo inline ou placeholders soltos (`<x>`, `%s`) sobre-mascarava a
//! prosa e tirava ao modelo o contexto de que precisa. Puro.

/// Um pedaco a preservar, por intervalo de bytes no input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Tabela ordenada token -> texto original. Os tokens sao unicos e nao colidem por prefixo
/// (o `}}` de fecho impede `{{EMBER_SPAN_1}}` de casar dentro de `{{EMBER_SPAN_10}}`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpanTable {
    entries: Vec<(String, String)>,
}

impl SpanTable {
    pub fn tokens(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(t, _)| t.as_str())
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

fn token(n: usize) -> String {
    format!("{{{{EMBER_SPAN_{n}}}}}")
}

/// Encontra os spans a preservar: primeiro os blocos em fence (por linha), depois os URLs FORA
/// das fences. Devolvidos ordenados por inicio, sem sobreposicao.
pub fn scan_spans(input: &str) -> Vec<Span> {
    let mut fences: Vec<Span> = Vec::new();
    let mut idx = 0usize;
    let mut open: Option<usize> = None;
    for line in input.split_inclusive('\n') {
        let line_start = idx;
        let is_fence = line.trim_start().starts_with("```");
        if is_fence {
            match open {
                None => open = Some(line_start),
                Some(start) => {
                    fences.push(Span {
                        start,
                        end: line_start + line.len(),
                    });
                    open = None;
                }
            }
        }
        idx += line.len();
    }
    // Fence por fechar: ignora (nao mascara meia fence).

    let in_fence = |pos: usize| fences.iter().any(|s| pos >= s.start && pos < s.end);
    let mut spans = fences.clone();
    for u in find_urls(input) {
        if !in_fence(u.start) {
            spans.push(u);
        }
    }
    spans.sort_by_key(|s| s.start);
    spans
}

/// URLs http(s) de esquema completo. Corta em whitespace/delimitador e apara pontuacao final
/// de frase. Heuristica: mesmo que a fronteira seja imperfeita, o unmask repoe exatamente o que
/// foi mascarado, por isso nao ha corrupcao (so o que fica de fora fica exposto ao modelo).
fn find_urls(input: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < input.len() {
        let rest = &input[i..];
        if rest.starts_with("http://") || rest.starts_with("https://") {
            let start = i;
            let mut end = input.len();
            for (off, ch) in rest.char_indices() {
                if ch.is_whitespace()
                    || matches!(ch, '<' | '>' | '"' | '\'' | '`' | '|' | '\\' | '^' | '{' | '}')
                {
                    end = start + off;
                    break;
                }
            }
            // Apara pontuacao de frase que quase de certeza nao faz parte do URL.
            while end > start {
                let last = input[start..end].chars().last().unwrap();
                if matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']') {
                    end -= last.len_utf8();
                } else {
                    break;
                }
            }
            if end > start + "https://".len() {
                out.push(Span { start, end });
                i = end;
                continue;
            }
        }
        i += rest.chars().next().map(char::len_utf8).unwrap_or(1);
    }
    out
}

/// `true` se o input inteiro (aparado) e exatamente UMA fence de codigo. Nesse caso um output
/// em fence e legitimo e `strip_structural` nao o desembrulha.
pub fn is_single_fence(input: &str, spans: &[Span]) -> bool {
    if spans.len() != 1 {
        return false;
    }
    let s = spans[0];
    let text = &input[s.start..s.end];
    text.trim_start().starts_with("```")
        && input[..s.start].trim().is_empty()
        && input[s.end..].trim().is_empty()
}

/// Substitui cada span por um token e devolve o texto mascarado + a tabela.
pub fn mask(input: &str, spans: &[Span]) -> (String, SpanTable) {
    let mut out = String::with_capacity(input.len());
    let mut table = SpanTable::default();
    let mut last = 0usize;
    for span in spans {
        if span.start < last {
            continue; // defensivo: ignora sobreposicoes
        }
        out.push_str(&input[last..span.start]);
        let tok = token(table.entries.len());
        out.push_str(&tok);
        table
            .entries
            .push((tok, input[span.start..span.end].to_string()));
        last = span.end;
    }
    out.push_str(&input[last..]);
    (out, table)
}

/// Repoe cada token pelo texto original.
pub fn unmask(text: &str, table: &SpanTable) -> String {
    let mut out = text.to_string();
    for (tok, original) in &table.entries {
        out = out.replace(tok.as_str(), original);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_then_unmask_is_identity_when_body_unchanged() {
        let input = "see ```code``` and https://x.com/y here";
        let spans = scan_spans(input);
        let (masked, table) = mask(input, &spans);
        assert_eq!(unmask(&masked, &table), input);
    }

    #[test]
    fn fenced_code_and_urls_are_detected() {
        let input = "a\n```rust\nfn x(){}\n```\nb https://ex.com/p c";
        let spans = scan_spans(input);
        assert_eq!(spans.len(), 2);
        let (masked, _) = mask(input, &spans);
        assert!(!masked.contains("fn x(){}"));
        assert!(!masked.contains("https://ex.com/p"));
        assert!(masked.contains("EMBER_SPAN_0"));
        assert!(masked.contains("EMBER_SPAN_1"));
    }

    #[test]
    fn prose_placeholders_are_not_masked() {
        // Guarda a regressao de sobre-mascarar: <x>, %s, $X e caminhos nao viram spans.
        let input = "use <name> and %s and $VAR and ./path/to/file";
        let spans = scan_spans(input);
        assert!(spans.is_empty());
    }

    #[test]
    fn tokens_do_not_collide_by_prefix() {
        // 11 spans forca EMBER_SPAN_1 e EMBER_SPAN_10: o unmask nao pode confundi-los.
        let urls: Vec<String> = (0..11).map(|n| format!("https://e.com/{n}")).collect();
        let input = urls.join(" x ");
        let spans = scan_spans(&input);
        assert_eq!(spans.len(), 11);
        let (masked, table) = mask(&input, &spans);
        assert_eq!(unmask(&masked, &table), input);
    }

    #[test]
    fn is_single_fence_detects_whole_input_fence() {
        let input = "```py\nprint(1)\n```";
        let spans = scan_spans(input);
        assert!(is_single_fence(input, &spans));
        // Com prosa a envolver, ja nao e single fence.
        let input2 = "do this:\n```py\nprint(1)\n```";
        assert!(!is_single_fence(input2, &scan_spans(input2)));
    }

    #[test]
    fn unclosed_fence_is_not_masked() {
        let input = "```rust\nfn x(){}\n"; // sem fecho
        assert!(scan_spans(input).is_empty());
    }
}
