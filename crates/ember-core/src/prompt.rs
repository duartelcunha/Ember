//! Construcao do prompt de refinamento (o nucleo de qualidade). Puro e testavel.

use crate::model::{LlmRequest, Profile, RefineMode};

const BASE_INSTRUCTIONS: &str = "\
Es um refinador de prompts. Recebes um prompt em bruto e devolves uma versao melhorada,\
 pronta a ser enviada a um assistente de IA.

Regras:
- Preserva sempre a INTENCAO do utilizador. Nunca respondas ao prompt nem executes a\
 tarefa; so o reescreves melhor.
- Deteta a LINGUA do input e responde na MESMA lingua (a menos que o perfil diga outra).
- Corrige ortografia, gramatica, acentos e cedilhas na lingua detetada.
- Devolve APENAS o prompt refinado: sem preambulo, sem aspas a envolver, sem explicacoes,\
 sem markdown de code-fence.";

const ADAPTIVE_RULE: &str = "\
Escala a agressividade ao input: se for uma pergunta curta ou simples, so poli (clareza,\
 wording, ortografia) e mantem-no curto. Se descrever uma tarefa, estrutura-o bem (papel,\
 contexto, requisitos/constraints e formato de output desejado) sem inventar factos.";

const POLISH_RULE: &str = "\
So poli: corrige gramatica, acentos e clareza, melhora o wording, mas mantem a estrutura,\
 o tom e o tamanho do original. Nao adiciones seccoes nem reestrutures.";

const TURBO_RULE: &str = "\
Reescreve e estrutura ao maximo: papel, contexto, requisitos, exemplos quando ajudem, e\
 um formato de output explicito. Maximiza a qualidade mantendo a intencao.";

/// Constroi o system prompt final, injetando o perfil do utilizador quando existe.
pub fn build_system_prompt(profile: &Profile, mode: RefineMode) -> String {
    let mode_rule = match mode {
        RefineMode::Adaptive => ADAPTIVE_RULE,
        RefineMode::Polish => POLISH_RULE,
        RefineMode::Turbo => TURBO_RULE,
    };

    let mut out = String::with_capacity(BASE_INSTRUCTIONS.len() + mode_rule.len() + 256);
    out.push_str(BASE_INSTRUCTIONS);
    out.push_str("\n\n");
    out.push_str(mode_rule);

    if !profile.is_empty() {
        out.push_str(
            "\n\nPerfil e preferencias do utilizador a respeitar no prompt refinado \
             (estilo, tom, regras). Aplica-as, mas nao as cites nem as incluas no output:\n",
        );
        out.push_str(profile.text.trim());
    }
    out
}

/// Estima um `max_tokens` razoavel para o output. Com thinking, os tokens de raciocinio
/// sao cobrados contra o `maxOutputTokens`, por isso somamos folga generosa para nao truncar.
fn output_budget(input: &str, thinking: bool) -> u32 {
    // ~1 token por 4 chars; damos folga de 2x para reestruturar, dentro de [256, 4096].
    let approx_in = (input.chars().count() / 4) as u32;
    let answer = (approx_in.saturating_mul(2)).clamp(256, 4096);
    if thinking {
        // Reserva ~12k para o raciocinio + a resposta, com teto seguro.
        answer.saturating_add(12_288).min(32_768)
    } else {
        answer
    }
}

/// Monta o `LlmRequest` provider-agnostic a partir do input, perfil e config de thinking.
pub fn build_llm_request(
    input: &str,
    profile: &Profile,
    model: &str,
    mode: RefineMode,
    thinking: bool,
    thinking_level: &str,
) -> LlmRequest {
    LlmRequest {
        model: model.to_string(),
        system: build_system_prompt(profile, mode),
        user: input.to_string(),
        max_tokens: output_budget(input, thinking),
        temperature: 0.3,
        thinking,
        thinking_level: thinking_level.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProfileSource;

    fn empty_profile() -> Profile {
        Profile {
            text: String::new(),
            source: ProfileSource::Default,
        }
    }

    #[test]
    fn system_prompt_has_core_guarantees() {
        let s = build_system_prompt(&empty_profile(), RefineMode::Adaptive);
        assert!(s.contains("APENAS o prompt refinado"));
        assert!(s.contains("MESMA lingua"));
        assert!(s.contains("acentos"));
        // Sem perfil, nao injeta a seccao de preferencias.
        assert!(!s.contains("Perfil e preferencias"));
    }

    #[test]
    fn profile_is_injected_when_present() {
        let p = Profile {
            text: "Nunca usar em-dashes. Responder em portugues.".into(),
            source: ProfileSource::ClaudeMd,
        };
        let s = build_system_prompt(&p, RefineMode::Adaptive);
        assert!(s.contains("Perfil e preferencias"));
        assert!(s.contains("em-dashes"));
    }

    #[test]
    fn mode_changes_the_rule() {
        let polish = build_system_prompt(&empty_profile(), RefineMode::Polish);
        let turbo = build_system_prompt(&empty_profile(), RefineMode::Turbo);
        assert!(polish.contains("So poli"));
        assert!(turbo.contains("ao maximo"));
    }

    #[test]
    fn output_budget_is_clamped() {
        assert_eq!(output_budget("", false), 256);
        assert_eq!(output_budget(&"a".repeat(100_000), false), 4096);
    }

    #[test]
    fn thinking_raises_output_budget() {
        // Com thinking, ate o input vazio leva folga generosa (tokens de raciocinio).
        assert!(output_budget("", true) >= 8192);
        assert!(output_budget(&"a".repeat(100_000), true) <= 32_768);
        assert!(output_budget("", true) > output_budget("", false));
    }

    #[test]
    fn request_carries_input_and_model() {
        let req = build_llm_request(
            "ola mundo",
            &empty_profile(),
            "gemini-3.5-flash",
            RefineMode::Adaptive,
            true,
            "high",
        );
        assert_eq!(req.user, "ola mundo");
        assert_eq!(req.model, "gemini-3.5-flash");
        assert!(req.thinking);
        assert_eq!(req.thinking_level, "high");
        assert!(req.max_tokens >= 256);
    }
}
