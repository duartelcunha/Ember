//! Tipos de dominio partilhados (provider-agnostic).

use serde::{Deserialize, Serialize};

/// Os providers suportados. Gemini e o primario; Claude o fallback (familia diferente).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Gemini,
    Claude,
}

impl Provider {
    pub fn display_name(&self) -> &'static str {
        match self {
            Provider::Gemini => "Gemini",
            Provider::Claude => "Claude",
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
}
