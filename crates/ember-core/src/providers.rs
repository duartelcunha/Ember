//! Mapping puro entre o `LlmRequest`/resposta normalizada e o wire-format de cada
//! provider (Gemini, Claude). Sem rede: so constroi JSON e interpreta JSON.

use crate::model::LlmRequest;
use serde_json::{json, Value};

/// Header de versao obrigatorio da API Anthropic.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Modelo Gemini primario por defeito (ultimo Flash, com thinking).
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-2.5-flash";
/// Modelo Claude de fallback por defeito: o tier barato e rapido (Haiku), comparavel em custo
/// ao Gemini Flash. NAO o Sonnet (bem mais caro) por defeito; fica como opcao para quem quiser.
pub const DEFAULT_CLAUDE_MODEL: &str = "claude-haiku-4-5";
/// Base URL do provider OpenAI-compatible por defeito: OpenRouter. Um so endpoint (com a mesma
/// forma /chat/completions) da acesso a muitos modelos, incluindo os `:free` com raciocinio.
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://openrouter.ai/api/v1";
/// Modelo OpenAI-compatible por defeito: DeepSeek R1 free via OpenRouter (reasoning exposto em
/// `reasoning_content`). Estes ids `:free` mudam; a UI permite escrever qualquer id (Custom).
pub const DEFAULT_OPENAI_MODEL: &str = "deepseek/deepseek-r1:free";

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
        body.pointer("/candidates/0/finishReason").and_then(Value::as_str),
        Some("SAFETY") | Some("RECITATION") | Some("BLOCKLIST") | Some("PROHIBITED_CONTENT")
    )
}

/// Resposta cortada pelo teto de tokens: o texto que vem esta incompleto e NUNCA deve
/// ser colado por cima da seleccao do utilizador (perderia a cauda em silencio).
pub fn gemini_is_truncated(body: &Value) -> bool {
    matches!(
        body.pointer("/candidates/0/finishReason").and_then(Value::as_str),
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
            ds.iter().any(|d| {
                d.get("reason").and_then(Value::as_str) == Some("API_KEY_INVALID")
            })
        })
        .unwrap_or(false);
    let msg_invalid = body
        .pointer("/error/message")
        .and_then(Value::as_str)
        .map(|m| m.contains("API key not valid") || m.contains("API_KEY_INVALID"))
        .unwrap_or(false);
    reason_invalid || msg_invalid
}

/// Extrai o atraso sugerido por um erro 429 do Gemini a partir do detalhe
/// `google.rpc.RetryInfo` no corpo (campo `retryDelay`, formato "42s"/"1.5s"), usado quando
/// o header HTTP `Retry-After` esta ausente. Sem isto, o backoff exponencial cego ignorava
/// o atraso que o proprio servidor recomenda.
pub fn gemini_retry_delay_ms(body: &Value) -> Option<u64> {
    let details = body.pointer("/error/details")?.as_array()?;
    let delay = details.iter().find_map(|d| {
        if d.get("@type").and_then(Value::as_str) == Some("type.googleapis.com/google.rpc.RetryInfo") {
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

// ---------------------------------------------------------------------------------------
// Claude / Anthropic
// ---------------------------------------------------------------------------------------

pub fn claude_url() -> &'static str {
    "https://api.anthropic.com/v1/messages"
}

pub fn claude_request_body(req: &LlmRequest, stream: bool) -> Value {
    // Sem `temperature`: os modelos Claude recentes (Opus 4.7+, Sonnet 5, Fable 5) rejeitam
    // qualquer temperatura nao-default com HTTP 400, e o utilizador pode escrever qualquer
    // id de modelo nas settings. Omitir aceita em todos; a orientacao vem do system prompt.
    json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "system": req.system,
        "messages": [{ "role": "user", "content": req.user }],
        "stream": stream
    })
}

/// Recusa por politica: stop_reason == "refusal".
pub fn claude_is_content_policy(body: &Value) -> bool {
    body.get("stop_reason").and_then(Value::as_str) == Some("refusal")
}

/// Resposta cortada pelo teto de tokens: stop_reason == "max_tokens". Texto incompleto,
/// nunca colar por cima da seleccao.
pub fn claude_is_truncated(body: &Value) -> bool {
    body.get("stop_reason").and_then(Value::as_str) == Some("max_tokens")
}

/// Um evento do stream de mensagens Claude, ja classificado a partir do `type` do JSON.
/// So os dois casos que importam ao refiner tem variante propria; o resto (message_start,
/// content_block_start/stop, thinking_delta, ping, ...) cai em `Other` e e ignorado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeStreamEvent {
    /// `content_block_delta` com `delta.type == "text_delta"`: texto novo desta tranche.
    TextDelta(String),
    /// `message_delta` com `delta.stop_reason` presente: terminal. O caller classifica com
    /// `claude_is_truncated`/`claude_is_content_policy` sobre `{"stop_reason": ..}`.
    Stopped { stop_reason: String },
    Other,
}

/// Classifica um chunk de streaming Claude (SSE `data:` ja parseado como JSON) pelo seu
/// campo `type`, ignorando o `event:` da linha SSE (redundante com o `type` do corpo).
pub fn claude_stream_event(chunk: &Value) -> ClaudeStreamEvent {
    match chunk.get("type").and_then(Value::as_str) {
        Some("content_block_delta")
            if chunk.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta") =>
        {
            match chunk.pointer("/delta/text").and_then(Value::as_str) {
                Some(t) => ClaudeStreamEvent::TextDelta(t.to_string()),
                None => ClaudeStreamEvent::Other,
            }
        }
        Some("message_delta") => match chunk.pointer("/delta/stop_reason").and_then(Value::as_str) {
            Some(sr) => ClaudeStreamEvent::Stopped {
                stop_reason: sr.to_string(),
            },
            None => ClaudeStreamEvent::Other,
        },
        _ => ClaudeStreamEvent::Other,
    }
}

// ---------------------------------------------------------------------------------------
// OpenAI-compatible (OpenRouter, DeepSeek, Groq, Ollama... todos partilham /chat/completions)
// ---------------------------------------------------------------------------------------

/// `true` se o base URL aponta para o OpenRouter (unica familia a quem mandamos o campo
/// `reasoning`: outros endpoints OpenAI-compatible rejeitam/ignoram campos desconhecidos de
/// forma inconsistente). Host-match tolerante a scheme/porta.
pub fn openai_is_openrouter(base_url: &str) -> bool {
    base_url.contains("openrouter.ai")
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
        chunk.pointer("/choices/0/finish_reason").and_then(Value::as_str),
        Some("content_filter")
    )
}

/// Resposta cortada pelo teto de tokens: `finish_reason == "length"`. Texto incompleto, nunca
/// colar por cima da seleccao.
pub fn openai_is_truncated(chunk: &Value) -> bool {
    matches!(
        chunk.pointer("/choices/0/finish_reason").and_then(Value::as_str),
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
    Stopped { finish_reason: String },
    Other,
}

/// Classifica um chunk de streaming OpenAI-compatible (SSE `data:` ja parseado como JSON).
/// Ordem: finish_reason (terminal) primeiro, depois raciocinio, depois conteudo final.
pub fn openai_stream_event(chunk: &Value) -> OpenAiStreamEvent {
    // 1. Terminal: finish_reason presente (pode vir num chunk com delta vazio no fim).
    if let Some(fr) = chunk.pointer("/choices/0/finish_reason").and_then(Value::as_str) {
        return OpenAiStreamEvent::Stopped {
            finish_reason: fr.to_string(),
        };
    }
    // 2. Raciocinio: OpenRouter normaliza para `reasoning`; DeepSeek nativo usa `reasoning_content`.
    if let Some(r) = chunk.pointer("/choices/0/delta/reasoning").and_then(Value::as_str) {
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
    if let Some(c) = chunk.pointer("/choices/0/delta/content").and_then(Value::as_str) {
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

        let safety = json!({ "candidates": [{ "finishReason": "SAFETY", "content": { "parts": [] } }] });
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
            b.pointer("/generationConfig/thinkingConfig/thinkingLevel").unwrap(),
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
            b.pointer("/generationConfig/thinkingConfig/thinkingLevel").unwrap(),
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
            on.pointer("/generationConfig/thinkingConfig/thinkingBudget").unwrap(),
            -1
        );
        r.thinking = false;
        let off = gemini_request_body(&r);
        assert_eq!(
            off.pointer("/generationConfig/thinkingConfig/thinkingBudget").unwrap(),
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
    fn claude_body_shape() {
        let b = claude_request_body(&req(), true);
        assert_eq!(b.get("system").unwrap(), "sys");
        assert_eq!(b.pointer("/messages/0/content").unwrap(), "usr");
        assert_eq!(b.get("stream").unwrap(), true);
    }

    #[test]
    fn claude_stream_event_extracts_text_delta() {
        let chunk = json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "text_delta", "text": "Refina" }
        });
        assert_eq!(
            claude_stream_event(&chunk),
            ClaudeStreamEvent::TextDelta("Refina".into())
        );
    }

    #[test]
    fn claude_stream_event_ignores_non_text_deltas_and_other_events() {
        let thinking = json!({
            "type": "content_block_delta",
            "delta": { "type": "thinking_delta", "thinking": "..." }
        });
        assert_eq!(claude_stream_event(&thinking), ClaudeStreamEvent::Other);

        let start = json!({ "type": "message_start" });
        assert_eq!(claude_stream_event(&start), ClaudeStreamEvent::Other);

        let ping = json!({ "type": "ping" });
        assert_eq!(claude_stream_event(&ping), ClaudeStreamEvent::Other);
    }

    #[test]
    fn claude_stream_event_extracts_stop_reason_from_message_delta() {
        let chunk = json!({
            "type": "message_delta",
            "delta": { "stop_reason": "end_turn" },
            "usage": { "output_tokens": 42 }
        });
        assert_eq!(
            claude_stream_event(&chunk),
            ClaudeStreamEvent::Stopped { stop_reason: "end_turn".into() }
        );

        // message_delta sem stop_reason (acontece em deltas intermedios de usage) e Other.
        let no_stop = json!({ "type": "message_delta", "delta": {} });
        assert_eq!(claude_stream_event(&no_stop), ClaudeStreamEvent::Other);
    }

    #[test]
    fn claude_detects_refusal() {
        assert!(claude_is_content_policy(&json!({ "stop_reason": "refusal" })));
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
    fn claude_detects_truncation() {
        assert!(claude_is_truncated(&json!({ "stop_reason": "max_tokens" })));
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

    #[test]
    fn claude_never_sends_temperature() {
        let b = claude_request_body(&req(), false);
        assert!(b.get("temperature").is_none());
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
        assert_eq!(
            openai_models_url("https://x/"),
            "https://x/models"
        );
    }

    #[test]
    fn openai_body_shape_system_then_user_and_stream_flag() {
        let b = openai_request_body(&req(), true, DEFAULT_OPENAI_BASE_URL);
        assert_eq!(b.pointer("/messages/0/role").unwrap(), "system");
        assert_eq!(b.pointer("/messages/0/content").unwrap(), "sys");
        assert_eq!(b.pointer("/messages/1/role").unwrap(), "user");
        assert_eq!(b.pointer("/messages/1/content").unwrap(), "usr");
        assert_eq!(b.get("stream").unwrap(), true);
        assert_eq!(b.get("max_tokens").unwrap(), 512);
    }

    #[test]
    fn openai_body_adds_reasoning_only_for_openrouter_when_thinking() {
        // OpenRouter + thinking on -> reasoning presente.
        let b = openai_request_body(&req(), true, DEFAULT_OPENAI_BASE_URL);
        assert_eq!(b.pointer("/reasoning/include").unwrap(), true);

        // Outro base URL (ex: DeepSeek direto) -> sem reasoning, mesmo com thinking on.
        let b2 = openai_request_body(&req(), true, "https://api.deepseek.com/v1");
        assert!(b2.get("reasoning").is_none());

        // OpenRouter mas thinking off -> sem reasoning.
        let mut r = req();
        r.thinking = false;
        let b3 = openai_request_body(&r, true, DEFAULT_OPENAI_BASE_URL);
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
                OpenAiStreamEvent::Stopped { finish_reason: fr.into() }
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
