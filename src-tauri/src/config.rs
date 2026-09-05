//! Definicoes nao-secretas persistidas em disco (config.json no app config dir).
//! As chaves de API NAO vivem aqui: ficam no Windows Credential Manager (ver secrets.rs).

use ember_core::model::{Provider, RefineMode};
use ember_core::providers::{DEFAULT_GEMINI_MODEL, DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL};
use serde::{Deserialize, Serialize};

/// Como o slot de fallback se autentica.
///
/// Nao e um provider novo: e a MESMA familia OpenAI-compativel com outra credencial. Por isso vive
/// aqui como um campo e nao como uma variante do `Provider`, que e um contrato IPC pinado por
/// testes e atravessaria o codigo todo por causa de uma escolha que ocupa a mesma linha da UI que
/// o Groq ou o OpenRouter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OpenAiAuth {
    /// Chave de API BYOK (o que sempre houve).
    #[default]
    ApiKey,
    /// Login com a conta ChatGPT: os refines saem do plano que o utilizador ja paga. Caminho NAO
    /// oficial (ver `ember_core::codex`), e a UI diz isso antes de ele escolher.
    ChatGpt,
}
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(default)]
    pub schema_version: u32,
    pub revision: u64,
    pub gemini_model: String,
    /// Modelo do provider OpenAI-compatible (default fallback). Id livre; a UI aceita Custom.
    pub openai_model: String,
    /// O modelo do Gemini e escolhido pelo Ember (o melhor gratuito que o provider anunciar) ou
    /// foi fixado a mao pelo utilizador?
    ///
    /// Default `true`: o utilizador nao tem de perceber de ids de modelos para a app funcionar
    /// bem, e a escolha certa muda quando a Google lanca uma geracao nova. Passa a `false` no
    /// instante em que ele escolhe um a mao, e a partir dai nao lhe mexemos mais. Sem esta flag
    /// nao havia forma de distinguir "ainda nao escolheu" de "escolheu este de proposito", e a
    /// descoberta acabava por lhe apagar a escolha a cada arranque.
    pub gemini_model_auto: bool,
    /// Base URL do provider OpenAI-compatible. Default: OpenRouter. Serve DeepSeek/Groq/Ollama.
    pub openai_base_url: String,
    /// Como o slot de fallback se autentica: chave de API (default) ou a subscricao ChatGPT.
    /// Uma config gravada antes disto existir nao tem o campo e cai no default, que e o
    /// comportamento de sempre.
    pub openai_auth: OpenAiAuth,
    /// Qual dos dois providers e tentado PRIMEIRO. Default Gemini, que e gratuito e portanto a
    /// escolha certa para quem nunca pensou no assunto. Quem ja paga uma subscricao prefere
    /// gasta-la em vez de esperar por um free tier: so muda a ordem, os dois slots ficam iguais.
    pub primary_provider: Provider,
    /// Atalho principal: dispara o refine no `mode` escolhido nas settings.
    pub hotkey: String,
    /// Atalhos que fixam um modo, para escolher no momento em que se dispara em vez de ter de
    /// abrir as settings a meio de um pensamento. Vazio = nao registado.
    ///
    /// Vazios POR DEFEITO, e isso foi uma decisao tomada com evidencia, nao por cautela: a
    /// escolha obvia (`CmdOrCtrl+Alt+Space`, o atalho principal mais um Alt) ja estava ocupada
    /// na primeira maquina onde correu, e o registo e tudo-ou-nada. Um default que colide
    /// transforma uma instalacao limpa num arranque com aviso, e por um atalho que a pessoa
    /// nem pediu. Quem os quer poe a combinacao que sabe estar livre, nas settings.
    pub hotkey_polish: String,
    pub hotkey_turbo: String,
    /// Atalho do picker de projetos. Vazio = nao registado, pelas mesmas razoes dos de modo.
    pub hotkey_picker: String,
    pub autostart: bool,
    pub mode: RefineMode,
    /// Raciocinio alargado do Gemini (default on). Mais qualidade, um pouco mais lento.
    pub thinking_enabled: bool,
    /// Nivel de thinking para Gemini 3.x: "minimal"|"low"|"medium"|"high".
    pub thinking_level: String,
    /// Reviewed profile text. None uses the built-in default, never ambient files.
    pub profile_override: Option<String>,
    /// Provenance of explicitly imported, reviewed snapshots. Never a live filesystem grant.
    pub profile_sources: Vec<ember_core::profile_import::Source>,
    /// Legacy discovery flag, retained to explain why automatic loading was disabled.
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
    /// Grava num ficheiro o que foi enviado ao modelo e o que ele respondeu, para se poder
    /// melhorar o prompting com casos reais. Default OFF, e por privacidade: ao contrario do log
    /// normal, isto leva o TEXTO do utilizador para disco. Ver `prompt_log`.
    pub save_prompts: bool,
    /// Results remain in session memory unless encrypted retention is explicitly enabled.
    /// Disabling retention deletes retained files; legacy data is preserved during migration.
    pub keep_results: bool,
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
    /// Se nao havia nada selecionado, seleciona o campo em foco (Ctrl+A) e refina-o todo.
    /// Default ON: e o caso dominante fora de terminais (escreveste o prompt na caixa e nunca o
    /// selecionaste). Uma captura por esta via passa SEMPRE pelo gate de preview, mesmo com o
    /// preview global desligado, porque o Ctrl+A pode ter apanhado mais do que um campo.
    pub select_all_fallback: bool,
    /// Teto de chars de uma captura vinda do select-all. Acima disto assumimos que o foco nao
    /// estava num campo e que o Ctrl+A agarrou a pagina toda, e abortamos sem colar.
    pub select_all_max_chars: usize,
    /// Projetos registados pelo utilizador. Cada um traz um brief que entra no prompt quando esse
    /// projeto esta ativo. Vazio por defeito; uma config anterior a isto nem tem o campo.
    pub projects: Vec<ember_core::projects::Project>,
    /// O id do projeto ativo. `None` = nenhum, e ai vale a detecao pela janela (se ligada).
    pub active_project: Option<String>,
}

/// Limites do timing de captura. Fonte unica: `commands::set_capture_timing` e a
/// sanitizacao no load usam os mesmos, para a UI e o disco nunca divergirem.
pub const CAPTURE_POLLS: (u32, u32) = (5, 200);
pub const CAPTURE_STEP_MS: (u64, u64) = (1, 100);
pub const PASTE_SETTLE_MS: (u64, u64) = (0, 1000);
/// Gama do teto do select-all. O minimo e generoso de proposito: um teto pequeno de mais
/// rejeitaria prompts longos legitimos, que e o caso que este fallback existe para servir.
pub const SELECT_ALL_MAX_CHARS: (usize, usize) = (500, 100_000);

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            gemini_model: DEFAULT_GEMINI_MODEL.to_string(),
            openai_model: DEFAULT_OPENAI_MODEL.to_string(),
            gemini_model_auto: true,
            openai_base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
            openai_auth: OpenAiAuth::ApiKey,
            primary_provider: Provider::Gemini,
            hotkey: "CmdOrCtrl+Shift+Space".to_string(),
            hotkey_polish: String::new(),
            hotkey_turbo: String::new(),
            hotkey_picker: String::new(),
            autostart: false,
            mode: RefineMode::Adaptive,
            thinking_enabled: true,
            thinking_level: "high".to_string(),
            profile_override: None,
            profile_sources: Vec::new(),
            ignore_claude_md: true,
            terminal_handling: true,
            capture_polls: 30,
            capture_step_ms: 10,
            paste_settle_ms: 90,
            save_prompts: false,
            keep_results: false,
            debug_mode: false,
            project_context: false,
            preview_before_paste: false,
            theme: "cream".to_string(),
            select_all_fallback: true,
            select_all_max_chars: 8_000,
            projects: Vec::new(),
            active_project: None,
        }
    }
}

/// Modelos que sabemos pertencer a cada endpoint do provider de fallback. Serve para detetar um
/// modelo que ficou COLADO AO ENDPOINT ERRADO, que e o que acontece quando o utilizador (ou uma
/// migracao nossa) troca de servico: um id do OpenRouter mandado ao Groq da 404.
///
/// Isto e agora so o PALPITE DE ARRANQUE A FRIO, antes de qualquer descoberta. A autoridade
/// sobre que modelos existem passou para `models_cache`: a listagem que o provider publica, lida
/// do mesmo `GET /models` que ja validava a chave, e reconciliada em `ember_core::models`. Por
/// isso a lista de modelos MORTOS que vivia aqui foi apagada: um modelo descontinuado (foi o caso
/// do `deepseek-r1:free`, que era o nosso default e dava erro em todos os refines de quem
/// instalava) desaparece sozinho da listagem do provider, sem ninguem o ter de vir apagar do
/// nosso codigo. As listas por endpoint ficam porque resolvem outro problema, o de um id colado
/// ao endpoint errado, e o pior que fazem ao envelhecer e nao reconhecer um modelo novo (caso 3
/// abaixo: nao se toca).
const OPENROUTER_MODELS: [&str; 3] = [
    "meta-llama/llama-3.3-70b-instruct:free",
    "google/gemma-4-31b-it:free",
    "qwen/qwen3-next-80b-a3b-instruct:free",
];
const GROQ_MODELS: [&str; 3] = [
    "llama-3.3-70b-versatile",
    "llama-3.1-8b-instant",
    "openai/gpt-oss-120b",
];
const OPENAI_MODELS: [&str; 3] = ["gpt-4o-mini", "gpt-4.1-mini", "gpt-5-nano"];
const ANTHROPIC_MODELS: [&str; 2] = ["claude-haiku-4-5", "claude-sonnet-4-6"];

/// O modelo a usar, dado o que esta gravado e o endpoint atual.
///
/// Tres casos, e o terceiro e o que nos mordeu: (1) vazio -> default do endpoint;
/// (2) modelo que sabemos ser de OUTRO endpoint -> default do endpoint atual (senao ficava um id
/// do OpenRouter apontado ao Groq, que da 404 e aparece como "Custom..." na UI);
/// (3) qualquer outro -> NAO se toca (e um modelo que o utilizador escreveu a mao, e a escolha
/// dele manda).
fn migrate_openai_model(model: &str, base_url: &str, default_model: &str) -> String {
    let is_openrouter = base_url.contains("openrouter.ai");
    let is_groq = base_url.contains("api.groq.com");
    let is_openai = base_url.contains("api.openai.com");
    let is_anthropic = base_url.contains("api.anthropic.com");

    // Default do endpoint ATUAL, e nao o default global.
    //
    // Isto estava errado e via-se na UI: so o OpenRouter tinha default proprio, e todos os
    // outros caiam no default global, que e um modelo do Groq. Trocar o servico para OpenAI
    // deixava `llama-3.3-70b-versatile` gravado contra `api.openai.com`, o que da 404 em todos
    // os refines. Trocar um modelo do endpoint errado por outro do endpoint errado nao corrigia
    // nada; so parecia que sim.
    //
    // Um endpoint que nao conhecemos (DeepSeek, Ollama) continua a ficar com o default global,
    // porque ai nao temos mesmo nada melhor para oferecer.
    let endpoint_default = if is_openrouter {
        OPENROUTER_MODELS[0]
    } else if is_openai {
        OPENAI_MODELS[0]
    } else if is_anthropic {
        ANTHROPIC_MODELS[0]
    } else {
        default_model
    };

    if model.is_empty() {
        return endpoint_default.to_string();
    }

    // Pertence a um endpoint que NAO e o atual?
    let belongs_elsewhere = (OPENROUTER_MODELS.contains(&model) && !is_openrouter)
        || (GROQ_MODELS.contains(&model) && !is_groq)
        || (OPENAI_MODELS.contains(&model) && !is_openai)
        || (ANTHROPIC_MODELS.contains(&model) && !is_anthropic);
    if belongs_elsewhere {
        return endpoint_default.to_string();
    }
    model.to_string()
}

impl Config {
    /// Os dois providers pela ordem em que sao tentados. Fonte unica desta ordem: a cadeia do
    /// refine, a pre-validacao e a UI leem daqui, para nao poderem discordar entre si sobre qual
    /// e o primario.
    pub fn provider_order(&self) -> [Provider; 2] {
        match self.primary_provider {
            Provider::Gemini => [Provider::Gemini, Provider::OpenAi],
            Provider::OpenAi => [Provider::OpenAi, Provider::Gemini],
        }
    }

    /// Normaliza valores fora de gama ou vazios (config editada a mao, ou de uma versao
    /// anterior). Campos criticos vazios voltam ao default; o timing e clampado as gamas
    /// aceites pela UI, para um `capture_step_ms: 0` (busy-loop) nunca chegar ao runtime.
    fn sanitize(mut self) -> Self {
        let d = Config::default();
        // So o vazio volta ao default. Aqui esteve uma migracao que reescrevia `gemini-3.5-flash`
        // (um default fantasma de uma versao antiga, quando o id nao existia) e essa migracao
        // passou a fazer o contrario do que devia: a Google lancou mesmo o `gemini-3.5-flash`, e
        // desde entao quem o escolhia na UI via a escolha ser desfeita em silencio no arranque
        // seguinte, sem erro nenhum. Quem decide se um modelo existe e a listagem do provider
        // (`models_cache::reconcile_saved`), nunca um id escrito a mao aqui, que envelhece sozinho.
        if self.gemini_model.trim().is_empty() {
            self.gemini_model = d.gemini_model;
        }
        // Base URL vazia -> default; barra final removida (nao duplicar no caminho do endpoint).
        // Resolvida ANTES do modelo, porque a migracao do modelo depende do endpoint.
        let base = self.openai_base_url.trim().trim_end_matches('/');
        self.openai_base_url = if base.is_empty() {
            d.openai_base_url.clone()
        } else {
            base.to_string()
        };

        // A migracao por endpoint so faz sentido com chave de API. Na subscricao ChatGPT o base
        // URL nao e usado (o backend e outro) e os modelos sao os `gpt-5.x` que so esse backend
        // serve: deixar a migracao correr seria trocar o modelo por um do `api.openai.com` que
        // ali nao existe.
        self.openai_model = match self.openai_auth {
            OpenAiAuth::ChatGpt => {
                let m = self.openai_model.trim();
                // Vazio, ou um id que a OpenAI ja retirou do login ChatGPT: leva o default. O
                // segundo caso e a unica lista de modelos mortos que resta no projeto, e so
                // existe porque este backend nao publica listagem nenhuma de onde a derivar
                // (ver `codex::CODEX_RETIRED_MODELS`). Um id desconhecido nosso NAO se toca: pode
                // ser um modelo novo, ou um que o utilizador escreveu a mao em "Custom...".
                if m.is_empty() || ember_core::codex::CODEX_RETIRED_MODELS.contains(&m) {
                    ember_core::codex::DEFAULT_CODEX_MODEL.to_string()
                } else {
                    m.to_string()
                }
            }
            OpenAiAuth::ApiKey => migrate_openai_model(
                self.openai_model.trim(),
                &self.openai_base_url,
                &d.openai_model,
            ),
        };
        // So o atalho PRINCIPAL volta ao default quando vazio: sem ele a app ficava inutil e em
        // silencio. Os de modo sao opcionais, e vazio ali quer mesmo dizer "nao registes".
        if self.hotkey.trim().is_empty() {
            self.hotkey = d.hotkey;
        }
        self.hotkey_polish = self.hotkey_polish.trim().to_string();
        self.hotkey_turbo = self.hotkey_turbo.trim().to_string();
        self.hotkey_picker = self.hotkey_picker.trim().to_string();
        // Um atalho de picker gravado antes desta regra existir (o `Shift+Up` da primeira
        // utilizacao) fica limpo no load. Deixa-lo em disco era manter uma combinacao que abre a
        // lista e a fecha a seguir, e o utilizador nao tinha como perceber porque.
        if let Some(key) = ember_core::hotkey::picker_key_clash(&self.hotkey_picker) {
            log::warn!(
                "config: atalho do picker descartado ({}): usa {key}, que a lista precisa para navegar",
                self.hotkey_picker
            );
            self.hotkey_picker.clear();
        }
        if self.thinking_level.trim().is_empty() {
            self.thinking_level = d.thinking_level;
        }
        if self.theme != "dark" && self.theme != "cream" {
            self.theme = d.theme;
        }
        // Projetos primeiro: o `active_project` so pode ser validado contra a lista ja limpa,
        // senao um id de um projeto que o sanitize acabou de descartar sobrevivia a apontar para
        // nada, e o refine ficava a procurar um brief que nunca mais existe.
        self.projects = ember_core::projects::sanitize_projects(std::mem::take(&mut self.projects));
        if let Some(id) = self.active_project.as_deref() {
            if !self.projects.iter().any(|p| p.id == id) {
                self.active_project = None;
            }
        }
        self.capture_polls = self.capture_polls.clamp(CAPTURE_POLLS.0, CAPTURE_POLLS.1);
        self.capture_step_ms = self
            .capture_step_ms
            .clamp(CAPTURE_STEP_MS.0, CAPTURE_STEP_MS.1);
        self.paste_settle_ms = self
            .paste_settle_ms
            .clamp(PASTE_SETTLE_MS.0, PASTE_SETTLE_MS.1);
        self.select_all_max_chars = self
            .select_all_max_chars
            .clamp(SELECT_ALL_MAX_CHARS.0, SELECT_ALL_MAX_CHARS.1);
        self
    }
}

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("config.json"))
}

/// Carrega a config do disco; devolve defaults se nao existir ou estiver corrompida.
static CONFIG_WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn load(app: &AppHandle) -> Config {
    let _writer = CONFIG_WRITER.lock().unwrap_or_else(|e| e.into_inner());
    load_unlocked(app)
}

fn load_unlocked(app: &AppHandle) -> Config {
    config_path(app)
        .and_then(|p| read_at(&p).ok())
        .unwrap_or_default()
}

fn read_at(path: &std::path::Path) -> std::io::Result<Config> {
    use std::io::Read;
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(std::io::Error::other(
            "Configuration exceeds the size limit",
        ));
    }
    match serde_json::from_slice::<Config>(&bytes) {
        Ok(mut cfg) => {
            if cfg.schema_version > 1 {
                return Err(std::io::Error::other(
                    "Configuration belongs to a newer Ember version",
                ));
            }
            if cfg.schema_version == 0 {
                // The old default was on, so its boolean does not establish consent to the
                // new retention policy. Preserve the original before migrating to memory only.
                let backup = path.with_extension(format!("json.v0-{}.bak", crate::now_ms()));
                use std::io::Write;
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(backup)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                cfg.keep_results = false;
                cfg.schema_version = 1;
                cfg.revision = cfg
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| std::io::Error::other("Revision exhausted"))?;
                crate::atomic_file::write(path, &serde_json::to_vec_pretty(&cfg)?)?;
            }
            Ok(cfg.sanitize())
        }
        Err(_) => {
            // Copy into a unique backup before removing the corrupt original. Never overwrite
            // an earlier recovery file, and never authorize a save if backup fails.
            let backup = path.with_extension(format!(
                "json.corrupt-{}-{}.bak",
                crate::now_ms(),
                std::process::id()
            ));
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::remove_file(path)?;
            log::warn!("config: corrupt configuration preserved in a recovery file");
            Ok(Config::default())
        }
    }
}

/// Grava a config no disco (cria o diretorio se preciso).
pub fn save(app: &AppHandle, cfg: &Config) -> std::io::Result<()> {
    let path = config_path(app)
        .ok_or_else(|| std::io::Error::other("Configuration directory unavailable"))?;
    save_at(&path, cfg)
}

fn save_at(path: &std::path::Path, cfg: &Config) -> std::io::Result<()> {
    let _writer = CONFIG_WRITER.lock().unwrap_or_else(|e| e.into_inner());
    let current = read_at(path)?;
    if current.revision != cfg.revision {
        return Err(std::io::Error::other(
            "Settings changed concurrently. Reload and retry.",
        ));
    }
    let mut cfg = cfg.clone().sanitize();
    cfg.revision = current
        .revision
        .checked_add(1)
        .ok_or_else(|| std::io::Error::other("Revision exhausted"))?;
    let bytes = serde_json::to_vec_pretty(&cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    crate::atomic_file::write(path, &bytes)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::field_reassign_with_default,
        reason = "Tests deliberately mutate one setting at a time"
    )]
    use super::*;

    fn test_folder() -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let folder =
            std::env::temp_dir().join(format!("ember-config-test-{}-{suffix}", std::process::id()));
        fs::create_dir(&folder).unwrap();
        folder
    }

    #[test]
    fn legacy_default_retention_requires_fresh_consent_and_preserves_original() {
        let folder = test_folder();
        let path = folder.join("config.json");
        let original = br#"{"keep_results":true,"theme":"cream"}"#;
        fs::write(&path, original).unwrap();
        let migrated = read_at(&path).unwrap();
        assert_eq!(migrated.schema_version, 1);
        assert!(!migrated.keep_results);
        assert_eq!(migrated.theme, "cream");
        assert_eq!(read_at(&path).unwrap(), migrated);
        let backup = fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "bak"))
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), original);
        let mut consented = migrated;
        consented.keep_results = true;
        save_at(&path, &consented).unwrap();
        assert!(read_at(&path).unwrap().keep_results);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn stale_writer_cannot_erase_another_setting() {
        let folder = test_folder();
        let path = folder.join("config.json");
        let first = Config {
            theme: "dark".into(),
            ..Config::default()
        };
        let stale = Config {
            keep_results: true,
            ..Config::default()
        };
        save_at(&path, &first).unwrap();
        assert!(save_at(&path, &stale).is_err());
        let current = read_at(&path).unwrap();
        assert_eq!(current.theme, "dark");
        assert!(!current.keep_results);
        assert_eq!(current.revision, 1);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn corrupt_configuration_is_preserved_before_recovery() {
        let folder = test_folder();
        let path = folder.join("config.json");
        fs::write(&path, "{broken configuration").unwrap();
        let recovered = read_at(&path).unwrap();
        assert_eq!(recovered.revision, 0);
        let backup = fs::read_dir(&folder)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(fs::read_to_string(backup).unwrap(), "{broken configuration");
        save_at(&path, &recovered).unwrap();
        assert_eq!(read_at(&path).unwrap().revision, 1);
        fs::remove_dir_all(folder).unwrap();
    }

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
    fn model_from_the_wrong_endpoint_is_swapped_for_one_that_exists_there() {
        // Regressao real (vista na UI): o default do fallback passou para o Groq, mas o modelo
        // gravado continuou a ser um id do OpenRouter (`gemma:free`). Um id do OpenRouter no Groq
        // da 404, e o seletor de modelo mostrava "Custom...", como se o utilizador o tivesse
        // escrito a mao.
        let d = Config::default();
        let mut c = Config::default(); // base URL = Groq (default)
        c.openai_model = "google/gemma-4-31b-it:free".into();
        assert_eq!(c.sanitize().openai_model, d.openai_model);

        // E ao contrario: um modelo do Groq com a base URL no OpenRouter.
        let mut r = Config::default();
        r.openai_base_url = "https://openrouter.ai/api/v1".into();
        r.openai_model = "llama-3.3-70b-versatile".into();
        assert_eq!(
            r.sanitize().openai_model,
            "meta-llama/llama-3.3-70b-instruct:free"
        );

        // Um modelo escrito A MAO (que nao conhecemos) fica INTACTO em qualquer endpoint: a
        // escolha do utilizador manda, e adivinhar por ele seria pior do que nao fazer nada.
        let mut mine = Config::default();
        mine.openai_base_url = "https://api.deepseek.com/v1".into();
        mine.openai_model = "deepseek-chat".into();
        assert_eq!(mine.sanitize().openai_model, "deepseek-chat");
    }

    #[test]
    fn a_config_written_by_an_older_build_keeps_the_new_defaults() {
        // Uma config gravada antes destes campos existirem nao os tem. O `#[serde(default)]` no
        // struct manda preencher a partir de `Config::default()`, nao do default do TIPO, que
        // para um bool seria `false` e deixava o fallback de select-all desligado em toda a gente
        // que ja tinha a app instalada, sem ninguem o ter escolhido. Este teste e a prova disso,
        // porque a diferenca entre as duas leituras do `serde(default)` nao se ve no codigo.
        let antigo = r#"{
            "gemini_model": "gemini-2.5-flash",
            "claude_model": "claude-haiku-4-5",
            "openai_model": "llama-3.3-70b-versatile",
            "openai_base_url": "https://api.groq.com/openai/v1",
            "hotkey": "CmdOrCtrl+Shift+Space",
            "autostart": false,
            "mode": "adaptive"
        }"#;
        let c: Config = serde_json::from_str(antigo).expect("config antiga tem de desserializar");
        assert!(c.select_all_fallback, "campo em falta tem de vir a ON");
        assert_eq!(c.select_all_max_chars, 8_000);
        assert!(c.hotkey_polish.is_empty());
        assert_eq!(c.theme, "cream");
        // E o que ESTAVA gravado continua a ganhar ao default.
        assert_eq!(c.gemini_model, "gemini-2.5-flash");
    }

    #[test]
    fn per_mode_hotkeys_ship_off_and_only_the_main_one_is_restored_when_blank() {
        // O default vazio nao e distraccao: `CmdOrCtrl+Alt+Space` (o candidato obvio) ja estava
        // ocupado na primeira maquina onde isto correu, e o registo dos atalhos e tudo-ou-nada.
        // Um default que colide da um arranque com aviso numa instalacao limpa, por um atalho
        // que ninguem pediu. Se alguem os voltar a preencher, este teste explica porque nao.
        let d = Config::default();
        assert!(d.hotkey_polish.is_empty());
        assert!(d.hotkey_turbo.is_empty());
        assert!(!d.hotkey.is_empty());

        // Vazio num atalho de modo quer mesmo dizer "nao registes" e sobrevive ao sanitize; o
        // principal, esse, volta ao default, porque sem ele a app fica inutil e em silencio.
        let mut c = Config::default();
        c.hotkey = "  ".into();
        c.hotkey_polish = "   ".into();
        c.hotkey_turbo = "CmdOrCtrl+F9".into();
        let c = c.sanitize();
        assert_eq!(c.hotkey, d.hotkey);
        assert!(c.hotkey_polish.is_empty());
        assert_eq!(c.hotkey_turbo, "CmdOrCtrl+F9");
    }

    #[test]
    fn select_all_fallback_is_on_by_default_with_a_sane_ceiling() {
        let d = Config::default();
        assert!(d.select_all_fallback);
        // O teto e clampado como o resto do timing: uma config editada a mao com 0 tornaria a
        // guarda de plausibilidade impossivel de passar e o fallback deixava de funcionar.
        let mut c = Config::default();
        c.select_all_max_chars = 0;
        assert_eq!(c.sanitize().select_all_max_chars, SELECT_ALL_MAX_CHARS.0);
        let mut c = Config::default();
        c.select_all_max_chars = usize::MAX;
        assert_eq!(c.sanitize().select_all_max_chars, SELECT_ALL_MAX_CHARS.1);
    }

    #[test]
    fn switching_service_gives_you_a_model_that_exists_there() {
        // Regressao vista na UI: Service = OpenAI, Base URL = api.openai.com, e o modelo ainda
        // `llama-3.3-70b-versatile`, que e do Groq. Todos os refines dariam 404. A causa era o
        // default do endpoint cair no default GLOBAL (um modelo do Groq) para tudo o que nao
        // fosse OpenRouter, portanto a "correcao" trocava um id errado por outro id errado.
        let d = Config::default();

        let mut openai = Config::default();
        openai.openai_base_url = "https://api.openai.com/v1".into();
        openai.openai_model = "llama-3.3-70b-versatile".into();
        let openai = openai.sanitize();
        assert_eq!(openai.openai_model, "gpt-4o-mini");
        assert_ne!(openai.openai_model, d.openai_model);

        let mut anthropic = Config::default();
        anthropic.openai_base_url = "https://api.anthropic.com/v1".into();
        anthropic.openai_model = "gpt-4o-mini".into();
        assert_eq!(anthropic.sanitize().openai_model, "claude-haiku-4-5");

        // E o caminho de volta: um modelo do OpenAI com o Groq configurado leva um do Groq.
        let mut groq = Config::default();
        groq.openai_model = "gpt-4o-mini".into();
        assert_eq!(groq.sanitize().openai_model, d.openai_model);

        // Endpoint desconhecido: nao ha lista nossa, e o modelo escrito a mao fica intacto.
        let mut ollama = Config::default();
        ollama.openai_base_url = "http://localhost:11434/v1".into();
        ollama.openai_model = "qwen2.5:7b".into();
        assert_eq!(ollama.sanitize().openai_model, "qwen2.5:7b");
    }

    #[test]
    fn sanitize_no_longer_guesses_which_models_are_dead() {
        // A lista de modelos mortos saiu daqui: quem decide se um modelo ainda existe e o
        // proprio provider, pela listagem que `models_cache` absorve no probe de arranque
        // (`ember_core::models::reconcile`, testado la com este mesmo `deepseek-r1:free`).
        //
        // Este teste pina a HANDOVER, nao a ausencia de protecao: o sanitize deixa passar um id
        // que nao reconhece, e e a descoberta que o corrige com um facto em vez de um palpite.
        // Sem isto, alguem que visse este id a sobreviver ao sanitize podia julgar que a
        // regressao do `deepseek-r1:free` tinha ficado sem guarda nenhuma.
        let mut c = Config::default();
        c.openai_model = "deepseek/deepseek-r1:free".into();
        assert_eq!(c.sanitize().openai_model, "deepseek/deepseek-r1:free");

        // O que o sanitize AINDA garante e outra coisa: um id colado ao endpoint errado (aqui um
        // id do OpenRouter com o Groq configurado) leva o default do endpoint atual, porque isso
        // e um erro de configuracao e nao uma questao de o modelo existir ou nao.
        let d = Config::default();
        let mut wrong = Config::default();
        wrong.openai_model = "meta-llama/llama-3.3-70b-instruct:free".into();
        assert_eq!(wrong.sanitize().openai_model, d.openai_model);

        // E um modelo escolhido a mao pelo utilizador continua intacto.
        let mut mine = Config::default();
        mine.openai_model = "mistralai/mistral-small:free".into();
        assert_eq!(mine.sanitize().openai_model, "mistralai/mistral-small:free");
    }

    #[test]
    fn sanitize_no_longer_undoes_a_gemini_model_the_user_picked() {
        // Regressao real, com rasto no log do utilizador: havia aqui uma migracao que reescrevia
        // `gemini-3.5-flash` para o default, porque esse id tinha sido um default fantasma antes
        // de existir. Entretanto a Google lancou-o a serio. A migracao ficou a apagar uma escolha
        // legitima a cada arranque, sem erro e sem aviso: o utilizador escolhia o modelo, a UI
        // confirmava, e no relancamento seguinte estava outro la.
        let mut c = Config::default();
        c.gemini_model = "gemini-3.5-flash".into();
        assert_eq!(c.sanitize().gemini_model, "gemini-3.5-flash");

        // Um id que nem sequer conhecemos tambem passa: quem o corrige e a listagem do provider
        // (models_cache), com um facto, e nao um palpite escrito a mao aqui.
        let mut novo = Config::default();
        novo.gemini_model = "gemini-4.0-flash".into();
        assert_eq!(novo.sanitize().gemini_model, "gemini-4.0-flash");

        // Vazio continua a voltar ao default: sem modelo nenhum nao havia pedido para fazer.
        let mut vazio = Config::default();
        vazio.gemini_model = "   ".into();
        assert_eq!(
            vazio.sanitize().gemini_model,
            Config::default().gemini_model
        );
    }

    #[test]
    fn subscription_mode_keeps_its_own_models_and_is_off_by_default() {
        // O default nao muda para quem ja usa a app: chave de API, como sempre.
        assert_eq!(Config::default().openai_auth, OpenAiAuth::ApiKey);

        // Em modo subscricao o base URL nao e usado (o backend e outro) e os `gpt-5.x` so
        // existem la. Deixar a migracao por endpoint correr trocava-os por um modelo do
        // `api.openai.com`, que naquele backend da erro.
        let mut c = Config::default();
        c.openai_auth = OpenAiAuth::ChatGpt;
        c.openai_model = "gpt-5.6-terra".into();
        c.openai_base_url = "https://api.openai.com/v1".into();
        assert_eq!(c.sanitize().openai_model, "gpt-5.6-terra");

        // Um modelo que a OpenAI JA retirou do login ChatGPT leva o default. `gpt-5.2` foi o
        // default com que este modo nasceu, portanto quem fez login nesses dias tem-no gravado e
        // apanharia erro em todos os refines sem perceber que a culpa era do modelo.
        let mut morto = Config::default();
        morto.openai_auth = OpenAiAuth::ChatGpt;
        morto.openai_model = "gpt-5.2".into();
        assert_eq!(
            morto.sanitize().openai_model,
            ember_core::codex::DEFAULT_CODEX_MODEL
        );

        // Um id que nao conhecemos NAO se toca: pode ser um modelo novo, ou um escrito a mao em
        // "Custom...". A lista de retirados e curta e explicita, nunca um "tudo o que nao conheco".
        let mut custom = Config::default();
        custom.openai_auth = OpenAiAuth::ChatGpt;
        custom.openai_model = "gpt-9.9-experimental".into();
        assert_eq!(custom.sanitize().openai_model, "gpt-9.9-experimental");

        // Vazio leva o default do proprio backend, e nao o do slot de chave.
        let mut vazio = Config::default();
        vazio.openai_auth = OpenAiAuth::ChatGpt;
        vazio.openai_model = "  ".into();
        assert_eq!(
            vazio.sanitize().openai_model,
            ember_core::codex::DEFAULT_CODEX_MODEL
        );
    }

    #[test]
    fn the_provider_order_follows_the_choice_and_defaults_to_the_free_one() {
        // Default: Gemini primeiro, porque e gratuito e e a escolha certa para quem nunca pensou
        // no assunto. Uma config antiga nao tem o campo e cai aqui.
        let d = Config::default();
        assert_eq!(d.primary_provider, Provider::Gemini);
        assert_eq!(d.provider_order(), [Provider::Gemini, Provider::OpenAi]);

        let mut c = Config::default();
        c.primary_provider = Provider::OpenAi;
        assert_eq!(c.provider_order(), [Provider::OpenAi, Provider::Gemini]);

        let antigo = r#"{ "gemini_model": "gemini-2.5-flash" }"#;
        let velho: Config = serde_json::from_str(antigo).expect("desserializa");
        assert_eq!(velho.primary_provider, Provider::Gemini);
    }

    #[test]
    fn a_config_from_before_subscription_mode_existed_still_uses_api_keys() {
        // O campo em falta tem de cair em `ApiKey`. Se caisse em `ChatGpt`, toda a gente que ja
        // tem a app instalada acordava com o fallback a apontar para uma sessao que nunca fez.
        let antigo = r#"{ "gemini_model": "gemini-2.5-flash", "openai_model": "gpt-4o-mini" }"#;
        let c: Config = serde_json::from_str(antigo).expect("config antiga desserializa");
        assert_eq!(c.openai_auth, OpenAiAuth::ApiKey);
        // E o id IPC e estavel: a UI compara com estas strings.
        assert_eq!(
            serde_json::to_string(&OpenAiAuth::ChatGpt).unwrap(),
            "\"chat_gpt\""
        );
        assert_eq!(
            serde_json::to_string(&OpenAiAuth::ApiKey).unwrap(),
            "\"api_key\""
        );
    }

    #[test]
    fn a_config_from_before_projects_existed_loads_with_none() {
        let antigo = r#"{ "gemini_model": "gemini-2.5-flash", "hotkey": "CmdOrCtrl+Shift+E" }"#;
        let c: Config = serde_json::from_str(antigo).expect("config antiga desserializa");
        assert!(c.projects.is_empty());
        assert_eq!(c.active_project, None);
    }

    #[test]
    fn an_active_project_that_names_nothing_is_cleared_on_load() {
        // Sem isto, apagar um projeto deixava o id ativo a apontar para nada e o refine ficava a
        // procurar um brief que nunca mais existe, sem ninguem perceber porque.
        let mut c = Config::default();
        c.active_project = Some("fantasma".into());
        assert_eq!(c.clone().sanitize().active_project, None);

        // E com o projeto la, mantem-se.
        c.projects = vec![ember_core::projects::Project {
            id: "fantasma".into(),
            name: "Existe".into(),
            accent: 0,
            icon: "sparkle".into(),
            brief: "Escreve curto.".into(),
            folder: None,
            source_path: None,
            source_fingerprint: None,
            accent_custom: None,
        }];
        assert_eq!(c.sanitize().active_project.as_deref(), Some("fantasma"));
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
        // Modelo vazio NO OPENROUTER -> um modelo do OpenRouter. Dar-lhe o default (que e um id
        // do Groq) seria "corrigir" para um modelo inexistente naquele endpoint.
        assert_eq!(c.openai_model, "meta-llama/llama-3.3-70b-instruct:free");
        // barra final removida
        assert_eq!(c.openai_base_url, "https://openrouter.ai/api/v1");

        // Modelo vazio no endpoint por defeito (Groq) -> o modelo default.
        let mut g = Config::default();
        g.openai_model = "  ".into();
        assert_eq!(g.sanitize().openai_model, d.openai_model);

        // base URL totalmente vazia -> default
        let mut c2 = Config::default();
        c2.openai_base_url = "   ".into();
        assert_eq!(c2.sanitize().openai_base_url, d.openai_base_url);
    }
}
