//! Persistencia da cache de refinados (`ember_core::RefineCache`) em disco.
//!
//! Vive ao lado do `config.json`, em `%APPDATA%\com.deleg8lab.ember\refine_cache.json`. Guarda
//! TEXTO DO UTILIZADOR, e por isso ha um interruptor (`keep_results`) nas settings; ao contrario
//! do `prompts.jsonl`, que e opt-in porque existe para nos estudarmos os prompts, este vem
//! ligado, porque existe para o utilizador: sem ele, fechar a app deita fora refinados ja pagos
//! e o atalho seguinte volta a pagar.
//!
//! Escrita atomica (tmp + rename): um corte de energia a meio nao deixa um JSON truncado que
//! depois falharia a ler e apagaria a cache toda em silencio.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use ember_core::RefineCache;

const FILE: &str = "refine_cache.json";

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(FILE))
}

/// Le a cache do disco. Qualquer falha (ficheiro inexistente, JSON de uma versao antiga) da uma
/// cache vazia: perder a cache e um contratempo, nao um erro que valha a pena atirar ao arranque.
pub fn load(app: &AppHandle) -> RefineCache {
    let Some(p) = path(app) else {
        return RefineCache::default();
    };
    match std::fs::read_to_string(&p) {
        Ok(raw) => match serde_json::from_str::<RefineCache>(&raw) {
            Ok(mut c) => {
                c.evict_expired(now_ms());
                log::info!("refine cache: {} entradas carregadas", c.len());
                c
            }
            Err(e) => {
                log::warn!("refine cache: ficheiro ilegivel ({e}); a comecar vazia");
                RefineCache::default()
            }
        },
        Err(_) => RefineCache::default(),
    }
}

/// Grava a cache. Best-effort: se falhar, a cache continua a valer em memoria ate a app fechar.
pub fn save(app: &AppHandle, cache: &RefineCache) {
    let Some(p) = path(app) else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(json) = serde_json::to_string(cache) else {
        return;
    };
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        if let Err(e) = std::fs::rename(&tmp, &p) {
            log::warn!("refine cache: nao consegui gravar ({e})");
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

/// Apaga o ficheiro. Chamado quando o utilizador desliga o `keep_results`: desligar tem de tirar
/// o que ja la esta, senao o interruptor so vale para o futuro e o texto antigo fica em disco.
pub fn forget(app: &AppHandle) {
    if let Some(p) = path(app) {
        let _ = std::fs::remove_file(p);
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
