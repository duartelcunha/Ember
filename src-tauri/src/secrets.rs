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

pub fn delete(provider: Provider) -> keyring::Result<()> {
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e),
    }
}
