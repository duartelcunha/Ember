//! Preserve code fences, inline code and URLs before any text normalization.

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
    let mut open: Option<(usize, u8, usize)> = None;
    for line in input.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let delimiter = trimmed.as_bytes().first().copied().unwrap_or(0);
        let count = trimmed.bytes().take_while(|b| *b == delimiter).count();
        if matches!(delimiter, b'`' | b'~') && count >= 3 {
            match open {
                None => open = Some((idx, delimiter, count)),
                Some((start, kind, width))
                    if delimiter == kind
                        && count >= width
                        && trimmed[count..].trim().is_empty() =>
                {
                    fences.push(Span {
                        start,
                        end: idx + line.len(),
                    });
                    open = None;
                }
                _ => {}
            }
        }
        idx += line.len();
    }
    // Incomplete code is still code. Preserve to EOF instead of exposing it to cleanup.
    if let Some((start, _, _)) = open {
        fences.push(Span {
            start,
            end: input.len(),
        });
    }
    // Inline code and quoted commands can include literal backticks via longer delimiters.
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(span) = fences.iter().find(|s| i >= s.start && i < s.end) {
            i = span.end;
            continue;
        }
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let width = bytes[i..].iter().take_while(|b| **b == b'`').count();
        let mut end = i + width;
        let mut found = None;
        while end < bytes.len() {
            if bytes[end] == b'`' {
                let closing = bytes[end..].iter().take_while(|b| **b == b'`').count();
                if closing == width {
                    found = Some(end + width);
                    break;
                }
                end += closing;
            } else {
                end += 1;
            }
        }
        if let Some(end) = found {
            if !fences.iter().any(|s| i < s.end && end > s.start) {
                fences.push(Span { start: i, end });
            }
            i = end;
        } else {
            i += width;
        }
    }
    let in_fence = |pos: usize| fences.iter().any(|s| pos >= s.start && pos < s.end);
    let mut spans = fences.clone();
    for u in find_urls(input) {
        if !in_fence(u.start) {
            spans.push(u);
        }
    }
    for technical in find_technical_spans(input) {
        if !spans
            .iter()
            .any(|s| technical.start < s.end && technical.end > s.start)
        {
            spans.push(technical);
        }
    }
    // Literal marker syntax belongs to the user's text, never to our generated namespace.
    for (start, _) in input.match_indices("{{EMBER_SPAN_") {
        if let Some(close) = input[start..].find("}}") {
            let end = start + close + 2;
            if !spans.iter().any(|s| start < s.end && end > s.start) {
                spans.push(Span { start, end });
            }
        }
    }
    spans.sort_by_key(|s| s.start);
    spans
}

fn find_technical_spans(input: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut offset = 0;
    for line in input.split_inclusive('\n') {
        let text = line.trim_start();
        // Only explicit shell prompts identify an entire command line. Ordinary prose naming
        // a command is still editable; backticks can protect ambiguous command fragments.
        if text.starts_with("$ ") || (text.starts_with("PS ") && text.contains("> ")) {
            spans.push(Span {
                start: offset,
                end: offset + line.len(),
            });
        }
        offset += line.len();
    }
    for (start, _) in input.char_indices() {
        let previous = input[..start].chars().next_back();
        if previous.is_some_and(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '(')) {
            continue;
        }
        let rest = &input[start..];
        let bytes = rest.as_bytes();
        let drive = bytes.len() > 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/');
        let relative = rest.starts_with("./") || rest.starts_with("../") || rest.starts_with("~/");
        let absolute =
            rest.starts_with('/') && bytes.get(1).is_some_and(|c| !c.is_ascii_whitespace());
        if !drive && !relative && !absolute && !rest.starts_with("\\\\") {
            continue;
        }
        let quote = previous.filter(|c| matches!(c, '"' | '\''));
        let end = if let Some(quote) = quote {
            rest.find(quote).map(|i| start + i)
        } else {
            Some(
                start
                    + rest
                        .find(|c: char| {
                            c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')')
                        })
                        .unwrap_or(rest.len()),
            )
        };
        if let Some(end) = end.filter(|end| *end > start + 1) {
            if !spans.iter().any(|s| start < s.end && end > s.start) {
                spans.push(Span { start, end });
            }
        }
    }
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
                    || matches!(
                        ch,
                        '<' | '>' | '"' | '\'' | '`' | '|' | '\\' | '^' | '{' | '}'
                    )
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
    (text.trim_start().starts_with("```") || text.trim_start().starts_with("~~~"))
        && input[..s.start].trim().is_empty()
        && input[s.end..].trim().is_empty()
}

/// Substitui cada span por um token e devolve o texto mascarado + a tabela.
pub fn mask(input: &str, spans: &[Span]) -> (String, SpanTable) {
    let mut out = String::with_capacity(input.len());
    let mut table = SpanTable::default();
    let mut last = 0usize;
    let mut next_token = 0;
    for span in spans {
        if span.start < last {
            continue; // defensivo: ignora sobreposicoes
        }
        out.push_str(&input[last..span.start]);
        // User text can already contain our marker syntax. Never reuse such a token.
        let tok = loop {
            let candidate = token(next_token);
            next_token += 1;
            if !input.contains(&candidate) {
                break candidate;
            }
        };
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
    // One pass over model output prevents restored text from being interpreted as tokens.
    let mut out = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some((position, token, original)) = table
        .entries
        .iter()
        .filter_map(|(token, original)| remaining.find(token).map(|p| (p, token, original)))
        .min_by_key(|(position, _, _)| *position)
    {
        out.push_str(&remaining[..position]);
        out.push_str(original);
        remaining = &remaining[position + token.len()..];
    }
    out.push_str(remaining);
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
        let input = "use <name> and %s and $VAR in a sentence";
        let spans = scan_spans(input);
        assert!(spans.is_empty());
    }

    #[test]
    fn paths_and_explicit_shell_commands_keep_their_bytes() {
        let input =
            "Ver ./src/main.rs, /etc/hosts e \"C:\\My Project\\file.md\".\n$ git  diff -- '*.rs'\n";
        let (masked, table) = mask(input, &scan_spans(input));
        assert!(!masked.contains("/etc/hosts"));
        assert!(!masked.contains("My Project"));
        assert!(!masked.contains("git  diff"));
        assert_eq!(unmask(&masked, &table), input);
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
    fn unclosed_fence_is_preserved() {
        let input = "```rust\nfn x(){}\n"; // sem fecho
        assert_eq!(
            scan_spans(input),
            vec![Span {
                start: 0,
                end: input.len()
            }]
        );
    }
}
