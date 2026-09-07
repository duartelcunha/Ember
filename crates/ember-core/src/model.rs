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
    /// Escreve a RESPOSTA a mensagem selecionada, em vez de a reescrever. Base de prompt
    /// propria: a base dos outros modos proibe explicitamente responder ao input.
    Reply,
}

/// Quanto o refine pode mexer no tamanho do texto. Ortogonal ao modo: um Fix mais curto e um
/// Rebuild mais curto sao pedidos diferentes, e nenhum dos dois cabia numa quarta escolha de
/// modo. `Same` e o default e nao acrescenta regra nenhuma ao prompt: o modo ja diz o que fazer,
/// e uma frase a pedir "mantem o tamanho" so gasta contexto para repetir o silencio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Length {
    /// Sai mais curto do que entrou, sem perder factos.
    Shorter,
    /// Sem instrucao de tamanho: o modo decide.
    #[default]
    Same,
    /// Desenvolve o que o input comprime, sem acrescentar afirmacoes novas.
    Longer,
}

/// A aparencia da brasa que fica junto ao cursor.
///
/// Todas as peles desenham DENTRO da mesma caixa de tinta para o tamanho escolhido, e e isso que
/// torna esta opcao barata: a ancoragem ao cursor, o morph da superficie e os testes de
/// flutuacao medem essa caixa, nao o desenho. Trocar de pele nao mexe em geometria nenhuma.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrbSkin {
    /// A brasa: oito celulas a perseguirem-se em volta de um halo quente.
    #[default]
    Ember,
    /// Um ponto que respira. Para quem quer saber que esta a correr sem olhar para la.
    Pulse,
    /// Um anel fino com um arco a girar. O mais neutro dos tres.
    Ring,
}

/// O tamanho da brasa, em multiplos INTEIROS do pixel do desenho: 2, 3 ou 4, o que da 10, 15 ou
/// 20px de tinta.
///
/// Inteiro nao e um detalhe de implementacao. A marca e arte de pixeis; um multiplo fracionario
/// poe as arestas entre dois pixeis do ecra e o anti-aliasing transforma a brasa num borrao. Tres
/// passos fixos em vez de um seletor livre e a forma de tornar isso impossivel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrbSize {
    Small,
    #[default]
    Normal,
    Large,
}

impl OrbSkin {
    pub const fn as_str(self) -> &'static str {
        match self {
            OrbSkin::Ember => "ember",
            OrbSkin::Pulse => "pulse",
            OrbSkin::Ring => "ring",
        }
    }
}

impl NoticeSpeed {
    pub const fn as_str(self) -> &'static str {
        match self {
            NoticeSpeed::Quick => "quick",
            NoticeSpeed::Normal => "normal",
            NoticeSpeed::Relaxed => "relaxed",
        }
    }
}

impl OrbSize {
    pub const fn as_str(self) -> &'static str {
        match self {
            OrbSize::Small => "small",
            OrbSize::Normal => "normal",
            OrbSize::Large => "large",
        }
    }

    pub const fn px(self) -> u32 {
        match self {
            OrbSize::Small => 2,
            OrbSize::Normal => 3,
            OrbSize::Large => 4,
        }
    }
}

/// Quanto tempo as notas ficam no ecra depois de o refine acabar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NoticeSpeed {
    Quick,
    #[default]
    Normal,
    Relaxed,
}

/// Pele, tamanho e ritmo, juntos porque viajam juntos: sao lidos de uma vez para o estado em
/// memoria e enviados de uma vez para a janela do overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OverlayStyle {
    pub skin: OrbSkin,
    pub size: OrbSize,
    pub notice: NoticeSpeed,
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
        assert_eq!(
            serde_json::to_string(&Provider::Gemini).unwrap(),
            "\"gemini\""
        );
        assert_eq!(
            serde_json::to_string(&Provider::OpenAi).unwrap(),
            "\"openai\""
        );
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
