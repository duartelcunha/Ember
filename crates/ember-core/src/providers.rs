//! Mapping puro entre o `LlmRequest`/resposta normalizada e o wire-format de cada
//! provider (Gemini, Claude). Sem rede: so constroi JSON e interpreta JSON.

use crate::error::CoreError;
use crate::model::LlmRequest;
use serde_json::{json, Value};

/// Header de versao obrigatorio da API Anthropic.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Modelo Gemini primario por defeito (ultimo Flash, com thinking).
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-3.5-flash";
/// Modelo Claude de fallback por defeito.
pub const DEFAULT_CLAUDE_MODEL: &str = "claude-sonnet-4-6";

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
    let mut gen = json!({
        "temperature": req.temperature,
        "maxOutputTokens": req.max_tokens
    });
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

pub fn gemini_extract_text(body: &Value) -> Result<String, CoreError> {
    if gemini_is_content_policy(body) {
        return Err(CoreError::ContentPolicy);
    }
    let parts = body
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Parse("candidates[0].content.parts ausente".into()))?;
    // Salta as partes de "thought" (resumos de raciocinio); so o texto final conta.
    let text: String = parts
        .iter()
        .filter(|p| !p.get("thought").and_then(Value::as_bool).unwrap_or(false))
        .filter_map(|p| p.get("text").and_then(Value::as_str))
        .collect();
    if text.trim().is_empty() {
        return Err(CoreError::EmptyResponse);
    }
    Ok(text)
}

// ---------------------------------------------------------------------------------------
// Claude / Anthropic
// ---------------------------------------------------------------------------------------

pub fn claude_url() -> &'static str {
    "https://api.anthropic.com/v1/messages"
}

pub fn claude_request_body(req: &LlmRequest, stream: bool) -> Value {
    json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "temperature": req.temperature,
        "system": req.system,
        "messages": [{ "role": "user", "content": req.user }],
        "stream": stream
    })
}

/// Recusa por politica: stop_reason == "refusal".
pub fn claude_is_content_policy(body: &Value) -> bool {
    body.get("stop_reason").and_then(Value::as_str) == Some("refusal")
}

pub fn claude_extract_text(body: &Value) -> Result<String, CoreError> {
    if claude_is_content_policy(body) {
        return Err(CoreError::ContentPolicy);
    }
    let blocks = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Parse("content ausente".into()))?;
    let text: String = blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect();
    if text.trim().is_empty() {
        return Err(CoreError::EmptyResponse);
    }
    Ok(text)
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
    fn gemini_extracts_concatenated_text() {
        let body = json!({
            "candidates": [{ "content": { "parts": [{ "text": "Ola " }, { "text": "mundo" }] } }]
        });
        assert_eq!(gemini_extract_text(&body).unwrap(), "Ola mundo");
    }

    #[test]
    fn gemini_detects_content_policy() {
        let blocked = json!({ "promptFeedback": { "blockReason": "SAFETY" } });
        assert!(gemini_is_content_policy(&blocked));
        assert_eq!(gemini_extract_text(&blocked), Err(CoreError::ContentPolicy));

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
    fn gemini_skips_thought_parts() {
        let body = json!({
            "candidates": [{ "content": { "parts": [
                { "thought": true, "text": "raciocinio interno" },
                { "text": "resposta final" }
            ] } }]
        });
        assert_eq!(gemini_extract_text(&body).unwrap(), "resposta final");
    }

    #[test]
    fn claude_body_shape() {
        let b = claude_request_body(&req(), true);
        assert_eq!(b.get("system").unwrap(), "sys");
        assert_eq!(b.pointer("/messages/0/content").unwrap(), "usr");
        assert_eq!(b.get("stream").unwrap(), true);
    }

    #[test]
    fn claude_extracts_text_blocks_only() {
        let body = json!({
            "stop_reason": "end_turn",
            "content": [
                { "type": "text", "text": "Refinado" },
                { "type": "thinking", "text": "ignora isto" }
            ]
        });
        assert_eq!(claude_extract_text(&body).unwrap(), "Refinado");
    }

    #[test]
    fn claude_detects_refusal() {
        let body = json!({ "stop_reason": "refusal", "content": [] });
        assert!(claude_is_content_policy(&body));
        assert_eq!(claude_extract_text(&body), Err(CoreError::ContentPolicy));
    }

    #[test]
    fn empty_response_errors() {
        let body = json!({ "content": [] });
        assert_eq!(claude_extract_text(&body), Err(CoreError::EmptyResponse));
    }
}
