//! Definicoes nao-secretas persistidas em disco (config.json no app config dir).
//! As chaves de API NAO vivem aqui: ficam no Windows Credential Manager (ver secrets.rs).

use ember_core::model::RefineMode;
use ember_core::providers::{DEFAULT_CLAUDE_MODEL, DEFAULT_GEMINI_MODEL, DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub gemini_model: String,
    pub claude_model: String,
    /// Modelo do provider OpenAI-compatible (default fallback). Id livre; a UI aceita Custom.
    pub openai_model: String,
    /// Base URL do provider OpenAI-compatible. Default: OpenRouter. Serve DeepSeek/Groq/Ollama.
    pub openai_base_url: String,
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
    /// Modo debug: abre as devtools nas settings e mostra o painel de diagnostico. O ficheiro
    /// de log capta sempre; isto controla a superficie visivel ao utilizador. Default off.
    pub debug_mode: bool,
    /// Contexto de projeto: deteta o CLAUDE.md/AGENTS.md/GEMINI.md do projeto em foco e junta-o
    /// ao perfil global. Default OFF (privacidade: um repo de cliente nao deve ir para o LLM sem
    /// o utilizador ligar isto). So-leitura de ficheiros de contexto conhecidos, com redacao.
    pub project_context: bool,
    /// Preview antes de colar: mostra um pill de aprovacao apos refinar e cola so no Enter (Esc
    /// mantem o original). Default OFF. So Windows (usa um keyboard hook para capturar Enter/Esc).
    pub preview_before_paste: bool,
    /// Tema visual da janela de Settings: "dark" (default) ou "cream". So afeta as Settings; a
    /// overlay/splash mantem a identidade dark de marca.
    pub theme: String,
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
            openai_model: DEFAULT_OPENAI_MODEL.to_string(),
            openai_base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
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
            debug_mode: false,
            project_context: false,
            preview_before_paste: false,
            theme: "cream".to_string(),
        }
    }
}

impl Config {
    /// Normaliza valores fora de gama ou vazios (config editada a mao, ou de uma versao
    /// anterior). Campos criticos vazios voltam ao default; o timing e clampado as gamas
    /// aceites pela UI, para um `capture_step_ms: 0` (busy-loop) nunca chegar ao runtime.
    fn sanitize(mut self) -> Self {
        let d = Config::default();
        // Migracao: `gemini-3.5-flash` foi um default fantasma (modelo inexistente) de uma
        // versao anterior; reescreve-o para o default valido para nao ir parar ao pedido.
        if self.gemini_model.trim().is_empty() || self.gemini_model == "gemini-3.5-flash" {
            self.gemini_model = d.gemini_model;
        }
        if self.claude_model.trim().is_empty() {
            self.claude_model = d.claude_model;
        }
        if self.openai_model.trim().is_empty() {
            self.openai_model = d.openai_model;
        }
        // Base URL vazia -> default; barra final removida (nao duplicar no caminho do endpoint).
        let base = self.openai_base_url.trim().trim_end_matches('/');
        self.openai_base_url = if base.is_empty() {
            d.openai_base_url
        } else {
            base.to_string()
        };
        if self.hotkey.trim().is_empty() {
            self.hotkey = d.hotkey;
        }
        if self.thinking_level.trim().is_empty() {
            self.thinking_level = d.thinking_level;
        }
        if self.theme != "dark" && self.theme != "cream" {
            self.theme = d.theme;
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

    #[test]
    fn sanitize_refills_empty_openai_fields_and_trims_base_url_slash() {
        let mut c = Config::default();
        c.openai_model = "  ".into();
        c.openai_base_url = "https://openrouter.ai/api/v1/".into();
        let d = Config::default();
        let c = c.sanitize();
        assert_eq!(c.openai_model, d.openai_model);
        // barra final removida
        assert_eq!(c.openai_base_url, "https://openrouter.ai/api/v1");
        // base URL totalmente vazia -> default
        let mut c2 = Config::default();
        c2.openai_base_url = "   ".into();
        assert_eq!(c2.sanitize().openai_base_url, d.openai_base_url);
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
        Err(e) => {
            // Ficheiro corrompido: preserva-o (config.json.bak) antes de seguir com defaults,
            // senao o proximo save escrevia por cima e a config do utilizador perdia-se sem
            // deixar rasto para recuperar.
            log::warn!("config: corrupt config.json ({e}); backing up to .bak and using defaults");
            if let Err(e) = fs::rename(&p, p.with_extension("json.bak")) {
                log::warn!("config: could not back up corrupt config: {e}");
            }
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
