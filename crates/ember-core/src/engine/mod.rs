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
    /// O output veio noutra lingua que o input: o modelo traduziu apesar de o prompt o proibir.
    /// Colar isto substituia o texto do utilizador por uma traducao que ele nao pediu.
    LanguageFlipped,
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
/// Teto de caracteres abaixo do qual uma seleccao curta e olhada com desconfianca. Acima disto
/// ha sempre frase que chegue para valer a pena refinar.
const TRIVIAL_MAX_CHARS: usize = 20;
/// Palavras maximas de uma seleccao trivial. Duas ("tester aasdd") nao tem estrutura para
/// melhorar; a partir de tres ja pode haver uma frase curta a serio.
const TRIVIAL_MAX_WORDS: usize = 2;

/// Vale a pena gastar uma chamada ao modelo com este texto?
///
/// Existe por causa de um caso real: refinar "tester aasdd" custou 3,8 segundos e uma chamada
/// para devolver exatamente a mesma coisa. Pagar para descobrir que nao havia nada a melhorar e
/// desperdicio, e desperdicio que o utilizador ve.
///
/// A regra e DELIBERADAMENTE conservadora, porque o erro caro e o falso positivo: saltar um
/// refine que a pessoa queria e pior do que gastar uma chamada a mais. So salta o que e curto E
/// tem pouquissimas palavras E nao tem sinal nenhum de frase.
pub fn is_worth_refining(text: &str, mode: RefineMode) -> bool {
    // O modo Turbo existe para PEGAR num fragmento e o expandir (ver `output_budget`, que lhe da
    // um teto proprio precisamente para entradas curtas). Saltar "ship monday" em Turbo era
    // desligar o atalho exatamente no caso para que ele foi feito.
    if mode == RefineMode::Turbo {
        return true;
    }
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if t.chars().count() > TRIVIAL_MAX_CHARS {
        return true;
    }
    // Fora do ASCII nao se decide nada. A contagem de palavras assenta em espacos, e o japones,
    // o chines e o tailandes escrevem frases inteiras sem nenhum: uma frase de quinze
    // caracteres contava como UMA palavra e era descartada em silencio. Como o erro caro aqui e
    // saltar um refine que a pessoa queria, tudo o que nao seja ASCII simples passa.
    if !t.is_ascii() {
        return true;
    }
    // Pontuacao e sinal de que ha frase, mesmo curta ("nao, obrigado").
    if t.chars()
        .any(|c| matches!(c, '.' | '!' | '?' | ',' | ';' | ':'))
    {
        return true;
    }
    t.split_whitespace().count() > TRIVIAL_MAX_WORDS
}

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
    // Guarda de lingua sobre o texto MASCARADO dos dois lados: os spans de codigo e URLs estao
    // reduzidos a tokens, por isso um bloco de codigo em ingles dentro de um texto portugues nao
    // desequilibra a contagem. (O `stripped` e o output ainda mascarado; o `finalized` ja tem o
    // codigo de volta e daria um veredicto pior.)
    if guard::language_flipped(&prepared.masked_input, &stripped) {
        return EngineResult::Degrade(DegradeReason::LanguageFlipped);
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
        let r = run(
            "hi",
            "[EMBER_INPUT]\nHello there.\n[/EMBER_INPUT]",
            RefineMode::Adaptive,
        );
        assert_eq!(r, EngineResult::Paste("Hello there.".into()));
    }

    #[test]
    fn dropped_span_degrades_without_pasting() {
        let input = "run https://example.com/x now";
        let prepared = precondition(input, RefineMode::Adaptive);
        // O modelo deitou fora o token do URL: nao da para restaurar -> degrada.
        let r = postprocess("Please run the command now.", &prepared);
        assert_eq!(
            r,
            EngineResult::Degrade(DegradeReason::PreservationViolation)
        );
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

#[cfg(test)]
mod preflight {
    use super::is_worth_refining;

    use crate::model::RefineMode;

    #[test]
    fn a_sentence_without_spaces_is_never_silently_skipped() {
        // Japones, chines e tailandes escrevem sem espacos: a contagem de palavras dava 1 e a
        // frase inteira era descartada. Fora do ASCII nao se decide nada.
        assert!(is_worth_refining(
            "明日の会議は中止になりました。",
            RefineMode::Polish
        ));
        assert!(is_worth_refining("我们明天开会", RefineMode::Polish));
        assert!(is_worth_refining("ola mundo", RefineMode::Polish) == false);
        assert!(is_worth_refining("olá mundo", RefineMode::Polish));
    }

    #[test]
    fn turbo_never_skips_because_expanding_fragments_is_its_job() {
        // O Turbo existe para pegar num fragmento e o expandir; saltar aqui desligava o atalho
        // no unico caso que ele serve.
        assert!(is_worth_refining("ship monday", RefineMode::Turbo));
        assert!(is_worth_refining("asdf", RefineMode::Turbo));
    }

    #[test]
    fn a_selection_with_nothing_to_improve_never_reaches_the_model() {
        // O caso que motivou isto: duas palavras sem estrutura, 3,8s e uma chamada para
        // devolver o mesmo texto.
        assert!(!is_worth_refining("tester aasdd", RefineMode::Polish));
        assert!(!is_worth_refining("asdf", RefineMode::Polish));
        assert!(!is_worth_refining("   ", RefineMode::Polish));
        assert!(!is_worth_refining("hello world", RefineMode::Polish));
    }

    #[test]
    fn anything_that_could_be_a_real_sentence_is_refined() {
        // O falso positivo e o erro caro: saltar um refine que a pessoa queria e pior do que
        // gastar uma chamada a mais. Tres palavras passam, por curtas que sejam.
        assert!(is_worth_refining("fix this bug", RefineMode::Polish));
        assert!(is_worth_refining("ok.", RefineMode::Polish));
        assert!(is_worth_refining("we ship monday", RefineMode::Polish));
        assert!(is_worth_refining(
            "preciso que vas ao linear ver os issues",
            RefineMode::Polish
        ));
        assert!(is_worth_refining("nao, obrigado", RefineMode::Polish));
    }
}
