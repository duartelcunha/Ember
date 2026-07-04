//! Logging e observabilidade. O objetivo do modo debug: nada volta a falhar em silencio.
//!
//! Em release a consola esta destacada (`windows_subsystem = "windows"`), por isso panics e
//! erros ficariam invisiveis. Aqui montamos um ficheiro rotativo no log dir do SO (sempre
//! ativo) e um panic hook que grava o panic + backtrace nesse ficheiro. O toggle `debug_mode`
//! (config) controla depois o que e visivel ao utilizador (devtools + painel de diagnostico);
//! o ficheiro ja captura Debug do nosso crate quer o modo debug esteja ligado ou nao, para o
//! log da execucao que rebentou nao depender de o utilizador ter ativado algo de antemao.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

/// Nome base do ficheiro de log (fica `ember.log` no log dir da app).
pub const LOG_FILE_STEM: &str = "ember";

/// Constroi o plugin de log. Ficheiro rotativo (5 MB, mantem um antigo) no log dir + stdout
/// em debug. O nosso crate loga a Debug; as dependencias ficam em Info para nao poluir.
pub fn plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let mut builder = tauri_plugin_log::Builder::new()
        .level(log::LevelFilter::Info)
        .level_for("ember_lib", log::LevelFilter::Debug)
        .max_file_size(5_000_000)
        .rotation_strategy(RotationStrategy::KeepOne)
        .timezone_strategy(TimezoneStrategy::UseLocal)
        .target(Target::new(TargetKind::LogDir {
            file_name: Some(LOG_FILE_STEM.to_string()),
        }));
    // Consola so existe em debug; em release e destacada e o alvo Stdout nao teria para onde ir.
    if cfg!(debug_assertions) {
        builder = builder.target(Target::new(TargetKind::Stdout));
    }
    builder.build()
}

/// Instala um panic hook que grava o panic + backtrace no log (e mantem o hook anterior).
/// Corre antes do abort em release (`panic = "abort"`), por isso deixa sempre um rasto.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        log::error!("PANIC: {info}\n{backtrace}");
        previous(info);
    }));
}

/// Caminho do ficheiro de log atual (`<log_dir>/ember.log`).
pub fn log_file_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_log_dir()
        .ok()
        .map(|d| d.join(format!("{LOG_FILE_STEM}.log")))
}

/// Ultimas `max_lines` linhas do log, para o painel de diagnostico. Le o ficheiro inteiro e
/// corta a cauda: o ficheiro esta capado a 5 MB pela rotacao, por isso e barato.
pub fn read_recent(app: &AppHandle, max_lines: usize) -> String {
    let Some(path) = log_file_path(app) else {
        return String::new();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return String::new();
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}
