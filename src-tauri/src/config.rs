//! Definicoes nao-secretas persistidas em disco (config.json no app config dir).
//! As chaves de API NAO vivem aqui: ficam no Windows Credential Manager (ver secrets.rs).

use ember_core::model::RefineMode;
use ember_core::providers::{DEFAULT_CLAUDE_MODEL, DEFAULT_GEMINI_MODEL};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub gemini_model: String,
    pub claude_model: String,
    pub hotkey: String,
    pub autostart: bool,
    pub mode: RefineMode,
    /// Raciocinio alargado do Gemini (default on). Mais qualidade, um pouco mais lento.
    pub thinking_enabled: bool,
    /// Nivel de thinking para Gemini 3.x: "minimal"|"low"|"medium"|"high".
    pub thinking_level: String,
    /// Override do perfil escrito nas settings. `None` = usar o CLAUDE.md detetado ou o default.
    pub profile_override: Option<String>,
    /// Se `true`, ignora o CLAUDE.md e usa o perfil de qualidade por defeito.
    pub ignore_claude_md: bool,
    /// Deteta terminais em foco e usa Ctrl+Shift+C/V (default on). Desliga se uma app
    /// nao-terminal for mal-classificada.
    pub terminal_handling: bool,
    /// Quantas vezes faz poll ao clipboard a espera da copia (intervalo de `capture_step_ms`).
    pub capture_polls: u32,
    /// Intervalo entre polls de captura, em ms.
    pub capture_step_ms: u64,
    /// Tempo de espera apos o paste antes de restaurar o clipboard original, em ms.
    pub paste_settle_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gemini_model: DEFAULT_GEMINI_MODEL.to_string(),
            claude_model: DEFAULT_CLAUDE_MODEL.to_string(),
            hotkey: "CmdOrCtrl+Shift+Space".to_string(),
            autostart: false,
            mode: RefineMode::Adaptive,
            thinking_enabled: true,
            thinking_level: "high".to_string(),
            profile_override: None,
            ignore_claude_md: false,
            terminal_handling: true,
            capture_polls: 30,
            capture_step_ms: 10,
            paste_settle_ms: 90,
        }
    }
}

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("config.json"))
}

/// Carrega a config do disco; devolve defaults se nao existir ou estiver corrompida.
pub fn load(app: &AppHandle) -> Config {
    let Some(p) = config_path(app) else {
        return Config::default();
    };
    match fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

/// Grava a config no disco (cria o diretorio se preciso).
pub fn save(app: &AppHandle, cfg: &Config) -> std::io::Result<()> {
    if let Some(p) = config_path(app) {
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir)?;
        }
        let s = serde_json::to_string_pretty(cfg).unwrap_or_default();
        fs::write(p, s)?;
    }
    Ok(())
}
