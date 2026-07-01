//! Storage seguro das chaves de API no Windows Credential Manager (via keyring).
//! As chaves NUNCA passam pela camada JS nem ficam em texto/config.

use ember_core::model::Provider;

const SERVICE: &str = "Ember";

fn entry_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Gemini => "gemini_api_key",
        Provider::Claude => "claude_api_key",
    }
}

fn entry(provider: Provider) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, entry_name(provider))
}

pub fn get(provider: Provider) -> Option<String> {
    entry(provider).ok()?.get_password().ok()
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

pub fn has(provider: Provider) -> bool {
    get(provider).is_some()
}
