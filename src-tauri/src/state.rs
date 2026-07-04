//! Estado partilhado da app (managed state do Tauri).

use ember_core::health::KeyCheck;
use ember_core::model::Provider;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::Notify;

pub struct AppState {
    /// Um unico `reqwest::Client` partilhado (pool de conexoes interno).
    pub http: Client,
    /// Cache dos probes de validacao de chave (resultado + timestamp ms). Preenchido no
    /// arranque (pre-validacao dos fallbacks) e quando o utilizador valida/muda uma chave. O
    /// `ember_core::health::assess_providers` le isto para dizer se ha um fallback provado.
    pub key_checks: Mutex<HashMap<Provider, (KeyCheck, u64)>>,
    /// `true` quando o utilizador pediu para sair (tray -> Quit). O handler de
    /// `ExitRequested` so impede a saida quando isto e `false` (fechar janelas != sair).
    pub quitting: AtomicBool,
    /// `true` quando o overlay mostra o orb (fase "refining"), `false` quando mostra a
    /// pilula (success/error/hint). O orb e muito mais pequeno do que a janela fixa que
    /// o contem, por isso o seguimento do cursor precisa de saber qual conteudo clampar.
    pub orb_visible: AtomicBool,
    /// `true` enquanto um ciclo de refinamento decorre (do hotkey ate esconder o orb).
    /// Uma segunda tecla enquanto isto e `true` cancela o ciclo em curso (ver `cancel`).
    pub busy: AtomicBool,
    /// Pedido de cancelamento do ciclo em curso. Posto a `true` pela segunda tecla; o fluxo
    /// verifica-o entre fases e no `select!` do refine. Reposto a `false` no arranque do ciclo.
    pub cancel: AtomicBool,
    /// Acorda o `select!` do refine quando um cancelamento e pedido a meio da chamada HTTP.
    pub cancel_notify: Notify,
}

impl AppState {
    pub fn new() -> Self {
        // Sem timeout total: as chamadas de refine sao sempre em streaming e um pedido com
        // thinking pesado pode legitimamente demorar minutos a completar (o audit encontrou
        // um teto de 30s a colidir com pedidos de ate 32768 tokens). Um `connect_timeout`
        // continua a falhar depressa se a rede estiver mesmo inalcancavel; uma ligacao presa
        // A MEIO do stream e detetada pelo watchdog de stall em `providers::call_once`, nao
        // aqui, para nao penalizar streams legitimamente longos.
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            http,
            key_checks: Mutex::new(HashMap::new()),
            quitting: AtomicBool::new(false),
            orb_visible: AtomicBool::new(true),
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            cancel_notify: Notify::new(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
