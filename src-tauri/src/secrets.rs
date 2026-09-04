//! Storage seguro das chaves de API no Windows Credential Manager (via keyring).
//! As chaves NUNCA passam pela camada JS nem ficam em texto/config.

use ember_core::model::Provider;

const SERVICE: &str = "Ember";

/// Entrada do cofre do provider Claude, que existiu ate a Anthropic passar a ser mais um servico
/// do slot OpenAI-compativel. Fica aqui nomeada para se saber que pode existir uma chave orfa no
/// Credential Manager de quem ja usava a app. NAO a apagamos: e uma credencial do utilizador, e
/// apagar segredos sem ele pedir e a decisao errada por omissao. O Diagnostics menciona-a.
const LEGACY_CLAUDE_ENTRY: &str = "claude_api_key";

/// Ficou uma chave do Claude no cofre de quem ja usava a app? So para o Diagnostics: e melhor
/// dizer-lhe que ela la esta do que deixar uma credencial esquecida sem ninguem saber.
pub fn has_legacy_claude_key() -> bool {
    keyring::Entry::new(SERVICE, LEGACY_CLAUDE_ENTRY)
        .and_then(|e| e.get_password())
        .is_ok()
}

fn entry_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Gemini => "gemini_api_key",
        Provider::OpenAi => "openai_api_key",
    }
}

fn entry(provider: Provider) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, entry_name(provider))
}

/// Como `get`, mas distingue "chave nao configurada" (`Ok(None)`) de uma falha real do
/// cofre (`Err`). Sem isto, um Credential Manager bloqueado devolvia `None` e o provider
/// era silenciosamente retirado da cadeia (degradava em silencio, contra a regra da casa).
pub fn try_get(provider: Provider) -> Result<Option<String>, ember_core::CoreError> {
    let entry = entry(provider).map_err(|_| ember_core::CoreError::KeyStore)?;
    match entry.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(ember_core::CoreError::KeyStore),
    }
}

/// `try_get` em bool, para a UI/pre-validacao que so quer saber se ha chave. NAO engole erros
/// do cofre: propaga `KeyStore` (regra de resiliencia). Substitui o antigo `has`/`get`, que
/// colapsavam uma falha do cofre em `false`/`None` e faziam a UI mentir "sem chave configurada".
pub fn try_has(provider: Provider) -> Result<bool, ember_core::CoreError> {
    Ok(try_get(provider)?.is_some())
}

pub fn set(provider: Provider, key: &str) -> keyring::Result<()> {
    entry(provider)?.set_password(key)
}

// ---------------------------------------------------------------------------------------
// Sessao ChatGPT (modo subscricao do slot OpenAI)
// ---------------------------------------------------------------------------------------

/// Entradas separadas e nao um JSON so: o Credential Manager do Windows tem um teto por credencial
/// e estes tokens sao JWTs longos. Nao e teorico, aconteceu: o access token sozinho nao coube.
///
/// Por isso o access token JA NAO SE GUARDA AQUI. Vive so em memoria enquanto a app corre (ver
/// `AppState::oauth_access`), o que alias e o sitio certo para ele: expira dentro de horas e
/// renova-se num pedido. O que tem MESMO de sobreviver ao fecho da app e o refresh token, que e
/// pequeno e e o unico sem o qual a sessao se perde para sempre.
///
/// A entrada antiga fica nomeada para o logout a poder apagar em quem a chegou a gravar.
const OAUTH_ACCESS_LEGACY: &str = "openai_oauth_access";
const OAUTH_REFRESH: &str = "openai_oauth_refresh";
const OAUTH_META: &str = "openai_oauth_meta";

/// A sessao ChatGPT guardada. O `access_token` so esta preenchido acabado de vir do servidor: o
/// que se le do cofre traz sempre vazio, porque nao e la que ele vive (ver acima).
#[derive(Debug, Clone)]
pub struct OAuthSession {
    pub access_token: String,
    pub refresh_token: String,
    pub account_id: Option<String>,
    /// O que se MOSTRA nas settings (email, ou nome). Opcional de verdade: sessoes gravadas antes
    /// de isto existir nao o tem, e uma renovacao pode nao trazer token nenhum com claims.
    pub account_label: Option<String>,
    pub expires_at_ms: u64,
}

fn oauth_entry(name: &str) -> Result<keyring::Entry, ember_core::CoreError> {
    keyring::Entry::new(SERVICE, name).map_err(|_| ember_core::CoreError::KeyStore)
}

fn read(name: &str) -> Result<Option<String>, ember_core::CoreError> {
    match oauth_entry(name)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(ember_core::CoreError::KeyStore),
    }
}

/// A sessao gravada, ou `None` se nunca houve login. `Err` so em falha real do cofre: como no
/// `try_get`, um cofre bloqueado nunca se disfarca de "nao configurado".
///
/// Sem `refresh_token` nao ha sessao nenhuma: o access token sozinho expira em horas e nao ha
/// forma de o renovar. Devolver essa meia-sessao punha a app a garantir um login que ja nao serve.
pub fn get_oauth() -> Result<Option<OAuthSession>, ember_core::CoreError> {
    let Some(refresh_token) = read(OAUTH_REFRESH)? else {
        return Ok(None);
    };
    let meta: serde_json::Value = read(OAUTH_META)?
        .and_then(|m| serde_json::from_str(&m).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    Ok(Some(OAuthSession {
        // Vazio de proposito: o access token nao vem do cofre. Quem o quer chama
        // `oauth::access_token`, que o tem em memoria ou o renova.
        access_token: String::new(),
        refresh_token,
        account_id: meta
            .get("account_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        account_label: meta
            .get("account_label")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        expires_at_ms: meta
            .get("expires_at_ms")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    }))
}

/// Grava a sessao. O refresh token vai PRIMEIRO e de forma bloqueante: e a unica peca sem a qual
/// a sessao se perde para sempre, e a OpenAI roda-o a cada renovacao. Se o cofre falhar a meio,
/// e melhor ter o refresh gravado e o access por gravar (renova-se) do que o contrario.
pub fn set_oauth(s: &OAuthSession) -> Result<(), ember_core::CoreError> {
    let store = |name: &str, v: &str| -> Result<(), ember_core::CoreError> {
        oauth_entry(name)?
            .set_password(v)
            .map_err(|_| ember_core::CoreError::KeyStore)
    };
    store(OAUTH_REFRESH, &s.refresh_token)?;
    let meta = serde_json::json!({
        "account_id": s.account_id,
        "account_label": s.account_label,
        "expires_at_ms": s.expires_at_ms,
    });
    store(OAUTH_META, &meta.to_string())?;
    Ok(())
}

/// Apaga a sessao. Ao contrario da chave orfa do Claude, isto apaga-se sem hesitar: quem carrega
/// em "sign out" esta a pedir exatamente isto.
pub fn clear_oauth() -> Result<(), ember_core::CoreError> {
    for name in [OAUTH_ACCESS_LEGACY, OAUTH_REFRESH, OAUTH_META] {
        match oauth_entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => return Err(ember_core::CoreError::KeyStore),
        }
    }
    Ok(())
}

/// Ha sessao ChatGPT utilizavel? Usado pela saude e pela construcao da cadeia, onde "configurado"
/// em modo subscricao quer dizer isto e nao "tem chave de API".
pub fn has_oauth() -> Result<bool, ember_core::CoreError> {
    Ok(get_oauth()?.is_some())
}

pub fn delete(provider: Provider) -> keyring::Result<()> {
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e),
    }
}
