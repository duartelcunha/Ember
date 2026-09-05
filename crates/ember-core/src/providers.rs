//! Mapping puro entre o `LlmRequest`/resposta normalizada e o wire-format de cada
//! provider (Gemini, e qualquer endpoint OpenAI-compativel). Sem rede: so constroi JSON e
//! interpreta JSON.

use crate::model::LlmRequest;
use serde_json::{json, Value};

/// Modelo Gemini primario por defeito (ultimo Flash, com thinking).
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-2.5-flash";
/// Base URL do provider de fallback (OpenAI-compatible) por defeito: **Groq**.
///
/// Era o OpenRouter, e isso estava errado. Um fallback existe para estar la quando o primario
/// cai; o tier gratuito do OpenRouter, sem creditos comprados, da ~50 pedidos POR DIA aos
/// modelos `:free`, e esses modelos sao servidos por upstreams partilhados que devolvem 429 em
/// hora de ponta. Ou seja: a rede de seguranca do Ember rompia-se ao primeiro dia de uso a
/// serio (medido, nao teorico). O free tier do Groq da ~14 000 pedidos por dia, sem cartao de
/// credito, e serve os modelos ele proprio. Para um fallback, isso e a diferenca entre existir e
/// nao existir. O OpenRouter continua a um clique de distancia nas Settings.
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.groq.com/openai/v1";
/// Modelo de fallback por defeito: o generalista forte do Groq. Instruct e de prosa de proposito
/// (o Ember refina TEXTO, nao gera codigo).
///
/// Historico util: o default foi `deepseek/deepseek-r1:free`, que o OpenRouter DESCONTINUOU, e
/// todo o utilizador novo que seguisse o quick start apanhava um erro porque o modelo por
/// omissao ja nao existia. O `qwen3-coder:free` que se lhe seguiu era um modelo de CODIGO, mau
/// para prosa e o mais rate-limited de todos.
pub const DEFAULT_OPENAI_MODEL: &str = "llama-3.3-70b-versatile";

// ---------------------------------------------------------------------------------------
// Gemini
// ---------------------------------------------------------------------------------------

/// URL do endpoint Gemini. A chave vai no header `x-goog-api-key`, nunca na URL.
pub fn gemini_url(model: &str, stream: bool) -> String {
    let method = if stream {
        "streamGenerateContent?alt=sse"
    } else {
        "generateContent"
    };
    format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:{method}")
}

pub fn gemini_request_body(req: &LlmRequest) -> Value {
    let mut gen = json!({ "maxOutputTokens": req.max_tokens });
    // Temperatura so nos modelos 2.5: a Google recomenda manter o default (1.0) nos 3.x,
    // onde uma temperatura baixa pode degradar ou meter o modelo em loop.
    if req.model.starts_with("gemini-2.5") {
        if let Some(obj) = gen.as_object_mut() {
            obj.insert("temperature".into(), json!(req.temperature));
        }
    }
    // O campo de thinking depende da geracao do modelo (3.x e 2.5 sao mutuamente exclusivos).
    let thinking = if req.model.starts_with("gemini-3") {
        // 3.x: thinkingLevel (string). Sem desligar de todo -> "minimal" quando off.
        let level = if req.thinking {
            req.thinking_level.as_str()
        } else {
            "minimal"
        };
        Some(json!({ "thinkingLevel": level }))
    } else if req.model.starts_with("gemini-2.5") {
        // 2.5: thinkingBudget (int). -1 dinamico (max), 0 desliga.
        Some(json!({ "thinkingBudget": if req.thinking { -1 } else { 0 } }))
    } else {
        None
    };
    if let (Some(tc), Some(obj)) = (thinking, gen.as_object_mut()) {
        obj.insert("thinkingConfig".into(), tc);
    }
    json!({
        "contents": [{ "role": "user", "parts": [{ "text": req.user }] }],
        "systemInstruction": { "parts": [{ "text": req.system }] },
        "generationConfig": gen
    })
}

/// Recusa por politica: bloqueio de prompt ou finishReason SAFETY/RECITATION.
pub fn gemini_is_content_policy(body: &Value) -> bool {
    if body
        .get("promptFeedback")
        .and_then(|p| p.get("blockReason"))
        .is_some()
    {
        return true;
    }
    matches!(
        body.pointer("/candidates/0/finishReason")
            .and_then(Value::as_str),
        Some("SAFETY") | Some("RECITATION") | Some("BLOCKLIST") | Some("PROHIBITED_CONTENT")
    )
}

/// Resposta cortada pelo teto de tokens: o texto que vem esta incompleto e NUNCA deve
/// ser colado por cima da seleccao do utilizador (perderia a cauda em silencio).
pub fn gemini_is_truncated(body: &Value) -> bool {
    matches!(
        body.pointer("/candidates/0/finishReason")
            .and_then(Value::as_str),
        Some("MAX_TOKENS")
    )
}

/// Chave de API invalida no corpo de um erro Gemini (HTTP 400, nao 401/403): o status e
/// INVALID_ARGUMENT com reason API_KEY_INVALID, ou a mensagem diz "API key not valid".
/// Detetado aqui (no corpo) porque o `classify` por status sozinho trataria isto como
/// Payload e nao dispararia o fallback para a outra familia.
pub fn gemini_is_invalid_key(body: &Value) -> bool {
    let reason_invalid = body
        .pointer("/error/details")
        .and_then(Value::as_array)
        .map(|ds| {
            ds.iter()
                .any(|d| d.get("reason").and_then(Value::as_str) == Some("API_KEY_INVALID"))
        })
        .unwrap_or(false);
    let msg_invalid = body
        .pointer("/error/message")
        .and_then(Value::as_str)
        .map(|m| m.contains("API key not valid") || m.contains("API_KEY_INVALID"))
        .unwrap_or(false);
    reason_invalid || msg_invalid
}

/// O 503 do Gemini que quer dizer "este MODELO esta cheio agora", e nao "o servico esta em baixo".
///
/// A diferenca importa porque muda a decisao: um transitorio normal repete-se no mesmo sitio, mas
/// uma fila de capacidade de um modelo especifico responde exatamente o mesmo por mais que se
/// insista. Medido no log de producao: o `gemini-3.7-flash` (o mais recente, logo o mais disputado
/// do free tier) devolvia isto tres vezes seguidas, ~3s deitados fora, antes de o Ember fazer o
/// que devia ter feito logo, que era ir para o modelo do lado. Esse acabou por responder bem.
pub fn gemini_is_overloaded(body: &Value) -> bool {
    let status_unavailable = body
        .pointer("/error/status")
        .and_then(Value::as_str)
        .is_some_and(|s| s == "UNAVAILABLE");
    let msg = body
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    status_unavailable || msg.contains("high demand") || msg.contains("overloaded")
}

/// Extrai o atraso sugerido por um erro 429 do Gemini a partir do detalhe
/// `google.rpc.RetryInfo` no corpo (campo `retryDelay`, formato "42s"/"1.5s"), usado quando
/// o header HTTP `Retry-After` esta ausente. Sem isto, o backoff exponencial cego ignorava
/// o atraso que o proprio servidor recomenda.
pub fn gemini_retry_delay_ms(body: &Value) -> Option<u64> {
    let details = body.pointer("/error/details")?.as_array()?;
    let delay = details.iter().find_map(|d| {
        if d.get("@type").and_then(Value::as_str)
            == Some("type.googleapis.com/google.rpc.RetryInfo")
        {
            d.get("retryDelay").and_then(Value::as_str)
        } else {
            None
        }
    })?;
    let secs: f64 = delay.trim_end_matches('s').parse().ok()?;
    Some((secs * 1000.0).round() as u64)
}

/// Extrai o delta de texto de UM chunk de streaming Gemini (SSE `data:` ja parseado como
/// JSON). Cada chunk repete a forma do corpo completo mas so traz o texto NOVO desta
/// tranche. Ao contrario de uma extracao de resposta inteira, nao falha quando nao ha
/// texto: chunks intermedios podem ser so raciocinio (thought) ou virem vazios antes do
/// finishReason final. A classificacao de terminal (truncado/politica) usa
/// `gemini_is_truncated`/`gemini_is_content_policy` diretamente neste mesmo chunk.
pub fn gemini_stream_text_delta(chunk: &Value) -> Option<String> {
    let parts = chunk.pointer("/candidates/0/content/parts")?.as_array()?;
    let text: String = parts
        .iter()
        .filter(|p| !p.get("thought").and_then(Value::as_bool).unwrap_or(false))
        .filter_map(|p| p.get("text").and_then(Value::as_str))
        .collect();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

// A Anthropic ja NAO tem wire-format proprio aqui. O caminho nativo (`/v1/messages`, o header
// `anthropic-version`, o `ClaudeStreamEvent` e os seus proprios campos de erro) foi todo
// removido: a Anthropic serve um endpoint OpenAI-compativel, e passar por ele custa uma entrada
// numa lista de servicos em vez de uma familia inteira de codigo, testes e casos especiais.
//
// A propria Anthropic diz que essa camada e para testar e comparar modelos, nao para producao.
// A ressalva vale para quem usa ferramentas com schema estrito ou audio; um refine e uma chamada
// de texto com um system prompt e nada mais, que e exatamente o que a camada faz bem. Fica dito
// na UI, para a escolha ser informada.

// ---------------------------------------------------------------------------------------
// OpenAI-compatible (OpenRouter, DeepSeek, Groq, Ollama... todos partilham /chat/completions)
// ---------------------------------------------------------------------------------------

/// `true` se o base URL aponta para o OpenRouter (unica familia a quem mandamos o campo
/// `reasoning`: outros endpoints OpenAI-compatible rejeitam/ignoram campos desconhecidos de
/// forma inconsistente). Host-match tolerante a scheme/porta.
pub fn openai_is_openrouter(base_url: &str) -> bool {
    let Some(authority) = base_url
        .strip_prefix("https://")
        .map(|s| s.split('/').next().unwrap_or(""))
    else {
        return false;
    };
    authority.eq_ignore_ascii_case("openrouter.ai")
        || authority.eq_ignore_ascii_case("openrouter.ai:443")
}

/// URL do endpoint de chat. Tira uma barra final defensivamente (um base URL escrito a mao com
/// `/` extra nao devia duplicar a barra no caminho).
pub fn openai_chat_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

/// URL do endpoint de listagem de modelos, usado so pelo probe de validacao de chave.
pub fn openai_models_url(base_url: &str) -> String {
    format!("{}/models", base_url.trim_end_matches('/'))
}

/// Corpo do pedido no formato chat-completions. So acrescenta `reasoning` quando e OpenRouter
/// e o thinking esta ligado (o unico caso em que sabemos que o campo e aceite). `max_tokens` e
/// universalmente aceite pela familia OpenAI-compatible que visamos (nao `max_completion_tokens`,
/// rename so do OpenAI o-series direto).
/// Modelos gratuitos alternativos, para o failover automatico do OpenRouter (ver
/// `openai_fallback_models`). Ordem = preferencia. Servidos por upstreams DIFERENTES de
/// proposito: e essa diversidade que faz o failover valer alguma coisa (todos no mesmo upstream
/// cairiam ao mesmo tempo, que foi exatamente o que aconteceu com a Venice a servir o
/// `qwen3-coder:free` e o `llama-3.3:free`).
pub const OPENROUTER_FREE_MODELS: [&str; 3] = [
    "meta-llama/llama-3.3-70b-instruct:free",
    "google/gemma-4-31b-it:free",
    "qwen/qwen3-next-80b-a3b-instruct:free",
];

/// Lista para o campo `models` do OpenRouter: o modelo escolhido primeiro, seguido dos outros
/// gratuitos como rede. O OpenRouter tenta-os por ordem e serve o primeiro que estiver livre,
/// e o failover dispara exatamente no nosso caso (rate-limit e downtime do upstream). Custa
/// zero: paga-se (ou nao, sendo gratuitos) so o modelo que acabou por responder.
///
/// Porque isto existe: os modelos `:free` sao servidos por parceiros com capacidade partilhada
/// por toda a gente. Um `llama:free` sozinho devolvia 429 em horas de ponta e a cadeia toda do
/// Ember morria, mesmo com a chave e a conta perfeitas. Com a lista, o OpenRouter escolhe outro
/// modelo livre sem sequer nos devolver o erro.
///
/// So se aplica ao OpenRouter: um endpoint OpenAI-compatible qualquer (DeepSeek, Groq, Ollama)
/// nao conhece o campo `models` e podia rejeitar o pedido.
pub fn openai_fallback_models(chosen: &str, base_url: &str) -> Option<Vec<String>> {
    if !openai_is_openrouter(base_url) {
        return None;
    }
    // So faz sentido dar rede a um modelo GRATUITO: quem escolheu um modelo pago (ou um custom)
    // pediu aquele modelo, e nao queremos gastar-lhe dinheiro noutro nem trocar-lhe a qualidade
    // pelas costas.
    if !chosen.ends_with(":free") {
        return None;
    }
    let mut out = vec![chosen.to_string()];
    for m in OPENROUTER_FREE_MODELS {
        if m != chosen {
            out.push(m.to_string());
        }
    }
    Some(out)
}

pub fn openai_request_body(req: &LlmRequest, stream: bool, base_url: &str) -> Value {
    let mut body = json!({
        "model": req.model,
        "messages": [
            { "role": "system", "content": req.system },
            { "role": "user", "content": req.user }
        ],
        "max_tokens": req.max_tokens,
        "temperature": req.temperature,
        "stream": stream
    });
    // Failover de modelo DENTRO do OpenRouter, antes de a cadeia do Ember desistir desta familia.
    if let Some(models) = openai_fallback_models(&req.model, base_url) {
        if let Some(obj) = body.as_object_mut() {
            obj.insert("models".into(), json!(models));
        }
    }
    if req.thinking && openai_is_openrouter(base_url) {
        // Spelling atual do OpenRouter: `reasoning: { include: true }`. O legacy
        // `include_reasoning: true` ainda funciona mas esta descontinuado.
        if let Some(obj) = body.as_object_mut() {
            obj.insert("reasoning".into(), json!({ "include": true }));
        }
    }
    body
}

/// Recusa por politica: `finish_reason == "content_filter"`.
pub fn openai_is_content_policy(chunk: &Value) -> bool {
    matches!(
        chunk
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str),
        Some("content_filter")
    )
}

/// Resposta cortada pelo teto de tokens: `finish_reason == "length"`. Texto incompleto, nunca
/// colar por cima da seleccao.
pub fn openai_is_truncated(chunk: &Value) -> bool {
    matches!(
        chunk
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str),
        Some("length")
    )
}

// NOTA: nao existe `openai_is_invalid_key`. A familia OpenAI-compatible devolve 401 para chaves
// mas, e o `classify` ja mapeia 401->Auth (dispara fallback). Isto so existia para a Gemini
// porque ela devolve 400 para chaves mas. Nao adicionar uma versao redundante aqui.

/// Um evento do stream chat-completions, ja classificado. So os casos que importam ao refiner
/// tem variante propria; o resto (chunk de role, usage, ping...) cai em `Other` e e ignorado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenAiStreamEvent {
    /// `choices[0].delta.content`: texto novo da resposta final.
    ContentDelta(String),
    /// `choices[0].delta.reasoning` (OpenRouter) OU `reasoning_content` (DeepSeek nativo): traco
    /// de raciocinio. Nunca e colado por cima da seleccao (o shell so loga a debug); existe como
    /// variante para podermos captura-lo no futuro (painel "thinking") sem tocar no parser.
    ReasoningDelta(String),
    /// `choices[0].finish_reason` presente: terminal. O caller classifica com
    /// `openai_is_truncated`/`openai_is_content_policy` sobre `{ "finish_reason": .. }`.
    Stopped {
        finish_reason: String,
    },
    Other,
}

/// Classifica um chunk de streaming OpenAI-compatible (SSE `data:` ja parseado como JSON).
/// Ordem: finish_reason (terminal) primeiro, depois raciocinio, depois conteudo final.
pub fn openai_stream_event(chunk: &Value) -> OpenAiStreamEvent {
    // 1. Terminal: finish_reason presente (pode vir num chunk com delta vazio no fim).
    if let Some(fr) = chunk
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
    {
        return OpenAiStreamEvent::Stopped {
            finish_reason: fr.to_string(),
        };
    }
    // 2. Raciocinio: OpenRouter normaliza para `reasoning`; DeepSeek nativo usa `reasoning_content`.
    if let Some(r) = chunk
        .pointer("/choices/0/delta/reasoning")
        .and_then(Value::as_str)
    {
        if !r.is_empty() {
            return OpenAiStreamEvent::ReasoningDelta(r.to_string());
        }
    }
    if let Some(r) = chunk
        .pointer("/choices/0/delta/reasoning_content")
        .and_then(Value::as_str)
    {
        if !r.is_empty() {
            return OpenAiStreamEvent::ReasoningDelta(r.to_string());
        }
    }
    // 3. Conteudo final. O primeiro chunk so traz `role: "assistant"` (sem content) -> Other.
    if let Some(c) = chunk
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
    {
        if !c.is_empty() {
            return OpenAiStreamEvent::ContentDelta(c.to_string());
        }
    }
    OpenAiStreamEvent::Other
}

// ---------------------------------------------------------------------------------------
// Framing SSE (comum aos providers: todos usam `data: <json>\n\n`)
// ---------------------------------------------------------------------------------------

/// Parte um buffer bruto de bytes SSE nos eventos completos (delimitados por linha em
/// branco `\n\n`) e devolve o resto por consumir. Pura: so particiona bytes, sem I/O. O
/// shell acumula chunks de rede num buffer e chama isto a cada chunk recebido; o resto
/// devolvido fica no buffer para a proxima chamada, para nunca cortar um evento a meio.
pub fn split_sse_events(buf: &[u8]) -> (Vec<String>, Vec<u8>) {
    let mut events = Vec::new();
    let mut start = 0;
    while start < buf.len() {
        let pos_crlf = buf[start..].windows(4).position(|w| w == b"\r\n\r\n");
        let pos_lf = buf[start..].windows(2).position(|w| w == b"\n\n");

        let (rel, sep_len) = match (pos_crlf, pos_lf) {
            (Some(c), Some(l)) => {
                if c <= l {
                    (c, 4)
                } else {
                    (l, 2)
                }
            }
            (Some(c), None) => (c, 4),
            (None, Some(l)) => (l, 2),
            (None, None) => break,
        };

        let end = start + rel;
        events.push(String::from_utf8_lossy(&buf[start..end]).into_owned());
        start = end + sep_len;
    }
    (events, buf[start..].to_vec())
}

/// Extrai os payloads das linhas `data:` de UM bloco de evento SSE (pode ter outras linhas
/// como `event:`, que sao ignoradas). Filtra o sentinela `[DONE]` usado por algumas APIs.
pub fn parse_sse_data_lines(event_block: &str) -> Vec<&str> {
    event_block
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "[DONE]")
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Base URL do OpenRouter, explicita: o DEFAULT_OPENAI_BASE_URL passou a ser o Groq, e
    /// os campos `models`/`reasoning` so vao para o OpenRouter.
    const OPENROUTER: &str = "https://openrouter.ai/api/v1";

    fn req() -> LlmRequest {
        LlmRequest {
            model: "gemini-2.5-flash".into(),
            system: "sys".into(),
            user: "usr".into(),
            max_tokens: 512,
            temperature: 0.3,
            thinking: true,
            thinking_level: "high".into(),
        }
    }

    #[test]
    fn gemini_url_streams_and_not() {
        assert!(gemini_url("gemini-2.5-flash", false).ends_with(":generateContent"));
        assert!(gemini_url("gemini-2.5-flash", true).ends_with(":streamGenerateContent?alt=sse"));
    }

    #[test]
    fn gemini_body_shape() {
        let b = gemini_request_body(&req());
        assert_eq!(b.pointer("/contents/0/parts/0/text").unwrap(), "usr");
        assert_eq!(b.pointer("/systemInstruction/parts/0/text").unwrap(), "sys");
        assert_eq!(b.pointer("/generationConfig/maxOutputTokens").unwrap(), 512);
    }

    #[test]
    fn gemini_stream_delta_concatenates_parts_in_one_chunk() {
        let chunk = json!({
            "candidates": [{ "content": { "parts": [{ "text": "Ola " }, { "text": "mundo" }] } }]
        });
        assert_eq!(gemini_stream_text_delta(&chunk).unwrap(), "Ola mundo");
    }

    #[test]
    fn gemini_detects_content_policy() {
        let blocked = json!({ "promptFeedback": { "blockReason": "SAFETY" } });
        assert!(gemini_is_content_policy(&blocked));

        let safety =
            json!({ "candidates": [{ "finishReason": "SAFETY", "content": { "parts": [] } }] });
        assert!(gemini_is_content_policy(&safety));
    }

    #[test]
    fn gemini_3x_uses_thinking_level() {
        let mut r = req();
        r.model = "gemini-3.5-flash".into();
        r.thinking = true;
        r.thinking_level = "high".into();
        let b = gemini_request_body(&r);
        assert_eq!(
            b.pointer("/generationConfig/thinkingConfig/thinkingLevel")
                .unwrap(),
            "high"
        );
        assert!(b
            .pointer("/generationConfig/thinkingConfig/thinkingBudget")
            .is_none());
    }

    #[test]
    fn gemini_3x_off_is_minimal() {
        let mut r = req();
        r.model = "gemini-3.5-flash".into();
        r.thinking = false;
        let b = gemini_request_body(&r);
        assert_eq!(
            b.pointer("/generationConfig/thinkingConfig/thinkingLevel")
                .unwrap(),
            "minimal"
        );
    }

    #[test]
    fn gemini_25_uses_thinking_budget() {
        let mut r = req();
        r.model = "gemini-2.5-flash".into();
        r.thinking = true;
        let on = gemini_request_body(&r);
        assert_eq!(
            on.pointer("/generationConfig/thinkingConfig/thinkingBudget")
                .unwrap(),
            -1
        );
        r.thinking = false;
        let off = gemini_request_body(&r);
        assert_eq!(
            off.pointer("/generationConfig/thinkingConfig/thinkingBudget")
                .unwrap(),
            0
        );
    }

    #[test]
    fn gemini_stream_delta_skips_thought_parts() {
        let chunk = json!({
            "candidates": [{ "content": { "parts": [
                { "thought": true, "text": "raciocinio interno" },
                { "text": "resposta final" }
            ] } }]
        });
        assert_eq!(gemini_stream_text_delta(&chunk).unwrap(), "resposta final");
    }

    #[test]
    fn gemini_stream_delta_is_none_for_thought_only_or_empty_chunks() {
        let thought_only = json!({
            "candidates": [{ "content": { "parts": [{ "thought": true, "text": "so raciocinio" }] } }]
        });
        assert_eq!(gemini_stream_text_delta(&thought_only), None);

        let no_parts = json!({ "candidates": [{ "finishReason": "STOP" }] });
        assert_eq!(gemini_stream_text_delta(&no_parts), None);
    }

    #[test]
    fn gemini_detects_truncation() {
        let chunk = json!({
            "candidates": [{
                "finishReason": "MAX_TOKENS",
                "content": { "parts": [{ "text": "reescrita a meio" }] }
            }]
        });
        assert!(gemini_is_truncated(&chunk));
    }

    #[test]
    fn split_sse_events_splits_on_blank_line_and_keeps_incomplete_remainder() {
        let buf = b"data: {\"a\":1}\n\ndata: {\"a\":2}\n\ndata: {\"a\":3, incompl";
        let (events, rest) = split_sse_events(buf);
        assert_eq!(events, vec!["data: {\"a\":1}", "data: {\"a\":2}"]);
        assert_eq!(rest, b"data: {\"a\":3, incompl");
    }

    #[test]
    fn split_sse_events_handles_no_complete_event_yet() {
        let (events, rest) = split_sse_events(b"data: {\"partial");
        assert!(events.is_empty());
        assert_eq!(rest, b"data: {\"partial");
    }

    #[test]
    fn parse_sse_data_lines_extracts_data_ignores_event_line_and_done() {
        let block = "event: content_block_delta\ndata: {\"x\":1}";
        assert_eq!(parse_sse_data_lines(block), vec!["{\"x\":1}"]);

        assert_eq!(parse_sse_data_lines("data: [DONE]"), Vec::<&str>::new());
        assert_eq!(parse_sse_data_lines("data:{\"y\":2}"), vec!["{\"y\":2}"]);
    }

    #[test]
    fn gemini_detects_invalid_key_in_error_body() {
        let by_reason = json!({
            "error": {
                "code": 400,
                "status": "INVALID_ARGUMENT",
                "details": [{ "reason": "API_KEY_INVALID" }]
            }
        });
        assert!(gemini_is_invalid_key(&by_reason));

        let by_message = json!({
            "error": { "code": 400, "message": "API key not valid. Please pass a valid API key." }
        });
        assert!(gemini_is_invalid_key(&by_message));

        // Um 400 de payload comum nao e uma chave invalida.
        let plain_400 = json!({ "error": { "code": 400, "message": "Invalid JSON payload" } });
        assert!(!gemini_is_invalid_key(&plain_400));
    }

    #[test]
    fn a_model_at_capacity_is_told_apart_from_a_normal_blip() {
        // Do log real do utilizador: tres destes seguidos no `gemini-3.7-flash`, ~3s deitados
        // fora, e o modelo do lado respondeu a primeira. Sem esta distincao os dois erros sao
        // "transitorio" e a maquina insiste onde nunca vai passar.
        let high_demand = json!({ "error": {
            "code": 503,
            "message": "This model is currently experiencing high demand. Please try again later.",
            "status": "UNAVAILABLE"
        }});
        assert!(gemini_is_overloaded(&high_demand));

        // So o status ja chega (a mensagem muda com o tempo e nao queremos depender dela).
        assert!(gemini_is_overloaded(
            &json!({ "error": { "status": "UNAVAILABLE" } })
        ));
        // E so a mensagem tambem (se um dia vier sem status).
        assert!(gemini_is_overloaded(
            &json!({ "error": { "message": "The model is overloaded" } })
        ));

        // Um 500 generico NAO e isto: e um transitorio normal e merece o retry no mesmo sitio.
        assert!(!gemini_is_overloaded(
            &json!({ "error": { "code": 500, "message": "Internal" } })
        ));
        // E um rate-limit tambem nao: esse tem `Retry-After` proprio e outro tratamento.
        assert!(!gemini_is_overloaded(&json!({ "error": {
            "code": 429, "status": "RESOURCE_EXHAUSTED", "message": "Quota exceeded"
        }})));
        assert!(!gemini_is_overloaded(&json!({})));
    }

    #[test]
    fn gemini_retry_delay_parses_retry_info_from_error_details() {
        let body = json!({
            "error": {
                "code": 429,
                "status": "RESOURCE_EXHAUSTED",
                "details": [
                    { "@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": "42s" }
                ]
            }
        });
        assert_eq!(gemini_retry_delay_ms(&body), Some(42_000));
    }

    #[test]
    fn gemini_retry_delay_parses_fractional_seconds() {
        let body = json!({
            "error": { "details": [
                { "@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": "1.5s" }
            ] }
        });
        assert_eq!(gemini_retry_delay_ms(&body), Some(1500));
    }

    #[test]
    fn gemini_retry_delay_is_none_without_retry_info() {
        let no_details = json!({ "error": { "code": 429 } });
        assert_eq!(gemini_retry_delay_ms(&no_details), None);

        let other_detail = json!({ "error": { "details": [{ "@type": "something.else" }] } });
        assert_eq!(gemini_retry_delay_ms(&other_detail), None);
    }

    #[test]
    fn gemini_3x_omits_temperature_but_25_keeps_it() {
        let mut r = req();
        r.model = "gemini-3.5-flash".into();
        let b3 = gemini_request_body(&r);
        assert!(b3.pointer("/generationConfig/temperature").is_none());

        r.model = "gemini-2.5-flash".into();
        let b25 = gemini_request_body(&r);
        let temp = b25
            .pointer("/generationConfig/temperature")
            .and_then(Value::as_f64)
            .unwrap();
        assert!((temp - 0.3).abs() < 1e-6);
    }

    // ----- OpenAI-compatible -----

    #[test]
    fn openai_chat_and_models_urls_trim_trailing_slash() {
        assert_eq!(
            openai_chat_url("https://openrouter.ai/api/v1"),
            "https://openrouter.ai/api/v1/chat/completions"
        );
        assert_eq!(
            openai_chat_url("https://openrouter.ai/api/v1/"),
            "https://openrouter.ai/api/v1/chat/completions"
        );
        assert_eq!(openai_models_url("https://x/"), "https://x/models");
    }

    #[test]
    fn openai_body_shape_system_then_user_and_stream_flag() {
        let b = openai_request_body(&req(), true, OPENROUTER);
        assert_eq!(b.pointer("/messages/0/role").unwrap(), "system");
        assert_eq!(b.pointer("/messages/0/content").unwrap(), "sys");
        assert_eq!(b.pointer("/messages/1/role").unwrap(), "user");
        assert_eq!(b.pointer("/messages/1/content").unwrap(), "usr");
        assert_eq!(b.get("stream").unwrap(), true);
        assert_eq!(b.get("max_tokens").unwrap(), 512);
    }

    #[test]
    fn openrouter_free_model_gets_the_other_free_models_as_fallbacks() {
        // Regressao real: os `:free` sao servidos por upstreams partilhados (a Venice servia o
        // qwen3-coder E o llama-3.3). Um so modelo dava 429 em hora de ponta e a familia toda
        // morria, com a chave e a conta perfeitas. O campo `models` poe o OpenRouter a escolher
        // outro modelo livre sem sequer nos devolver o erro.
        let mut r = req();
        r.model = "meta-llama/llama-3.3-70b-instruct:free".into();
        let b = openai_request_body(&r, true, OPENROUTER);
        let models: Vec<&str> = b
            .get("models")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_str().unwrap())
            .collect();
        // O escolhido vem primeiro, sem duplicados, com os outros gratuitos por tras.
        assert_eq!(models[0], "meta-llama/llama-3.3-70b-instruct:free");
        assert!(models.len() >= 2);
        assert_eq!(
            models.iter().filter(|m| **m == models[0]).count(),
            1,
            "o modelo escolhido nao pode aparecer duas vezes"
        );
        // O campo `model` continua la (o OpenRouter usa-o como primario).
        assert_eq!(
            b.get("model").unwrap(),
            "meta-llama/llama-3.3-70b-instruct:free"
        );
    }

    #[test]
    fn no_model_fallbacks_for_paid_models_or_other_endpoints() {
        // Modelo PAGO: quem o escolheu quer aquele. Nao lhe trocamos a qualidade nem lhe
        // gastamos dinheiro noutro modelo pelas costas.
        let mut paid = req();
        paid.model = "anthropic/claude-haiku-4.5".into();
        let b = openai_request_body(&paid, true, OPENROUTER);
        assert!(b.get("models").is_none());

        // Endpoint que NAO e OpenRouter (DeepSeek, Groq, Ollama): nao conhece o campo `models` e
        // podia rejeitar o pedido inteiro.
        let mut free = req();
        free.model = "meta-llama/llama-3.3-70b-instruct:free".into();
        let b2 = openai_request_body(&free, true, "https://api.deepseek.com/v1");
        assert!(b2.get("models").is_none());
    }

    #[test]
    fn openai_body_adds_reasoning_only_for_openrouter_when_thinking() {
        // OpenRouter + thinking on -> reasoning presente.
        let b = openai_request_body(&req(), true, OPENROUTER);
        assert_eq!(b.pointer("/reasoning/include").unwrap(), true);

        // Outro base URL (ex: DeepSeek direto) -> sem reasoning, mesmo com thinking on.
        let b2 = openai_request_body(&req(), true, "https://api.deepseek.com/v1");
        assert!(b2.get("reasoning").is_none());

        // OpenRouter mas thinking off -> sem reasoning.
        let mut r = req();
        r.thinking = false;
        let b3 = openai_request_body(&r, true, OPENROUTER);
        assert!(b3.get("reasoning").is_none());
    }

    #[test]
    fn openai_stream_event_extracts_content_delta() {
        let chunk = json!({
            "choices": [{ "delta": { "content": "Ola" }, "index": 0 }]
        });
        assert_eq!(
            openai_stream_event(&chunk),
            OpenAiStreamEvent::ContentDelta("Ola".into())
        );
    }

    #[test]
    fn openai_stream_event_extracts_reasoning_via_both_field_names() {
        let openrouter = json!({
            "choices": [{ "delta": { "reasoning": "a pensar" } }]
        });
        assert_eq!(
            openai_stream_event(&openrouter),
            OpenAiStreamEvent::ReasoningDelta("a pensar".into())
        );

        let deepseek = json!({
            "choices": [{ "delta": { "reasoning_content": "a pensar" } }]
        });
        assert_eq!(
            openai_stream_event(&deepseek),
            OpenAiStreamEvent::ReasoningDelta("a pensar".into())
        );
    }

    #[test]
    fn openai_stream_event_finish_reason_is_terminal_and_takes_precedence() {
        for fr in ["stop", "length", "content_filter"] {
            let chunk = json!({ "choices": [{ "finish_reason": fr, "delta": {} }] });
            assert_eq!(
                openai_stream_event(&chunk),
                OpenAiStreamEvent::Stopped {
                    finish_reason: fr.into()
                }
            );
        }
    }

    #[test]
    fn openai_stream_event_ignores_role_chunk_and_empty_choices() {
        // Primeiro chunk: so anuncia o role, sem conteudo.
        let role = json!({ "choices": [{ "delta": { "role": "assistant" } }] });
        assert_eq!(openai_stream_event(&role), OpenAiStreamEvent::Other);

        // Chunk de usage sem choices (defensivo: nao pedimos usage, mas alguns mandam).
        let usage = json!({ "usage": { "total_tokens": 42 } });
        assert_eq!(openai_stream_event(&usage), OpenAiStreamEvent::Other);
    }

    #[test]
    fn openai_detects_content_filter_and_length() {
        assert!(openai_is_content_policy(&json!({
            "choices": [{ "finish_reason": "content_filter" }]
        })));
        assert!(!openai_is_content_policy(&json!({
            "choices": [{ "finish_reason": "stop" }]
        })));

        assert!(openai_is_truncated(&json!({
            "choices": [{ "finish_reason": "length" }]
        })));
        assert!(!openai_is_truncated(&json!({
            "choices": [{ "finish_reason": "stop" }]
        })));
    }
}

#[cfg(test)]
mod origin_regressions {
    #[test]
    fn openrouter_identity_requires_exact_https_authority() {
        assert!(super::openai_is_openrouter("https://openrouter.ai/api/v1"));
        for url in [
            "https://openrouter.ai.attacker.invalid/v1",
            "https://attacker.invalid/openrouter.ai",
            "https://openrouter.ai@attacker.invalid/v1",
            "http://openrouter.ai/api/v1",
        ] {
            assert!(!super::openai_is_openrouter(url));
        }
    }
}
