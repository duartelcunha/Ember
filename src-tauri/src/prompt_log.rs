//! Registo do que foi mesmo enviado ao modelo e do que ele respondeu, para se poder melhorar o
//! prompting com base em casos reais em vez de memoria.
//!
//! Porque e um ficheiro a parte e nao o log normal: o log normal e diagnostico (que provider,
//! que codigo HTTP) e nunca leva o texto do utilizador. Isto leva o texto todo, e por isso e
//! **opt-in** e vive noutro ficheiro, que se pode apagar sozinho sem levar o diagnostico atras.
//!
//! Uma linha JSON por refine (JSONL): abre-se com qualquer coisa, se le linha a linha, e um
//! ficheiro cortado a meio por um crash perde uma linha em vez de ficar ilegivel, ao contrario
//! de um array JSON.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// Nome do ficheiro no log dir da app, ao lado do `ember.log`.
pub const PROMPTS_FILE: &str = "prompts.jsonl";

/// Teto do ficheiro. Ao passar, roda para `.1` (fica um antigo), como o log normal. Sem teto,
/// um mes de uso deixava centenas de MB de texto do utilizador em disco sem ninguem reparar.
const MAX_BYTES: u64 = 5_000_000;

/// Teto por campo de texto. Um refine gigante nao pode transformar uma linha do ficheiro num
/// bloco de megabytes: para perceber o prompting bastam os primeiros milhares de caracteres.
const MAX_FIELD_CHARS: usize = 8_000;

fn clip(s: &str) -> String {
    if s.chars().count() <= MAX_FIELD_CHARS {
        return s.to_string();
    }
    let head: String = s.chars().take(MAX_FIELD_CHARS).collect();
    format!("{head}\u{2026}[cortado]")
}

pub fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_log_dir().ok().map(|d| d.join(PROMPTS_FILE))
}

/// O que se guarda de um refine. Nomes curtos de proposito: sao milhares de linhas.
pub struct Record<'a> {
    pub mode: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
    pub ms: u128,
    pub system: &'a str,
    pub input: &'a str,
    pub output: &'a str,
    /// De onde veio o contexto de projeto, quando veio (`None` = nao houve).
    pub project: Option<&'a str>,
}

/// Acrescenta uma linha. Best-effort de ponta a ponta: isto e observabilidade, e um disco cheio
/// ou uma permissao negada NUNCA pode fazer falhar um refine que ja correu bem.
pub fn append(app: &AppHandle, rec: &Record<'_>) {
    let Some(p) = path(app) else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Rotacao antes de escrever, para o teto ser um teto e nao uma sugestao.
    if std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) >= MAX_BYTES {
        let _ = std::fs::rename(&p, p.with_extension("jsonl.1"));
    }
    let line = serde_json::json!({
        "ts": chrono_ish(),
        "mode": rec.mode,
        "provider": rec.provider,
        "model": rec.model,
        "ms": rec.ms,
        "project": rec.project,
        "system": clip(rec.system),
        "input": clip(rec.input),
        "output": clip(rec.output),
    });
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{line}");
    }
}

/// Timestamp em epoch ms. Sem dependencia de datas: quem le o ficheiro converte, e o que
/// interessa aqui e a ordem e o intervalo entre refines.
fn chrono_ish() -> u64 {
    crate::now_ms()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_clipped_and_marked() {
        let big = "a".repeat(MAX_FIELD_CHARS + 500);
        let out = clip(&big);
        assert!(out.ends_with("[cortado]"), "o corte tem de ser visivel");
        assert!(out.chars().count() < MAX_FIELD_CHARS + 20);
        // O que cabe passa intacto: nada de mexer no texto que se quer estudar.
        assert_eq!(clip("curto"), "curto");
    }
}
