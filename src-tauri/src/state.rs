//! Estado partilhado da app (managed state do Tauri).

use reqwest::Client;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

pub struct AppState {
    /// Um unico `reqwest::Client` partilhado (pool de conexoes interno).
    pub http: Client,
    /// `true` quando o utilizador pediu para sair (tray -> Quit). O handler de
    /// `ExitRequested` so impede a saida quando isto e `false` (fechar janelas != sair).
    pub quitting: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self {
            http,
            quitting: AtomicBool::new(false),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
