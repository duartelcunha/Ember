//! O "motor Ember": duas fases puras a envolver a UNICA chamada ao LLM. O modelo faz so o que
//! so ele consegue (a reescrita ciente da lingua); o motor faz o trabalho mecanico em
//! microssegundos, com garantias que o LLM nao da: codigo em fence e URLs voltam byte-a-byte,
//! nenhum delimitador `[EMBER_INPUT]` chega ao clipboard, e um output vazio ou que perdeu um
//! span nunca e colado por cima da seleccao do utilizador.
//!
//! Fluxo:  captura -> `precondition` -> chamada LLM (I/O, fora daqui) -> `postprocess` -> paste
//!
//! Honesto sobre o que "mais inteligente" significa: um motor puro em Rust nao acrescenta
//! semantica. O ganho e (a) fiabilidade/formato garantidos, (b) aliviar o modelo do trabalho
//! mecanico para o orcamento dele ir todo para a reescrita, e (c) o unico lever semantico real,
//! a injecao de contexto do projeto (fase separada). Aqui vive (a) e (b).

pub mod finalize;
pub mod guard;
pub mod mask;
pub mod normalize;
pub mod strip;

use crate::model::RefineMode;
pub use mask::SpanTable;

/// Estilo de fim-de-linha dominante do input, para o output sair na mesma convencao.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EolStyle {
    Lf,
    Crlf,
}

/// O input ja preparado para o modelo, mais o que e preciso para reconstruir o output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// Input normalizado, com spans (codigo/URLs) mascarados e marcadores escapados.
    pub masked_input: String,
    /// Tabela token -> texto original, para desmascarar e verificar preservacao.
    pub spans: SpanTable,
    /// EOL dominante do input original.
    pub eol: EolStyle,
    pub mode: RefineMode,
    /// O input era, ele proprio, uma unica fence de codigo? Se sim, um output em fence e
    /// legitimo e `strip_structural` nao o desembrulha.
    pub input_was_single_fence: bool,
}

/// Porque e que o motor recusou colar (degrada honestamente em vez de colar lixo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradeReason {
    /// Depois de limpar, o output ficou vazio.
    EmptyAfterCleanup,
    /// O modelo perdeu ou mutou um span mascarado (codigo/URL): nao da para restaurar intacto.
    PreservationViolation,
}

/// Resultado do pos-processamento: colar o texto final, ou degradar sem colar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineResult {
    Paste(String),
    Degrade(DegradeReason),
}

/// Fase 1 (pura): prepara a seleccao para o modelo. Normaliza -> mascara codigo/URLs ->
/// escapa marcadores. A ordem importa: mascarar ANTES de escapar protege o conteudo de codigo
/// (que vira token) de ser tocado pelo escape, e um `[/EMBER_INPUT]` dentro de codigo nunca
/// chega ao modelo (vai mascarado).
pub fn precondition(raw_selection: &str, mode: RefineMode) -> Prepared {
    let (normalized, eol) = normalize::normalize_input(raw_selection);
    let spans = mask::scan_spans(&normalized);
    let input_was_single_fence = mask::is_single_fence(&normalized, &spans);
    let (masked, table) = mask::mask(&normalized, &spans);
    let masked_input = normalize::escape_input_markers(&masked);
    Prepared {
        masked_input,
        spans: table,
        eol,
        mode,
        input_was_single_fence,
    }
}

/// Fase 2 (pura): transforma o texto cru do modelo no que se cola, ou degrada. Sequencia:
/// limpa ruido estrutural -> (vazio? degrada) -> (perdeu span? degrada) -> desmascara ->
/// finaliza -> Paste.
pub fn postprocess(raw_model_text: &str, prepared: &Prepared) -> EngineResult {
    let stripped = strip::strip_structural(raw_model_text, prepared.input_was_single_fence);
    if guard::is_effectively_empty(&stripped) {
        return EngineResult::Degrade(DegradeReason::EmptyAfterCleanup);
    }
    // Verifica os tokens ANTES de desmascarar: cada span mascarado tem de estar presente.
    if !guard::check_preservation(&prepared.spans, &stripped) {
        return EngineResult::Degrade(DegradeReason::PreservationViolation);
    }
    let restored = mask::unmask(&stripped, &prepared.spans);
    let finalized = finalize::finalize(&restored, prepared.eol);
    if guard::is_effectively_empty(&finalized) {
        return EngineResult::Degrade(DegradeReason::EmptyAfterCleanup);
    }
    EngineResult::Paste(finalized)
}

#[cfg(test)]
mod golds {
    //! Um gold por arquetipo: (input, output cru do modelo, modo) -> Paste esperado | Degrade.
    //! Fixa o comportamento do motor como barra de regressao (facto, nao opiniao).
    use super::*;

    fn run(input: &str, model_out: &str, mode: RefineMode) -> EngineResult {
        let prepared = precondition(input, mode);
        postprocess(model_out, &prepared)
    }

    #[test]
    fn short_question_polish_near_identical_pastes() {
        // Polish quase-identico e sucesso, nao falha.
        let r = run("hows the weather", "How's the weather?", RefineMode::Polish);
        assert_eq!(r, EngineResult::Paste("How's the weather?".into()));
    }

    #[test]
    fn code_heavy_selection_survives_byte_for_byte() {
        let input = "make this better\n\n```rust\nfn f(){let x=1;}\n```\n";
        let prepared = precondition(input, RefineMode::Adaptive);
        // O modelo devolve a prosa reescrita + o token de codigo intacto.
        let tok = prepared.spans.tokens().next().unwrap().to_string();
        let model_out = format!("Improve this function:\n\n{tok}");
        let r = postprocess(&model_out, &prepared);
        match r {
            EngineResult::Paste(s) => {
                assert!(s.contains("fn f(){let x=1;}"));
                assert!(s.contains("```rust"));
                assert!(!s.contains("EMBER_SPAN"));
            }
            other => panic!("esperava Paste, veio {other:?}"),
        }
    }

    #[test]
    fn url_heavy_selection_preserves_urls() {
        let input = "check https://example.com/a?b=1 please";
        let prepared = precondition(input, RefineMode::Adaptive);
        let tok = prepared.spans.tokens().next().unwrap().to_string();
        let r = postprocess(&format!("Please review {tok}."), &prepared);
        match r {
            EngineResult::Paste(s) => assert!(s.contains("https://example.com/a?b=1")),
            other => panic!("esperava Paste, veio {other:?}"),
        }
    }

    #[test]
    fn leaked_outer_fence_is_unwrapped() {
        let r = run("hi", "```\nHello there.\n```", RefineMode::Adaptive);
        assert_eq!(r, EngineResult::Paste("Hello there.".into()));
    }

    #[test]
    fn echoed_markers_are_stripped() {
        let r = run("hi", "[EMBER_INPUT]\nHello there.\n[/EMBER_INPUT]", RefineMode::Adaptive);
        assert_eq!(r, EngineResult::Paste("Hello there.".into()));
    }

    #[test]
    fn dropped_span_degrades_without_pasting() {
        let input = "run https://example.com/x now";
        let prepared = precondition(input, RefineMode::Adaptive);
        // O modelo deitou fora o token do URL: nao da para restaurar -> degrada.
        let r = postprocess("Please run the command now.", &prepared);
        assert_eq!(r, EngineResult::Degrade(DegradeReason::PreservationViolation));
    }

    #[test]
    fn empty_model_output_degrades() {
        let prepared = precondition("please refine this", RefineMode::Adaptive);
        assert_eq!(
            postprocess("   \n  ", &prepared),
            EngineResult::Degrade(DegradeReason::EmptyAfterCleanup)
        );
    }

    #[test]
    fn input_that_is_a_single_fence_keeps_its_fence() {
        let input = "```py\nprint(1)\n```";
        let prepared = precondition(input, RefineMode::Adaptive);
        assert!(prepared.input_was_single_fence);
        let tok = prepared.spans.tokens().next().unwrap().to_string();
        // Output do modelo e so o token (a fence inteira): nao deve ser desembrulhado.
        let r = postprocess(&tok, &prepared);
        match r {
            EngineResult::Paste(s) => assert!(s.contains("```py") && s.contains("print(1)")),
            other => panic!("esperava Paste, veio {other:?}"),
        }
    }
}
