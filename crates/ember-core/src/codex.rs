//! Wire-format e decisoes puras do modo "OpenAI pela subscricao ChatGPT".
//!
//! O que isto e: em vez de uma chave de API paga, o utilizador faz login com a conta ChatGPT dele
//! e os refines saem do plano que ja paga. E o mesmo caminho que o Codex CLI usa. Nao ha rede
//! aqui: so se constroi JSON, se le JSON e se decide se um token precisa de ser renovado.
//!
//! O que isto NAO e: um caminho oficial. O endpoint nao esta documentado, o `client_id` e o do
//! Codex CLI e a OpenAI pode corta-lo sem aviso. Por isso tudo o que decide alguma coisa vive
//! aqui, testado, e o resto do sistema trata este modo como mais um provider que pode falhar: se
//! deixar de funcionar, degrada para o fallback normal em vez de partir a app. A UI diz isto ao
//! utilizador antes de ele escolher, e nao depois de deixar de funcionar.

use crate::model::LlmRequest;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------------------
// OAuth (endpoints e constantes do fluxo)
// ---------------------------------------------------------------------------------------

/// Cliente publico do Codex CLI. Nao e segredo (um cliente nativo nao pode guardar segredos, e e
/// por isso que o fluxo e PKCE): vai no URL do browser e qualquer um o le no trafego do CLI.
pub const OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const OAUTH_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
pub const OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
/// `offline_access` e o que traz o refresh token. Sem ele, a sessao morria em horas e o
/// utilizador tinha de voltar ao browser a meio do dia.
pub const OAUTH_SCOPES: &str = "openid profile email offline_access";
/// Portas de callback aceites pelo servidor da OpenAI. Sao as duas que o Codex CLI regista, e o
/// `redirect_uri` tem de bater CERTO com uma delas: nao podemos escolher uma porta livre qualquer.
/// A segunda existe para o caso de a primeira estar ocupada (outro login do Codex a decorrer).
pub const OAUTH_REDIRECT_PORTS: [u16; 2] = [1455, 1457];
pub const OAUTH_REDIRECT_PATH: &str = "/auth/callback";

/// Que cliente dizemos ser. Vai em dois sitios que TEM de concordar: o parametro `originator` do
/// URL de autorizacao e o header com o mesmo nome no pedido de inferencia. O backend so aceita
/// clientes que conhece, por isso nao pode ser "ember": um nome nosso da erro de autenticacao
/// antes sequer de o utilizador ver a pagina de login.
pub const ORIGINATOR: &str = "codex_cli_rs";

pub fn redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}{OAUTH_REDIRECT_PATH}")
}

/// URL para abrir no browser. `verifier` nunca sai daqui: o que vai e o `challenge` (o seu
/// SHA-256), e e isso que torna o fluxo seguro sem segredo do cliente.
pub fn authorize_url(challenge: &str, state: &str, port: u16) -> String {
    let params = [
        ("response_type", "code"),
        ("client_id", OAUTH_CLIENT_ID),
        ("redirect_uri", &redirect_uri(port)),
        ("scope", OAUTH_SCOPES),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("state", state),
        // Os parametros proprios da OpenAI. Os dois primeiros fazem o id_token trazer a
        // organizacao e o `chatgpt_account_id` que a chamada de inferencia exige.
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        // OBRIGATORIO, e nao opcional como parecia: sem ele o servidor da OpenAI nem chega a
        // mostrar a pagina de login e devolve `missing_required_parameter`. Nao esta em
        // documentacao nenhuma; descobriu-se a bater com a porta. O valor tem de ser um cliente
        // que eles conhecam, e e o mesmo que vai no header do pedido de inferencia.
        ("originator", ORIGINATOR),
    ];
    let query: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, percent_encode(v)))
        .collect();
    format!("{OAUTH_AUTHORIZE_URL}?{}", query.join("&"))
}

/// Percent-encoding do conjunto nao-reservado (RFC 3986). Escrito a mao porque o `ember-core` nao
/// tem dependencias alem do serde, e o que precisamos de escapar aqui sao meia duzia de valores
/// conhecidos (um URL de redirect e uma lista de scopes com espacos).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// O inverso, para ler os parametros que o browser devolve no callback. `+` conta como espaco
/// (form-encoding), e uma sequencia `%` partida fica como esta em vez de fazer perder o valor.
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(b[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------------------------------------------------------------------------------------
// base64url + JWT (sem dependencias)
// ---------------------------------------------------------------------------------------

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url SEM padding, que e a forma que o PKCE (RFC 7636) exige.
pub fn b64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64URL[(n >> 18) as usize & 63] as char);
        out.push(B64URL[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(B64URL[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(B64URL[n as usize & 63] as char);
        }
    }
    out
}

/// Descodifica base64url com ou sem padding. Aceita tambem o alfabeto standard (`+/`), porque um
/// JWT vindo de terceiros nem sempre respeita o url-safe em todos os segmentos.
pub fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a') as u32 + 26,
            b'0'..=b'9' => (c - b'0') as u32 + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        })
    };
    let clean: Vec<u8> = s.bytes().filter(|b| *b != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        if chunk.len() == 1 {
            return None; // um bloco de 1 caracter nao codifica byte nenhum: entrada partida.
        }
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// Le o payload de um JWT SEM validar a assinatura, e isso e deliberado: o token acabou de nos
/// ser entregue pelo servidor da OpenAI por TLS, e o unico campo que dele tiramos serve para
/// encaminhar o pedido de volta a mesma OpenAI. Nao ha decisao de seguranca tomada com base nisto;
/// se o conteudo estiver errado, o pedido e recusado do outro lado.
pub fn jwt_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    serde_json::from_slice(&b64url_decode(payload)?).ok()
}

/// A conta ChatGPT a cobrar, do id_token. Tres sitios possiveis, porque o formato do token mudou
/// ao longo do tempo e um cliente que so conheca um deles fica sem conta e leva 401.
pub fn chatgpt_account_id(claims: &Value) -> Option<String> {
    let direct = claims.get("chatgpt_account_id").and_then(Value::as_str);
    let namespaced = claims
        .pointer("/https:~1~1api.openai.com~1auth/chatgpt_account_id")
        .and_then(Value::as_str);
    let org = claims
        .pointer("/organizations/0/id")
        .and_then(Value::as_str);
    namespaced.or(direct).or(org).map(str::to_string)
}

/// The account name to SHOW: the email from the token, or whatever name came with it. It decides
/// nothing (what the API bills is `chatgpt_account_id`); it exists so that whoever opens the
/// settings can tell which account is signed in, which an opaque id never told them.
///
/// Several places, for the same reason as the account id: the shape of the token has changed over
/// time, and the scopes we ask for (`profile email`) put the email in more than one of them.
pub fn chatgpt_account_label(claims: &Value) -> Option<String> {
    [
        "/email",
        "/https:~1~1api.openai.com~1profile/email",
        "/https:~1~1api.openai.com~1auth/user_email",
        "/preferred_username",
        "/name",
    ]
    .iter()
    .filter_map(|p| claims.pointer(p).and_then(Value::as_str))
    .find(|s| !s.trim().is_empty())
    .map(str::to_string)
}

// ---------------------------------------------------------------------------------------
// Ciclo de vida do token
// ---------------------------------------------------------------------------------------

/// Margem antes da expiracao. Renovar em cima da hora dava pedidos a falhar por causa do relogio
/// da maquina estar uns segundos adiantado, ou de o pedido demorar a chegar.
pub const REFRESH_MARGIN_MS: u64 = 60_000;

pub fn token_needs_refresh(expires_at_ms: u64, now_ms: u64) -> bool {
    expires_at_ms <= now_ms.saturating_add(REFRESH_MARGIN_MS)
}

/// Quando expira um token acabado de receber. `expires_in` vem em segundos; sem ele assumimos uma
/// hora, que e o valor tipico, e a margem acima trata do resto.
pub fn expires_at_ms(token_response: &Value, now_ms: u64) -> u64 {
    let secs = token_response
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(3600);
    now_ms.saturating_add(secs.saturating_mul(1000))
}

/// O que fazer depois de uma tentativa de renovacao falhar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    Refreshed,
    /// A sessao acabou de vez (revogada nas definicoes da conta, ou o refresh token expirou).
    /// Insistir nao ajuda: e preciso o utilizador voltar a fazer login.
    ReAuthRequired,
    /// Rede ou servidor: vale a pena voltar a tentar mais tarde, e nao apagar a sessao.
    Transient,
}

pub fn classify_refresh(http_status: u16) -> RefreshOutcome {
    match http_status {
        200 => RefreshOutcome::Refreshed,
        // 400 `invalid_grant` e o caso normal de um refresh token revogado; 401/403 idem.
        400 | 401 | 403 => RefreshOutcome::ReAuthRequired,
        _ => RefreshOutcome::Transient,
    }
}

// ---------------------------------------------------------------------------------------
// Inferencia (Responses API do backend do ChatGPT)
// ---------------------------------------------------------------------------------------

/// Endpoints de inferencia, por ordem de tentativa. Sao dois porque este backend nao esta
/// documentado e os clientes que o usam nao concordam sobre o caminho: uns falam com `wham`
/// (o nome interno), outros com `codex`. Um 404 no primeiro nao e uma avaria, e so o outro nome:
/// o shell tenta o seguinte e fica com o que respondeu.
pub const CODEX_RESPONSES_URLS: [&str; 2] = [
    "https://chatgpt.com/backend-api/wham/responses",
    "https://chatgpt.com/backend-api/codex/responses",
];

/// Listagem de modelos do mesmo backend, pela mesma ordem de tentativa. Estas rotas EXISTEM
/// (sem token respondem 401, e nao 404), so nao estao documentadas nem tem formato publicado.
/// Por isso o parser abaixo aceita varias formas e, se nao reconhecer nada, devolve vazio: o
/// resto do sistema ja trata vazio como "nao sei" e serve a lista embutida dizendo que nao e viva.
pub const CODEX_MODELS_URLS: [&str; 2] = [
    "https://chatgpt.com/backend-api/wham/models",
    "https://chatgpt.com/backend-api/codex/models",
];

/// Modelos que este backend serve por login ChatGPT.
///
/// Esta lista tem um problema que as outras nao tem, e vale a pena dizer qual: aqui NAO ha
/// `/models` para descobrir, portanto ela nao se corrige sozinha como as do Gemini ou do Groq.
/// Envelhece a mao, e ja envelheceu uma vez (nasceu com `gpt-5.2`, que entretanto passou a
/// descontinuado para login ChatGPT). Quem a atualizar confirme em learn.chatgpt.com/docs/models;
/// quem quiser um modelo que nao esteja aqui escreve-o em "Custom...", que continua a funcionar.
///
/// Ordem = preferencia, e para refinar texto o mais rapido ganha: um refine e um pedido pequeno e
/// o utilizador esta a olhar para o ecra a espera dele. O `sol` fica no fim de proposito, porque
/// gasta mais quota do plano para um trabalho que nao precisa dela.
pub const CODEX_MODELS: [&str; 4] = ["gpt-5.6-luna", "gpt-5.6-terra", "gpt-5.5", "gpt-5.6-sol"];

pub const DEFAULT_CODEX_MODEL: &str = "gpt-5.6-luna";

/// Modelos que a OpenAI JA retirou do login ChatGPT. Existe uma lista escrita a mao aqui, ao
/// contrario do resto do projeto, porque nao ha alternativa: sem listagem do provider nao ha facto
/// nenhum de onde derivar isto, e um id retirado da erro em todos os refines sem dizer porque.
/// Fica curta e datada; entradas antigas podem sair assim que ninguem tiver a config com elas.
///
/// `gpt-5.2` esta aqui porque foi o default com que este modo nasceu: sem a migracao, quem fez
/// login nesses dias ficava com um modelo morto gravado e a culpa parecia ser do login.
pub const CODEX_RETIRED_MODELS: [&str; 4] =
    ["gpt-5.2", "gpt-5.2-codex", "gpt-5.3-codex", "gpt-5.1"];

/// Le uma listagem de modelos deste backend, seja qual for a forma em que venha.
///
/// Tolerante de proposito: a rota existe mas nao esta documentada, e nao sabemos se devolve
/// `{data:[...]}` (o formato OpenAI), `{models:[...]}`, ou um array simples de ids. Aceita as
/// tres, e cada entrada pode ser uma string ou um objeto com `id`/`slug`/`model`/`name`.
///
/// Se nao reconhecer nada devolve VAZIO, e vazio quer dizer "nao sei", nunca "nao ha modelos":
/// e `models_cache` que ja distingue as duas coisas e serve a lista embutida marcada como nao
/// viva, em vez de apresentar uma lista errada como se fosse fresca.
pub fn parse_codex_models(body: &Value) -> Vec<String> {
    let rows = body
        .get("data")
        .or_else(|| body.get("models"))
        .and_then(Value::as_array)
        .or_else(|| body.as_array());
    let Some(rows) = rows else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| match row {
            Value::String(s) => Some(s.clone()),
            _ => ["id", "slug", "model", "name"]
                .iter()
                .find_map(|k| row.get(*k).and_then(Value::as_str))
                .map(str::to_string),
        })
        .filter(|id| is_plausible_model_id(id))
        .collect()
}

/// Um id de modelo utilizavel? Guarda contra uma resposta que tenha um array de outra coisa
/// qualquer (mensagens, permissoes) e que de outra forma encheria o seletor de lixo.
fn is_plausible_model_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.contains(' ')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '/' | ':'))
}

/// Corpo no formato Responses API. Tres coisas nao sao opcionais e valem a pena dizer:
/// `instructions` (o system prompt vai aqui, e nao numa mensagem), `store: false` (o backend
/// recusa o pedido sem isto) e o conteudo tipado `input_text`.
///
/// `max_output_tokens` fica DE FORA de proposito. Nestes modelos o raciocinio conta para esse
/// teto, portanto um teto pensado para o tamanho do texto podia ser gasto todo a pensar e a
/// resposta chegava cortada. Um refine e limitado pelo tamanho do que se selecionou; o corte,
/// se acontecer, e detetado em `codex_is_truncated` e nunca e colado por cima da seleccao.
pub fn codex_request_body(req: &LlmRequest) -> Value {
    // A configuracao e partilhada com o Gemini, que aceita `minimal`. Os modelos Codex
    // expostos pelo Ember usam `none` para desligar e `low` como menor nivel ativo.
    let effort = match (req.thinking, req.thinking_level.as_str()) {
        (false, _) => "none",
        (true, "minimal") => "low",
        (true, level) => level,
    };
    json!({
        "model": req.model,
        "instructions": req.system,
        "input": [{
            "type": "message",
            "role": "user",
            "content": [{ "type": "input_text", "text": req.user }]
        }],
        "store": false,
        "stream": true,
        "include": [],
        "reasoning": { "effort": effort }
    })
}

/// Um evento do stream Responses, ja classificado. Ao contrario do chat-completions (onde o
/// delta vem sempre no mesmo sitio), aqui cada evento traz um campo `type` que diz o que e.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexStreamEvent {
    /// `response.output_text.delta`: texto novo da resposta final.
    TextDelta(String),
    /// `response.reasoning_summary_text.delta`: resumo do raciocinio. NUNCA e colado por cima da
    /// seleccao, pela mesma regra que o `thought` do Gemini e o `reasoning` do OpenRouter.
    ReasoningDelta(String),
    /// `response.completed`: terminal. `incomplete_reason` preenchido = a resposta veio cortada.
    Completed {
        status: String,
        incomplete_reason: Option<String>,
    },
    /// `response.failed` (ou um evento `error`): o backend desistiu a meio do stream.
    Failed {
        message: String,
    },
    /// O modelo recusou. Chega como um evento proprio, e nao como erro: sem um ramo para isto, a
    /// recusa dava um stream sem texto nenhum e era reportada como falha transitoria, o que
    /// gastava as tentativas todas e a familia seguinte a repetir um pedido que vai ser recusado
    /// na mesma.
    Refusal(String),
    Other,
}

pub fn codex_stream_event(chunk: &Value) -> CodexStreamEvent {
    let kind = chunk.get("type").and_then(Value::as_str).unwrap_or("");
    let delta = || {
        chunk
            .get("delta")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    match kind {
        "response.output_text.delta" => CodexStreamEvent::TextDelta(delta()),
        "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
            CodexStreamEvent::ReasoningDelta(delta())
        }
        "response.refusal.delta" | "response.refusal.done" => CodexStreamEvent::Refusal(
            chunk
                .get("refusal")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(delta),
        ),
        "response.completed" | "response.incomplete" => CodexStreamEvent::Completed {
            status: chunk
                .pointer("/response/status")
                .and_then(Value::as_str)
                .unwrap_or("completed")
                .to_string(),
            incomplete_reason: chunk
                .pointer("/response/incomplete_details/reason")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        "response.failed" | "error" => CodexStreamEvent::Failed {
            message: chunk
                .pointer("/response/error/message")
                .or_else(|| chunk.pointer("/error/message"))
                .or_else(|| chunk.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("the provider ended the stream without a reason")
                .to_string(),
        },
        _ => CodexStreamEvent::Other,
    }
}

/// A resposta veio cortada pelo teto de tokens? O texto que chegou esta incompleto e nunca deve
/// ser colado por cima da seleccao (perderia a cauda em silencio).
pub fn codex_is_truncated(status: &str, incomplete_reason: Option<&str>) -> bool {
    status == "incomplete" || incomplete_reason == Some("max_output_tokens")
}

/// Recusa por politica de conteudo, a partir da mensagem de erro. O backend nao tem um campo
/// proprio para isto, ao contrario do Gemini e do chat-completions; a mensagem e o que ha.
pub fn codex_is_content_policy(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("content policy") || m.contains("usage policy") || m.contains("safety")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> LlmRequest {
        LlmRequest {
            model: "gpt-5.2".into(),
            system: "sys".into(),
            user: "usr".into(),
            max_tokens: 512,
            temperature: 0.3,
            thinking: true,
            thinking_level: "low".into(),
        }
    }

    #[test]
    fn body_has_the_three_fields_the_backend_refuses_to_work_without() {
        let b = codex_request_body(&req());
        // `instructions` e o system prompt: numa mensagem `role: system` era ignorado.
        assert_eq!(b.get("instructions").unwrap(), "sys");
        // Conteudo TIPADO: uma string simples em `content` da erro de payload.
        assert_eq!(b.pointer("/input/0/content/0/type").unwrap(), "input_text");
        assert_eq!(b.pointer("/input/0/content/0/text").unwrap(), "usr");
        // `store: false` e obrigatorio, e ainda por cima e o que queremos: o texto do utilizador
        // nao fica guardado do lado deles.
        assert_eq!(b.get("store").unwrap(), false);
        assert_eq!(b.get("stream").unwrap(), true);
        // Sem teto de output: nestes modelos o raciocinio gastaria o teto e a resposta vinha
        // cortada. Ver o comentario em `codex_request_body`.
        assert!(b.get("max_output_tokens").is_none());
        assert!(b.get("max_tokens").is_none());
    }

    #[test]
    fn reasoning_effort_preserves_supported_levels() {
        let mut r = req();
        r.thinking_level = "high".into();
        assert_eq!(
            codex_request_body(&r).pointer("/reasoning/effort").unwrap(),
            "high"
        );
    }

    #[test]
    fn reasoning_effort_uses_none_when_thinking_is_disabled() {
        let mut r = req();
        r.model = "gpt-5.6-luna".into();
        r.thinking = false;
        assert_eq!(
            codex_request_body(&r).pointer("/reasoning/effort").unwrap(),
            "none"
        );
    }

    #[test]
    fn reasoning_effort_maps_minimal_to_low_for_codex_models() {
        let mut r = req();
        r.model = "gpt-5.6-luna".into();
        r.thinking_level = "minimal".into();
        assert_eq!(
            codex_request_body(&r).pointer("/reasoning/effort").unwrap(),
            "low"
        );
    }

    #[test]
    fn stream_events_separate_the_answer_from_the_reasoning() {
        // A distincao mais importante do parser: o resumo do raciocinio chega no MESMO stream e
        // com a mesma forma que a resposta, so muda o `type`. Confundi-los colava o raciocinio
        // do modelo por cima do texto do utilizador.
        let text = json!({ "type": "response.output_text.delta", "delta": "Ola" });
        assert_eq!(
            codex_stream_event(&text),
            CodexStreamEvent::TextDelta("Ola".into())
        );
        let reasoning =
            json!({ "type": "response.reasoning_summary_text.delta", "delta": "a pensar" });
        assert_eq!(
            codex_stream_event(&reasoning),
            CodexStreamEvent::ReasoningDelta("a pensar".into())
        );
        // Um evento que nao conhecemos e ignorado, nunca tratado como texto.
        let unknown = json!({ "type": "response.output_item.added", "item": {} });
        assert_eq!(codex_stream_event(&unknown), CodexStreamEvent::Other);
        assert_eq!(codex_stream_event(&json!({})), CodexStreamEvent::Other);
    }

    #[test]
    fn model_listings_are_read_in_whatever_shape_they_arrive() {
        // A rota existe (401 sem token, nao 404) mas nao ha formato publicado. Aceitar so uma
        // forma era ficar sem descoberta no dia em que ela passe a responder noutra.
        let openai_style = json!({ "data": [{ "id": "gpt-5.6-luna" }, { "id": "gpt-5.6-sol" }] });
        assert_eq!(
            parse_codex_models(&openai_style),
            vec!["gpt-5.6-luna", "gpt-5.6-sol"]
        );
        assert_eq!(
            parse_codex_models(&json!({ "models": [{ "slug": "gpt-5.5" }] })),
            vec!["gpt-5.5"]
        );
        assert_eq!(
            parse_codex_models(&json!(["gpt-5.6-terra"])),
            vec!["gpt-5.6-terra"]
        );
    }

    #[test]
    fn an_unrecognised_listing_says_i_dont_know_instead_of_guessing() {
        // Vazio nao e "nao ha modelos", e "nao sei": quem chama serve entao a lista embutida e
        // diz que nao e viva. Apresentar uma lista errada como fresca seria pior do que nao ter
        // descoberta nenhuma.
        assert!(parse_codex_models(&json!({ "detail": "unauthorized" })).is_empty());
        assert!(parse_codex_models(&json!({})).is_empty());
        // E um array de outra coisa qualquer nao enche o seletor de lixo.
        let junk =
            json!({ "data": [{ "message": "hello there friend" }, { "id": "com espacos" }] });
        assert!(parse_codex_models(&junk).is_empty());
    }

    #[test]
    fn a_refusal_is_its_own_event_and_not_an_empty_answer() {
        // Sem este ramo, uma recusa era um stream sem texto nenhum, classificada como falha
        // transitoria: gastavam-se as tres tentativas e a familia seguinte a repetir um pedido
        // que ia ser recusado na mesma, e o utilizador ficava sem saber porque.
        let ev = json!({ "type": "response.refusal.done", "refusal": "I can't help with that." });
        assert_eq!(
            codex_stream_event(&ev),
            CodexStreamEvent::Refusal("I can't help with that.".into())
        );
        // O delta parcial usa o campo `delta`, como os outros eventos incrementais.
        let partial = json!({ "type": "response.refusal.delta", "delta": "I can't" });
        assert_eq!(
            codex_stream_event(&partial),
            CodexStreamEvent::Refusal("I can't".into())
        );
    }

    #[test]
    fn a_cut_off_answer_is_recognised_as_truncated() {
        let ev = json!({
            "type": "response.completed",
            "response": {
                "status": "incomplete",
                "incomplete_details": { "reason": "max_output_tokens" }
            }
        });
        match codex_stream_event(&ev) {
            CodexStreamEvent::Completed {
                status,
                incomplete_reason,
            } => {
                assert!(codex_is_truncated(&status, incomplete_reason.as_deref()));
            }
            other => panic!("esperava Completed, veio {other:?}"),
        }
        // Uma resposta normal NAO e truncada (senao nunca se colava nada).
        let ok = json!({ "type": "response.completed", "response": { "status": "completed" } });
        match codex_stream_event(&ok) {
            CodexStreamEvent::Completed {
                status,
                incomplete_reason,
            } => {
                assert!(!codex_is_truncated(&status, incomplete_reason.as_deref()));
            }
            other => panic!("esperava Completed, veio {other:?}"),
        }
    }

    #[test]
    fn a_failed_stream_carries_the_reason() {
        let ev = json!({
            "type": "response.failed",
            "response": { "error": { "message": "rate limit reached for your plan" } }
        });
        assert_eq!(
            codex_stream_event(&ev),
            CodexStreamEvent::Failed {
                message: "rate limit reached for your plan".into()
            }
        );
        // Sem mensagem nenhuma continua a ser um Failed com texto util, e nao um panico nem um
        // Other que deixava o stream acabar em silencio.
        let bare = json!({ "type": "response.failed" });
        assert!(matches!(
            codex_stream_event(&bare),
            CodexStreamEvent::Failed { message } if !message.is_empty()
        ));
        assert!(codex_is_content_policy("blocked by our content policy"));
        assert!(!codex_is_content_policy("rate limit reached"));
    }

    #[test]
    fn base64url_round_trips_without_padding() {
        for case in [
            &b""[..],
            &b"f"[..],
            &b"fo"[..],
            &b"foo"[..],
            &b"foob"[..],
            &b"fooba"[..],
            &b"foobar"[..],
        ] {
            let enc = b64url_encode(case);
            assert!(!enc.contains('='), "PKCE exige base64url SEM padding");
            assert_eq!(b64url_decode(&enc).unwrap(), case);
        }
        // Os dois caracteres que distinguem base64url de base64: nunca podem aparecer.
        let tricky = b64url_encode(&[251, 255, 190]);
        assert!(!tricky.contains('+') && !tricky.contains('/'));
        // Aceita padding e o alfabeto standard a entrada (JWTs de terceiros nao sao consistentes).
        assert_eq!(b64url_decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(b64url_decode("Zm9v").unwrap(), b"foo");
        assert!(b64url_decode("!!!").is_none());
    }

    #[test]
    fn the_account_label_prefers_the_email_and_falls_back_to_a_name() {
        let label = |v| chatgpt_account_label(&v);
        assert_eq!(label(json!({ "email": "a@b.c" })).unwrap(), "a@b.c");
        assert_eq!(
            label(json!({ "https://api.openai.com/profile": { "email": "p@b.c" } })).unwrap(),
            "p@b.c"
        );
        assert_eq!(label(json!({ "name": "Duarte" })).unwrap(), "Duarte");
        // O email ganha ao nome: e o que identifica a conta sem ambiguidade.
        assert_eq!(
            label(json!({ "name": "Duarte", "email": "a@b.c" })).unwrap(),
            "a@b.c"
        );
        // Vazio nao e nome de conta nenhum, e um id opaco tambem nao: mostrar isso era pior do
        // que nao mostrar nada.
        assert_eq!(label(json!({ "email": "   ", "chatgpt_account_id": "acc-1" })), None);
    }

    #[test]
    fn account_id_is_found_wherever_the_token_happens_to_put_it() {
        // Tres formatos vistos no mesmo campo ao longo do tempo. Um cliente que so conheca um
        // deles fica sem conta para cobrar e leva 401 sem perceber porque.
        let namespaced = json!({
            "https://api.openai.com/auth": { "chatgpt_account_id": "acc-ns" }
        });
        assert_eq!(chatgpt_account_id(&namespaced).unwrap(), "acc-ns");
        assert_eq!(
            chatgpt_account_id(&json!({ "chatgpt_account_id": "acc-top" })).unwrap(),
            "acc-top"
        );
        assert_eq!(
            chatgpt_account_id(&json!({ "organizations": [{ "id": "org-1" }] })).unwrap(),
            "org-1"
        );
        assert_eq!(chatgpt_account_id(&json!({ "sub": "u" })), None);
    }

    #[test]
    fn jwt_payload_is_read_without_verifying_the_signature() {
        let payload = b64url_encode(br#"{"chatgpt_account_id":"acc-1"}"#);
        let token = format!("header.{payload}.signature");
        let claims = jwt_claims(&token).expect("payload legivel");
        assert_eq!(chatgpt_account_id(&claims).unwrap(), "acc-1");
        // Lixo nao rebenta: devolve None e o shell trata como "sem conta".
        assert!(jwt_claims("nao-e-um-jwt").is_none());
        assert!(jwt_claims("a.!!!.c").is_none());
        assert!(jwt_claims("").is_none());
    }

    #[test]
    fn a_token_is_refreshed_before_it_actually_expires() {
        let now = 1_000_000;
        // Dentro da margem: renova ANTES de expirar, senao um pedido lento apanhava a expiracao
        // a meio do caminho.
        assert!(token_needs_refresh(now + REFRESH_MARGIN_MS - 1, now));
        assert!(token_needs_refresh(now, now));
        assert!(token_needs_refresh(0, now));
        // Com folga suficiente, nao se toca.
        assert!(!token_needs_refresh(now + REFRESH_MARGIN_MS + 1_000, now));

        assert_eq!(
            expires_at_ms(&json!({ "expires_in": 3600 }), now),
            now + 3_600_000
        );
        // Sem `expires_in`, assume uma hora em vez de tratar como expirado (o que daria uma
        // renovacao em cada pedido) ou como eterno (que daria 401 sem renovar nunca).
        assert_eq!(expires_at_ms(&json!({}), now), now + 3_600_000);
    }

    #[test]
    fn a_revoked_session_asks_for_a_new_login_instead_of_retrying() {
        // `invalid_grant` chega como 400. Tratar isso como transitorio punha a app a bater no
        // servidor com um token que nunca mais vai servir.
        assert_eq!(classify_refresh(400), RefreshOutcome::ReAuthRequired);
        assert_eq!(classify_refresh(401), RefreshOutcome::ReAuthRequired);
        assert_eq!(classify_refresh(200), RefreshOutcome::Refreshed);
        // 5xx e rede: nao apagar a sessao por causa de um servidor com soluços.
        assert_eq!(classify_refresh(503), RefreshOutcome::Transient);
        assert_eq!(classify_refresh(429), RefreshOutcome::Transient);
    }

    #[test]
    fn the_authorize_url_carries_the_challenge_and_never_the_verifier() {
        let url = authorize_url("CHALLENGE123", "st-1", 1455);
        assert!(url.starts_with(OAUTH_AUTHORIZE_URL));
        assert!(url.contains("code_challenge=CHALLENGE123"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=st-1"));
        // O redirect tem de ir escapado, senao o `://` parte a query string.
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"));
        // Os espacos dos scopes idem.
        assert!(url.contains("scope=openid%20profile%20email%20offline_access"));
        // Sem estes dois, o id_token vem sem a conta a cobrar.
        assert!(url.contains("id_token_add_organizations=true"));
        assert!(url.contains("codex_cli_simplified_flow=true"));
        // Regressao real: sem `originator` a OpenAI recusa com `missing_required_parameter` e o
        // utilizador nem chega a ver a pagina de login. Nao esta documentado em lado nenhum, por
        // isso este assert e a unica coisa que impede que volte a desaparecer.
        assert!(
            url.contains(&format!("originator={ORIGINATOR}")),
            "o originator e obrigatorio no URL de autorizacao"
        );
        assert_eq!(redirect_uri(1457), "http://localhost:1457/auth/callback");
    }

    #[test]
    fn percent_encoding_round_trips_the_values_the_flow_actually_carries() {
        for s in [
            "http://localhost:1455/auth/callback",
            "openid profile email offline_access",
            "abc/123+def=",
        ] {
            assert_eq!(percent_decode(&percent_encode(s)), s);
        }
        // Form-encoding: `+` a entrada e um espaco.
        assert_eq!(percent_decode("a+b"), "a b");
        // Uma sequencia `%` partida nao pode fazer perder o resto do valor (seria um `code`
        // truncado e um login que falha sem explicacao).
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }
}
