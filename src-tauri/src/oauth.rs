//! Login com a conta ChatGPT (PKCE) e renovacao do token. So I/O: o que decide alguma coisa
//! (URL de autorizacao, leitura do id_token, quando renovar, o que fazer quando a renovacao
//! falha) vive em `ember_core::codex`, testado sem rede.
//!
//! Como funciona, em duas linhas: abrimos o browser na pagina da OpenAI, ficamos a ouvir numa
//! porta de localhost que a OpenAI aceita como redirect, e o browser devolve-nos um `code` que
//! trocamos por tokens. O `code` sozinho nao serve a mais ninguem: so vale com o `verifier` que
//! nunca saiu daqui (e isso o PKCE).

use ember_core::codex as wire;
use ember_core::CoreError;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::secrets::{self, OAuthSession};
use crate::state::AppState;

/// Quanto tempo esperamos que o utilizador termine o login no browser antes de desistir e
/// libertar a porta. Sem isto, um login abandonado deixava um listener preso para sempre e o
/// proximo login encontrava a porta ocupada por nos proprios.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// Teto para os pedidos ao servidor de tokens. O cliente partilhado nao tem teto total (um refine
/// com thinking pesado pode legitimamente demorar minutos), mas estes pedidos sao curtos e um
/// deles pendurado bloqueia o mutex de renovacao e, atras dele, todos os refines.
const TOKEN_TIMEOUT: Duration = Duration::from_secs(20);

/// Bytes de um CSPRNG do SO, em base64url. O `jitter01()` dos retries (nanos do relogio) NAO
/// serve aqui: um verifier adivinhavel destrui a unica protecao que o PKCE da.
fn random_b64(bytes: usize) -> Result<String, String> {
    let mut buf = vec![0u8; bytes];
    getrandom::getrandom(&mut buf).map_err(|e| format!("no secure randomness available: {e}"))?;
    Ok(wire::b64url_encode(&buf))
}

fn challenge_for(verifier: &str) -> String {
    wire::b64url_encode(&Sha256::digest(verifier.as_bytes()))
}

/// Corre o fluxo completo e grava a sessao. Devolve a conta ligada (quando o token a diz), para a
/// UI poder mostrar QUAL a conta em vez de um "signed in" anonimo.
pub async fn login(state: &AppState) -> Result<Option<String>, String> {
    let verifier = random_b64(32)?;
    let csrf_state = random_b64(16)?;
    let challenge = challenge_for(&verifier);

    // A porta nao e livre: tem de ser uma das que a OpenAI tem registadas como redirect valido.
    // Se as duas estiverem ocupadas, dizemos porque, em vez de falhar com um erro de socket que
    // nao ajuda ninguem (o suspeito habitual e um login do Codex CLI a decorrer).
    let mut bound = None;
    for port in wire::OAUTH_REDIRECT_PORTS {
        if let Ok(l) = TcpListener::bind(("127.0.0.1", port)).await {
            bound = Some((l, port));
            break;
        }
    }
    let Some((listener, port)) = bound else {
        return Err(format!(
            "Ports {} and {} are both busy. Close whatever is using them (another sign-in, maybe the Codex CLI) and try again.",
            wire::OAUTH_REDIRECT_PORTS[0], wire::OAUTH_REDIRECT_PORTS[1]
        ));
    };

    let url = wire::authorize_url(&challenge, &csrf_state, port);
    crate::commands::open_in_browser(&url)
        .map_err(|e| format!("couldn't open the browser: {e}"))?;
    log::info!("oauth: a espera do callback do browser na porta {port}");

    let code =
        match tokio::time::timeout(LOGIN_TIMEOUT, wait_for_code(&listener, &csrf_state)).await {
            Ok(r) => r?,
            Err(_) => return Err("Sign-in timed out. Try again when you're ready.".into()),
        };

    let session = exchange(state, &code, &verifier, port).await?;
    let account = session.account_id.clone();
    let label = session.account_label.clone();
    secrets::set_oauth(&session).map_err(|_| {
        "Signed in, but the credential vault refused to store the session. Try again.".to_string()
    })?;
    // Guarda o access token acabado de receber, senao o primeiro pedido a seguir ao login gastava
    // uma renovacao (e uma rotacao do refresh token) para chegar a um token que ja tinhamos.
    *state.oauth_access.lock().await = Some(crate::state::CachedAccess {
        token: session.access_token.clone(),
        account_id: session.account_id.clone(),
        expires_at_ms: session.expires_at_ms,
    });
    log::info!(
        "oauth: sessao gravada (conta={}, nome={})",
        account.as_deref().unwrap_or("desconhecida"),
        label.as_deref().unwrap_or("nao veio no token")
    );
    Ok(account)
}

/// Aceita ligacoes ate uma trazer o `code`. Nao para a primeira: o browser tambem pede o
/// `/favicon.ico` na mesma porta, e desistir nessa ligacao deixava o login pendurado.
async fn wait_for_code(listener: &TcpListener, expected_state: &str) -> Result<String, String> {
    loop {
        let (mut socket, _) = listener
            .accept()
            .await
            .map_err(|e| format!("the local sign-in server failed: {e}"))?;

        let Some(target) = read_request_target(&mut socket).await else {
            let _ = socket
                .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
                .await;
            continue;
        };
        if !target.starts_with(wire::OAUTH_REDIRECT_PATH) {
            // O browser tambem pede o /favicon.ico a esta porta. Responder e continuar a ouvir,
            // em vez de dar o login por terminado numa ligacao que nao era a boa.
            let _ = socket
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                .await;
            continue;
        }

        let params = query_params(&target);
        let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());

        // O `state` verifica-se PRIMEIRO, antes de responder e antes de olhar para qualquer outro
        // parametro. E ele que prova que este callback e a resposta ao NOSSO pedido: sem esta
        // ordem, uma ligacao qualquer a esta porta ficava com uma pagina de sucesso e conseguia
        // pôr texto a sua escolha no erro que mostramos ao utilizador como vindo da OpenAI.
        if get("state").as_deref() != Some(expected_state) {
            let _ = socket.write_all(page(false).as_bytes()).await;
            let _ = socket.flush().await;
            continue;
        }
        let ok = get("code").is_some();
        let _ = socket.write_all(page(ok).as_bytes()).await;
        let _ = socket.flush().await;

        if let Some(err) = get("error") {
            // Ja passou pelo `state`, portanto vem mesmo do servidor da OpenAI. Truncado para uma
            // mensagem de erro nao virar um paragrafo dentro de um toast.
            let short: String = err.chars().take(200).collect();
            return Err(format!("OpenAI refused the sign-in: {short}"));
        }
        if let Some(code) = get("code") {
            return Ok(code);
        }
        // Callback nosso, sem code e sem error: nao ha nada a fazer com isto, continua a ouvir
        // ate ao timeout em vez de declarar um sucesso que nao houve.
    }
}

/// Le o pedido ate ter a primeira linha completa e devolve o alvo (`/auth/callback?...`).
///
/// Em ciclo, e nao um `read` so: a linha pode chegar partida em varios segmentos TCP, e ler uma
/// vez podia corta-la a meio. Nesse caso o alvo saia truncado, a ligacao era tratada como "nao e
/// o callback", e o codigo de autorizacao (que so vem uma vez) perdia-se com o login a ficar
/// pendurado ate ao timeout, sem explicacao nenhuma.
async fn read_request_target(socket: &mut tokio::net::TcpStream) -> Option<String> {
    // Teto pequeno de proposito: ninguem precisa de mandar mais do que isto a esta porta, e sem
    // teto qualquer processo local podia fazer-nos crescer o buffer sem fim.
    const MAX: usize = 8192;
    let mut buf = Vec::with_capacity(1024);
    loop {
        if let Some(eol) = buf.windows(2).position(|w| w == b"\r\n") {
            let line = String::from_utf8_lossy(&buf[..eol]).into_owned();
            return line.split_whitespace().nth(1).map(str::to_string);
        }
        if buf.len() >= MAX {
            return None;
        }
        let mut chunk = [0u8; 1024];
        match socket.read(&mut chunk).await {
            Ok(0) | Err(_) => return None, // ligacao fechada antes da linha completa
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
}

fn page(ok: bool) -> String {
    let (title, body) = if ok {
        ("Signed in", "You can close this tab and go back to Ember.")
    } else {
        ("Sign-in failed", "Go back to Ember and try again.")
    };
    let html = format!(
        "<!doctype html><meta charset=utf-8><title>Ember</title>\
         <body style=\"font:16px system-ui;display:grid;place-items:center;height:100vh;margin:0\">\
         <div style=\"text-align:center\"><h1 style=\"font-size:1.2rem\">{title}</h1><p>{body}</p></div>"
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    )
}

fn query_params(target: &str) -> Vec<(String, String)> {
    let Some((_, query)) = target.split_once('?') else {
        return Vec::new();
    };
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), wire::percent_decode(v)))
        .collect()
}

/// Troca o `code` pelos tokens. O `verifier` viaja agora (e so agora), e e ele que prova que quem
/// troca o codigo e quem o pediu.
async fn exchange(
    state: &AppState,
    code: &str,
    verifier: &str,
    port: u16,
) -> Result<OAuthSession, String> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &wire::redirect_uri(port)),
        ("client_id", wire::OAUTH_CLIENT_ID),
        ("code_verifier", verifier),
    ];
    let resp = state
        .http
        .post(wire::OAUTH_TOKEN_URL)
        .form(&form)
        .timeout(TOKEN_TIMEOUT)
        .send()
        .await
        .map_err(|_| "Couldn't reach OpenAI to finish the sign-in.".to_string())?;
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    if status != 200 {
        // A mensagem do servidor e util (`invalid_grant`, `unauthorized_client`) e nao tem
        // segredos: e o erro, nunca o token.
        let detail = body
            .get("error_description")
            .or_else(|| body.get("error"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("no reason given");
        return Err(format!("OpenAI rejected the sign-in ({status}): {detail}"));
    }
    Ok(session_from(&body, None))
}

/// Monta a sessao a partir da resposta do servidor de tokens. `previous_refresh` cobre o caso da
/// renovacao em que a OpenAI nao devolve refresh novo: nesse caso o antigo continua valido, e
/// deita-lo fora era perder a sessao sem razao.
fn session_from(body: &serde_json::Value, previous_refresh: Option<&str>) -> OAuthSession {
    let s = |k: &str| {
        body.get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
    };
    let account_id = wire::jwt_claims(s("id_token"))
        .as_ref()
        .and_then(wire::chatgpt_account_id)
        // Alguns tokens so trazem a conta no access token.
        .or_else(|| {
            wire::jwt_claims(s("access_token"))
                .as_ref()
                .and_then(wire::chatgpt_account_id)
        });
    // O nome vem do mesmo sitio que a conta, e pelas mesmas razoes: o id_token primeiro, o
    // access token a seguir para os tokens que so la o trazem.
    let account_label = wire::jwt_claims(s("id_token"))
        .as_ref()
        .and_then(wire::chatgpt_account_label)
        .or_else(|| {
            wire::jwt_claims(s("access_token"))
                .as_ref()
                .and_then(wire::chatgpt_account_label)
        });
    let refresh = s("refresh_token");
    OAuthSession {
        access_token: s("access_token").to_string(),
        refresh_token: if refresh.is_empty() {
            previous_refresh.unwrap_or_default().to_string()
        } else {
            refresh.to_string()
        },
        account_id,
        account_label,
        expires_at_ms: wire::expires_at_ms(body, crate::now_ms()),
    }
}

/// Um access token valido, renovando-o se estiver a expirar. Devolve tambem a conta a cobrar,
/// que vai num header do pedido de inferencia.
///
/// Serializado por um mutex: um refine e um probe das settings podiam renovar ao mesmo tempo e,
/// como a OpenAI RODA o refresh token, o segundo gravava um token ja invalidado e a sessao
/// morria sem ninguem ter feito nada de errado.
pub async fn access_token(state: &AppState) -> Result<(String, Option<String>), CoreError> {
    token(state, false).await
}

/// Como `access_token`, mas renova SEMPRE. E o que o probe usa: sem isto, "validar" um token que
/// ainda esta dentro da validade nao fazia pedido nenhum e dizia que a sessao serve, mesmo que ela
/// tivesse sido revogada na conta ha uma hora. Um veredicto de saude que nao verificou nada e
/// pior do que nao haver veredicto: o Ember diria que tem fallback provado e nao tinha.
async fn token(
    state: &AppState,
    force_refresh: bool,
) -> Result<(String, Option<String>), CoreError> {
    let mut cache = state.oauth_access.lock().await;
    // O token em memoria serve enquanto for valido. Sem esta cache, e como o access token nao cabe
    // no cofre, CADA refine comecava por uma renovacao: um pedido a mais e uma rotacao de token a
    // mais para chegar exatamente ao mesmo sitio.
    if !force_refresh {
        if let Some(c) = cache.as_ref() {
            if !wire::token_needs_refresh(c.expires_at_ms, crate::now_ms()) {
                return Ok((c.token.clone(), c.account_id.clone()));
            }
        }
    }
    let Some(session) = secrets::get_oauth()? else {
        return Err(CoreError::Auth);
    };
    let next = refresh_session(state, &session).await?;
    let out = (next.access_token.clone(), next.account_id.clone());
    *cache = Some(crate::state::CachedAccess {
        token: next.access_token,
        account_id: next.account_id,
        expires_at_ms: next.expires_at_ms,
    });
    Ok(out)
}

/// Renova o par de tokens. Depois de esta funcao devolver `Ok`, o refresh novo JA esta gravado:
/// devolver o access antes de gravar era arriscar ficar com um refresh que o servidor ja rodou.
///
/// Corre numa TASK PROPRIA, e isso e o que impede o pior modo de falha desta feature. A OpenAI
/// roda o refresh token no instante em que responde 200: a partir dai o que temos gravado ja nao
/// serve, e so o que vem nesta resposta serve. Se o future fosse cancelado entre a resposta e a
/// gravacao (e e cancelado de verdade: a segunda tecla do atalho aborta o refine a meio, ver o
/// `select!` do flow.rs), ficavamos com um refresh token morto no cofre e a sessao acabava sem
/// ninguem ter feito nada de errado. Numa task, o cancelamento do chamador nao interrompe isto:
/// a gravacao chega sempre ao fim.
async fn refresh_session(
    state: &AppState,
    session: &OAuthSession,
) -> Result<OAuthSession, CoreError> {
    let http = state.http.clone();
    let session = session.clone();
    let handle = tokio::spawn(async move { refresh_inner(&http, &session).await });
    match handle.await {
        Ok(r) => r,
        // A task morreu (panico). Nao se sabe se o token foi rodado: transitorio, e o proximo
        // pedido volta a tentar com o que estiver gravado.
        Err(e) => {
            log::error!("oauth: a task de renovacao morreu: {e}");
            Err(CoreError::AllProvidersFailed)
        }
    }
}

async fn refresh_inner(
    http: &reqwest::Client,
    session: &OAuthSession,
) -> Result<OAuthSession, CoreError> {
    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", session.refresh_token.as_str()),
        ("client_id", wire::OAUTH_CLIENT_ID),
    ];
    let resp = http
        .post(wire::OAUTH_TOKEN_URL)
        .form(&form)
        // O cliente partilhado nao tem teto total (os refines em streaming podem demorar minutos
        // de propositio). Aqui tem de haver um: sem ele, um servidor de tokens que aceita e nunca
        // responde prendia o mutex de renovacao e todos os refines seguintes atras dele.
        .timeout(TOKEN_TIMEOUT)
        .send()
        .await;
    let Ok(resp) = resp else {
        // Sem rede nao se sabe nada sobre a sessao: nao se apaga nada, e reporta-se transitorio.
        return Err(CoreError::AllProvidersFailed);
    };
    let status = resp.status().as_u16();
    match wire::classify_refresh(status) {
        wire::RefreshOutcome::Refreshed => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let mut next = session_from(&body, Some(&session.refresh_token));
            if next.access_token.is_empty() {
                // 200 sem token nenhum: nao ha nada para gravar, e gravar por cima do que temos
                // apagaria uma sessao que ainda pode servir.
                log::warn!("oauth: renovacao devolveu 200 sem access token");
                return Err(CoreError::AllProvidersFailed);
            }
            // A conta so vem no id_token do login; numa renovacao pode nao vir, e a que temos
            // continua a servir.
            if next.account_id.is_none() {
                next.account_id = session.account_id.clone();
            }
            if next.account_label.is_none() {
                next.account_label = session.account_label.clone();
            }
            // A gravacao TEM de acontecer: o servidor ja rodou o token e o que esta no cofre
            // deixou de servir neste instante. Se falhar, dizer alto, porque a sessao acabou de
            // se perder e o utilizador vai ter de voltar a fazer login sem perceber porque.
            if let Err(e) = secrets::set_oauth(&next) {
                log::error!(
                    "oauth: o cofre recusou gravar o token renovado; a sessao vai ter de ser refeita ({e:?})"
                );
                return Err(CoreError::KeyStore);
            }
            log::info!("oauth: token renovado");
            Ok(next)
        }
        wire::RefreshOutcome::ReAuthRequired => {
            // A sessao acabou (revogada na conta, ou o refresh expirou). NAO se apaga o que esta
            // gravado: quem apaga credenciais e o utilizador, e o "sign out" esta a um clique.
            // O que se faz e dizer a verdade, para a UI mandar fazer login outra vez.
            log::warn!(
                "oauth: sessao ChatGPT ja nao e valida (HTTP {status}); e preciso novo login"
            );
            Err(CoreError::Auth)
        }
        wire::RefreshOutcome::Transient => {
            log::warn!("oauth: renovacao falhou por agora (HTTP {status})");
            Err(CoreError::AllProvidersFailed)
        }
    }
}

/// Probe da sessao, para a saude e para o botao de validar das settings.
///
/// Nao ha `GET /models` neste backend, por isso o teste e a propria renovacao: se o servidor de
/// tokens ainda aceita o nosso refresh, a sessao serve. Bate num endpoint diferente do refine,
/// como os outros probes, e nunca tira o provider da cadeia: so informa.
pub async fn probe(state: &AppState) -> crate::providers::Probe {
    use ember_core::health::KeyCheck;
    let empty = |check| crate::providers::Probe {
        check,
        models: Vec::new(),
    };
    match secrets::get_oauth() {
        Ok(None) => return empty(KeyCheck::Invalid),
        Err(_) => return empty(KeyCheck::NetworkError), // cofre ilegivel: nada se sabe da sessao.
        Ok(Some(_)) => {}
    }
    // `true` = renova mesmo que o token ainda esteja dentro da validade. E o ponto todo do probe:
    // provar contra o servidor que a sessao ainda serve, e nao repetir o que temos em cache.
    let (access, account) = match token(state, true).await {
        Ok(t) => t,
        Err(CoreError::Auth) => return empty(KeyCheck::Invalid),
        Err(_) => return empty(KeyCheck::NetworkError),
    };
    // A sessao serve. Aproveita-se para perguntar que modelos existem HOJE, pelo mesmo motivo por
    // que o probe dos outros providers aproveita o `GET /models`: uma lista escrita a mao
    // envelhece sozinha (esta ja nasceu com um modelo que entretanto foi retirado). Falhar aqui
    // nao invalida a sessao, so deixa a lista embutida em uso, e a UI diz que nao e viva.
    crate::providers::Probe {
        check: KeyCheck::Valid,
        models: discover_models(state, &access, account.as_deref()).await,
    }
}

/// Pergunta ao backend que modelos serve. Rota nao documentada, por isso: tenta os dois nomes
/// (como a inferencia faz), aceita varias formas de resposta, e qualquer falha devolve vazio, que
/// o resto do sistema le como "nao sei" e nao como "nao ha".
async fn discover_models(
    state: &AppState,
    access: &str,
    account: Option<&str>,
) -> Vec<ember_core::models::ModelInfo> {
    for url in wire::CODEX_MODELS_URLS {
        let mut req = state
            .http
            .get(url)
            .header("Authorization", format!("Bearer {access}"))
            .header("originator", wire::ORIGINATOR)
            .timeout(TOKEN_TIMEOUT);
        if let Some(acc) = account {
            req = req.header("ChatGPT-Account-Id", acc);
        }
        let Ok(resp) = req.send().await else { continue };
        if !resp.status().is_success() {
            log::debug!("codex: {url} respondeu {}", resp.status().as_u16());
            continue;
        }
        let Ok(body) = resp.json::<serde_json::Value>().await else {
            continue;
        };
        let ids = wire::parse_codex_models(&body);
        if ids.is_empty() {
            log::debug!("codex: {url} respondeu num formato que nao reconheco");
            continue;
        }
        log::info!("codex: {} modelos descobertos em {url}", ids.len());
        return ids
            .into_iter()
            .map(|id| ember_core::models::ModelInfo {
                generation: ember_core::models::parse_generation(&id),
                preview: id.contains("preview") || id.contains("spark"),
                // Nao e "free tier": e um plano pago. O que interessa aqui e que todos os modelos
                // desta lista custam o mesmo ao utilizador, ou seja, nada a mais.
                free_tier: false,
                display_name: id.clone(),
                id,
            })
            .collect();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_challenge_is_the_sha256_of_the_verifier_in_base64url() {
        // Vetor da RFC 7636 (apendice B): se isto partir, o servidor recusa todos os logins e o
        // erro que se ve nao aponta para aqui.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            challenge_for(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn random_values_are_long_and_never_repeat() {
        let a = random_b64(32).unwrap();
        let b = random_b64(32).unwrap();
        assert_ne!(a, b, "dois verifiers iguais seriam um CSPRNG partido");
        // A RFC exige entre 43 e 128 caracteres.
        assert!(
            (43..=128).contains(&a.len()),
            "verifier com {} chars",
            a.len()
        );
        assert!(!a.contains('='));
    }

    #[test]
    fn the_callback_query_is_read_even_with_escaped_values() {
        let p = query_params("/auth/callback?code=abc%2F123&state=xyz");
        let get = |k: &str| p.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("code"), Some("abc/123"));
        assert_eq!(get("state"), Some("xyz"));
        // Um callback sem query nenhuma nao rebenta: fica sem code e o fluxo diz que falhou.
        assert!(query_params("/auth/callback").is_empty());
    }

    #[test]
    fn a_refresh_without_a_new_token_keeps_the_old_one() {
        // A OpenAI nem sempre devolve refresh novo. Deitar fora o antigo nesse caso era perder a
        // sessao a meio de uma renovacao BEM sucedida.
        let body = serde_json::json!({ "access_token": "at-2", "expires_in": 3600 });
        let s = session_from(&body, Some("rt-antigo"));
        assert_eq!(s.refresh_token, "rt-antigo");
        assert_eq!(s.access_token, "at-2");

        // E quando devolve, o novo ganha.
        let rotated = serde_json::json!({ "access_token": "at-3", "refresh_token": "rt-novo" });
        assert_eq!(
            session_from(&rotated, Some("rt-antigo")).refresh_token,
            "rt-novo"
        );
    }
}
