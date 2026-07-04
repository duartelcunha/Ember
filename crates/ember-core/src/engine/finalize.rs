//! Acabamento do texto final: apara espacos no fim de linha, colapsa linhas em branco a mais,
//! tira espacos/quebras nas pontas e repoe o EOL do input. Idempotente. Puro.

use super::EolStyle;

/// Finaliza o texto (trabalha em LF, o output do modelo pode vir com CRLF) e aplica o EOL do
/// input no fim. Idempotente: `finalize(finalize(x)) == finalize(x)`.
pub fn finalize(text: &str, eol: EolStyle) -> String {
    let lf = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(lf.len());
    let mut blank_run = 0usize;
    for line in lf.split('\n') {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_run += 1;
            // Colapsa: permite no maximo UMA linha em branco seguida.
            if blank_run <= 1 {
                out.push('\n');
            }
        } else {
            blank_run = 0;
            out.push_str(trimmed);
            out.push('\n');
        }
    }
    // Sem espacos/quebras nas pontas (uma seleccao substituida nao deve ganhar newline final).
    let trimmed = out.trim();
    match eol {
        EolStyle::Lf => trimmed.to_string(),
        EolStyle::Crlf => trimmed.replace('\n', "\r\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_trailing_spaces_and_edges() {
        assert_eq!(finalize("  hello world   \n\n", EolStyle::Lf), "hello world");
    }

    #[test]
    fn collapses_extra_blank_lines() {
        assert_eq!(finalize("a\n\n\n\nb", EolStyle::Lf), "a\n\nb");
    }

    #[test]
    fn is_idempotent() {
        let once = finalize("  a\r\n\r\n\r\n\r\nb  \n", EolStyle::Lf);
        assert_eq!(finalize(&once, EolStyle::Lf), once);
    }

    #[test]
    fn applies_eol_parity() {
        assert_eq!(finalize("a\nb", EolStyle::Lf), "a\nb");
        assert_eq!(finalize("a\nb", EolStyle::Crlf), "a\r\nb");
        // CRLF de entrada com saida CRLF continua idempotente.
        let crlf = finalize("a\r\nb", EolStyle::Crlf);
        assert_eq!(finalize(&crlf, EolStyle::Crlf), crlf);
    }
}
