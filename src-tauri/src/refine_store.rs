//! Opt-in authenticated result storage. Plaintext legacy data is never silently migrated.
use ember_core::RefineCache;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use std::{path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager};

const FILE: &str = "refine_cache.enc";
const HEADER: &[u8] = b"EMBER-CACHE-1";
const MAX_BYTES: u64 = 32 * 1024 * 1024;
static WRITER: Mutex<()> = Mutex::new(());

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(FILE))
}

fn storage_key(create: bool) -> Result<[u8; 32], String> {
    let entry = keyring::Entry::new("Ember", "result_cache_key")
        .map_err(|_| "Credential vault unavailable")?;
    match entry.get_secret() {
        Ok(bytes) => bytes.try_into().map_err(|_| "Invalid storage key".into()),
        Err(keyring::Error::NoEntry) if create => {
            let mut key = [0u8; 32];
            getrandom::getrandom(&mut key).map_err(|_| "Randomness unavailable")?;
            entry
                .set_secret(&key)
                .map_err(|_| "Credential vault rejected storage key")?;
            Ok(key)
        }
        Err(_) => Err("Storage key unavailable".into()),
    }
}

fn encrypt(key: &[u8; 32], plain: &[u8]) -> Result<Vec<u8>, String> {
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce).map_err(|_| "Randomness unavailable")?;
    let cipher = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).map_err(|_| "Invalid key")?);
    let mut body = plain.to_vec();
    cipher
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(HEADER),
            &mut body,
        )
        .map_err(|_| "Encryption failed")?;
    let mut out = HEADER.to_vec();
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&body);
    Ok(out)
}

fn decrypt(key: &[u8; 32], bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < HEADER.len() + 12 + 16 || !bytes.starts_with(HEADER) {
        return Err("Invalid cache envelope".into());
    }
    let nonce: [u8; 12] = bytes[HEADER.len()..HEADER.len() + 12]
        .try_into()
        .map_err(|_| "Invalid nonce")?;
    let mut body = bytes[HEADER.len() + 12..].to_vec();
    let cipher = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).map_err(|_| "Invalid key")?);
    let plain = cipher
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(HEADER),
            &mut body,
        )
        .map_err(|_| "Cache authentication failed")?;
    Ok(plain.to_vec())
}

pub fn load(app: &AppHandle) -> RefineCache {
    let _writer = WRITER.lock().unwrap_or_else(|e| e.into_inner());
    let result = (|| -> Result<RefineCache, String> {
        let p = path(app).ok_or("Cache directory unavailable")?;
        if !p.exists() {
            return Ok(RefineCache::default());
        }
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(&p)
            .map_err(|_| "Cache unavailable")?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cache read failed")?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Cache exceeds size limit".into());
        }
        let plain = decrypt(&storage_key(false)?, &bytes)?;
        let mut cache: RefineCache =
            serde_json::from_slice(&plain).map_err(|_| "Invalid cache data")?;
        cache.evict_expired(now_ms());
        // Persist eviction immediately, rather than retaining expired plaintext indefinitely.
        if crate::config::load(app).keep_results {
            save_locked(app, &cache)?;
        }
        Ok(cache)
    })();
    result.unwrap_or_else(|error| {
        log::warn!("result storage: {error}");
        RefineCache::default()
    })
}

fn save_locked(app: &AppHandle, cache: &RefineCache) -> Result<(), String> {
    let p = path(app).ok_or("Cache directory unavailable")?;
    let mut live = cache.clone();
    live.evict_expired(now_ms());
    let plain = serde_json::to_vec(&live).map_err(|_| "Cache serialization failed")?;
    if plain.len() as u64 > MAX_BYTES - 128 {
        return Err("Cache exceeds size limit".into());
    }
    let encrypted = encrypt(&storage_key(true)?, &plain)?;
    crate::atomic_file::write(&p, &encrypted).map_err(|_| "Cache write failed".into())
}

pub fn save(
    app: &AppHandle,
    key: &ember_core::CacheKey,
    entry: &ember_core::CacheEntry,
    generation: u64,
) {
    let _writer = WRITER.lock().unwrap_or_else(|e| e.into_inner());
    let state = app.state::<crate::state::AppState>();
    if !crate::config::load(app).keep_results
        || state
            .retention_generation
            .load(std::sync::atomic::Ordering::SeqCst)
            != generation
    {
        return;
    }
    // Session memory contains results produced while retention was disabled. Never serialize
    // that aggregate, even during a later authorized request.
    let Ok(mut cache) = state.persisted_store.lock() else {
        return;
    };
    let mut next = cache.clone();
    next.insert(key.clone(), entry.clone(), now_ms());
    match save_locked(app, &next) {
        Ok(()) => *cache = next,
        Err(error) => log::warn!("result storage: {error}"),
    }
}

/// Expire memory and encrypted results even when no new refinement is requested.
pub fn maintain(app: &AppHandle) {
    let _writer = WRITER.lock().unwrap_or_else(|e| e.into_inner());
    let state = app.state::<crate::state::AppState>();
    let now = now_ms();
    if let Ok(mut memory) = state.store.lock() {
        memory.evict_expired(now);
    }
    if !crate::config::load(app).keep_results {
        return;
    }
    let Ok(mut retained) = state.persisted_store.lock() else {
        return;
    };
    let before = retained.len();
    retained.evict_expired(now);
    if retained.len() != before {
        if let Err(error) = save_locked(app, &retained) {
            log::warn!("result expiry: {error}");
        }
    }
}

pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let _writer = WRITER.lock().map_err(|_| "Result storage unavailable")?;
    if enabled {
        storage_key(true)?;
    }
    let mut cfg = crate::config::load(app);
    cfg.keep_results = enabled;
    crate::config::save(app, &cfg).map_err(|e| e.to_string())?;
    app.state::<crate::state::AppState>()
        .retention_generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if !enabled {
        *app.state::<crate::state::AppState>()
            .persisted_store
            .lock()
            .map_err(|_| "Result storage unavailable")? = RefineCache::default();
        forget_locked(app)?;
    }
    Ok(())
}

fn forget_locked(app: &AppHandle) -> Result<(), String> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|_| "Cache directory unavailable")?;
    for name in [FILE, "refine_cache.json"] {
        match std::fs::remove_file(directory.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Could not delete retained results".into()),
        }
    }
    Ok(())
}

pub fn now_ms() -> u64 {
    crate::now_ms()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authenticated_storage_roundtrips_and_rejects_tampering() {
        let key = [7; 32];
        let plain = b"synthetic private text";
        let sealed = encrypt(&key, plain).unwrap();
        assert!(!sealed.windows(plain.len()).any(|w| w == plain));
        assert_eq!(decrypt(&key, &sealed).unwrap(), plain);
        assert!(decrypt(&[8; 32], &sealed).is_err());
        let mut altered = sealed.clone();
        *altered.last_mut().unwrap() ^= 1;
        assert!(decrypt(&key, &altered).is_err());
        assert!(decrypt(&key, &sealed[..15]).is_err());
        assert_ne!(encrypt(&key, plain).unwrap(), sealed);
    }
}

#[tauri::command]
pub fn legacy_results_present(app: AppHandle) -> bool {
    app.path()
        .app_config_dir()
        .ok()
        .is_some_and(|d| d.join("refine_cache.json").exists())
}

#[tauri::command]
pub fn delete_legacy_results(app: AppHandle) -> Result<(), String> {
    let _writer = WRITER.lock().map_err(|_| "Result storage unavailable")?;
    let path = app
        .path()
        .app_config_dir()
        .map_err(|_| "Cache directory unavailable")?
        .join("refine_cache.json");
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Legacy results could not be deleted".into()),
    }
}
