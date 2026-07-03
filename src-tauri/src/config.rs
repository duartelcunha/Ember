//! Definicoes nao-secretas persistidas em disco (config.json no app config dir).
//! As chaves de API NAO vivem aqui: ficam no Windows Credential Manager (ver secrets.rs).

use ember_core::model::RefineMode;
use ember_core::providers::{DEFAULT_CLAUDE_MODEL, DEFAULT_GEMINI_MODEL};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// Limites do timing de captura. Fonte unica: `commands::set_capture_timing` e a
/// sanitizacao no load usam os mesmos, para a UI e o disco nunca divergirem.
pub const CAPTURE_POLLS: (u32, u32) = (5, 200);
pub const CAPTURE_STEP_MS: (u64, u64) = (1, 100);
pub const PASTE_SETTLE_MS: (u64, u64) = (0, 1000);

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

impl Config {
    /// Normaliza valores fora de gama ou vazios (config editada a mao, ou de uma versao
    /// anterior). Campos criticos vazios voltam ao default; o timing e clampado as gamas
    /// aceites pela UI, para um `capture_step_ms: 0` (busy-loop) nunca chegar ao runtime.
    fn sanitize(mut self) -> Self {
        let d = Config::default();
        if self.gemini_model.trim().is_empty() || self.gemini_model == "gemini-3.5-flash" {
            self.gemini_model = d.gemini_model;
        }
        if self.claude_model.trim().is_empty() {
            self.claude_model = d.claude_model;
        }
        if self.hotkey.trim().is_empty() {
            self.hotkey = d.hotkey;
        }
        if self.thinking_level.trim().is_empty() {
            self.thinking_level = d.thinking_level;
        }
        self.capture_polls = self.capture_polls.clamp(CAPTURE_POLLS.0, CAPTURE_POLLS.1);
        self.capture_step_ms = self
            .capture_step_ms
            .clamp(CAPTURE_STEP_MS.0, CAPTURE_STEP_MS.1);
        self.paste_settle_ms = self
            .paste_settle_ms
            .clamp(PASTE_SETTLE_MS.0, PASTE_SETTLE_MS.1);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_clamps_timing_out_of_range() {
        let mut c = Config::default();
        c.capture_step_ms = 0; // busy-loop se chegasse ao runtime
        c.capture_polls = 100_000;
        c.paste_settle_ms = 99_999;
        let c = c.sanitize();
        assert_eq!(c.capture_step_ms, CAPTURE_STEP_MS.0);
        assert_eq!(c.capture_polls, CAPTURE_POLLS.1);
        assert_eq!(c.paste_settle_ms, PASTE_SETTLE_MS.1);
    }

    #[test]
    fn sanitize_refills_empty_critical_strings() {
        let mut c = Config::default();
        c.gemini_model = "  ".into();
        c.hotkey = String::new();
        c.thinking_level = String::new();
        let d = Config::default();
        let c = c.sanitize();
        assert_eq!(c.gemini_model, d.gemini_model);
        assert_eq!(c.hotkey, d.hotkey);
        assert_eq!(c.thinking_level, d.thinking_level);
    }

    #[test]
    fn sanitize_leaves_valid_config_untouched() {
        let c = Config::default();
        assert_eq!(c.clone().sanitize(), c);
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
    let Ok(s) = fs::read_to_string(&p) else {
        return Config::default();
    };
    match serde_json::from_str::<Config>(&s) {
        Ok(cfg) => cfg.sanitize(),
        Err(_) => {
            // Ficheiro corrompido: preserva-o (config.json.bak) antes de seguir com defaults,
            // senao o proximo save escrevia por cima e a config do utilizador perdia-se sem
            // deixar rasto para recuperar.
            let _ = fs::rename(&p, p.with_extension("json.bak"));
            Config::default()
        }
    }
}

/// Grava a config no disco (cria o diretorio se preciso).
pub fn save(app: &AppHandle, cfg: &Config) -> std::io::Result<()> {
    if let Some(p) = config_path(app) {
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir)?;
        }
        // Serializa antes de escrever: um erro (improvavel) nunca deve truncar o ficheiro
        // para vazio (o antigo unwrap_or_default escrevia "" e apagava tudo).
        let s = serde_json::to_string_pretty(cfg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(p, s)?;
    }
    Ok(())
}
