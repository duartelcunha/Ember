//! Tipos de dominio partilhados (provider-agnostic).

use serde::{Deserialize, Serialize};

/// Os providers suportados: o Gemini como primario, e UM slot de fallback que fala o protocolo
/// OpenAI. A cadeia de tentativa e construida por ordem de prioridade, filtrada pelos
/// configurados (ver `commands::refine_text`).
///
/// O Claude teve aqui uma variante propria e deixou de ter. Nao se perdeu capacidade: a
/// Anthropic serve um endpoint OpenAI-compativel, por isso o Claude passou a ser mais um
/// servico a escolher no slot de fallback, como o Groq ou o DeepSeek. Uma familia a menos para
/// manter em codigo, e para o utilizador uma escolha em vez de tres cartoes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Gemini,
    /// Qualquer endpoint OpenAI-compatible (Groq, OpenAI, OpenRouter, Anthropic, DeepSeek,
    /// Ollama...). O base URL e o modelo vivem na config; a chave e BYOK como a outra.
    /// `rename` explicito para o id IPC ficar "openai" (sem underscore) e bater com o
    /// `ProviderKind` do TS.
    #[serde(rename = "openai")]
    OpenAi,
}

impl Provider {
    pub fn display_name(&self) -> &'static str {
        match self {
            Provider::Gemini => "Gemini",
            Provider::OpenAi => "OpenAI-compatible",
        }
    }
}

/// Modo de refinamento, escolhido nas settings. `Adaptive` e o default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RefineMode {
    /// Escala a agressividade ao input: pergunta curta -> polir; tarefa -> estruturar.
    #[default]
    Adaptive,
    /// So corrige (gramatica, acentos, clareza), mantem estrutura e tamanho.
    Polish,
    /// Reescreve e estrutura ao maximo.
    Turbo,
}

/// De onde veio o perfil de personalizacao.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSource {
    /// Auto-detetado de um CLAUDE.md na maquina.
    ClaudeMd,
    /// Escrito/editado pelo utilizador no painel de settings.
    UserEdited,
    /// Perfil "qualidade" embutido (quem nao tem ficheiro nem editou nada).
    Default,
}

/// O perfil que guia o refinamento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub text: String,
    pub source: ProfileSource,
}

impl Profile {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

/// Pedido provider-agnostic. Os adapters mapeiam isto para o wire-format de cada provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: String,
    pub system: String,
    pub user: String,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Ativa o raciocinio alargado (Gemini). Falso = minimo/desligado.
    pub thinking: bool,
    /// Nivel de thinking para Gemini 3.x: "minimal"|"low"|"medium"|"high".
    pub thinking_level: String,
}

/// Resposta normalizada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub provider: Provider,
    /// O modelo que REALMENTE respondeu, que nao e sempre o escolhido nas settings: a cadeia
    /// pode ter caido para um alternativo quando o primeiro estava cheio. Sem isto, o registo de
    /// prompts atribuia a resposta ao modelo errado, que e pior do que nao a atribuir a nenhum.
    pub model: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A forma serializada do `Provider` E o contrato IPC com o `ProviderKind` do TS
    /// ("gemini" | "openai"). Um rename mal feito (ex: "open_ai") passaria despercebido em
    /// runtime e partiria o `needsRevalidation`/`set_model`. Pina-a aqui.
    #[test]
    fn provider_serializes_to_ipc_ids() {
        assert_eq!(serde_json::to_string(&Provider::Gemini).unwrap(), "\"gemini\"");
        assert_eq!(serde_json::to_string(&Provider::OpenAi).unwrap(), "\"openai\"");
        // Round-trip: o id "openai" (sem underscore) tem de desserializar de volta.
        let p: Provider = serde_json::from_str("\"openai\"").unwrap();
        assert_eq!(p, Provider::OpenAi);
    }

    /// Uma config gravada quando o Claude ainda era um provider traz `"claude"` algures. Isso
    /// tem de FALHAR a desserializar de forma limpa, e nao entrar como um provider fantasma
    /// que depois ninguem sabe tratar.
    #[test]
    fn the_removed_claude_provider_no_longer_deserializes() {
        assert!(serde_json::from_str::<Provider>("\"claude\"").is_err());
    }
}
