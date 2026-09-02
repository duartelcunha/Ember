//! Estado partilhado da app (managed state do Tauri).

use ember_core::health::KeyCheck;
use ember_core::model::Provider;
use ember_core::models::ModelInfo;
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
    /// Listagem de modelos que cada provider publicou, ja ordenada, com o timestamp ms da
    /// descoberta. Vem do MESMO pedido que preenche o `key_checks` (ver `providers::Probe`),
    /// por isso nao custa nem um pedido extra. Vazio = ainda nao houve descoberta; a UI serve
    /// a lista embutida e diz que nao e viva, em vez de fingir frescura.
    pub model_lists: Mutex<HashMap<Provider, (Vec<ModelInfo>, u64)>>,
    /// `true` quando o utilizador pediu para sair (tray -> Quit). O handler de
    /// `ExitRequested` so impede a saida quando isto e `false` (fechar janelas != sair).
    pub quitting: AtomicBool,
    /// `true` quando o overlay mostra o orb (fase "refining"), `false` quando mostra a
    /// pilula (success/error/hint). O orb e muito mais pequeno do que a janela fixa que
    /// o contem, por isso o seguimento do cursor precisa de saber qual conteudo clampar.
    pub orb_visible: AtomicBool,
    /// O overlay deve andar atras do cursor? Verdade enquanto ha trabalho a decorrer (o orb) e
    /// enquanto o preview espera resposta, e falso nas pilulas de resultado.
    ///
    /// E uma flag separada do `orb_visible` porque as duas perguntas sao diferentes: aquela diz
    /// COMO clampar (a caixa pequena do orb ou a janela toda), esta diz SE seguir. Enquanto foram
    /// a mesma, a pilula do preview ficava colada ao sitio onde o cursor estava quando o refine
    /// acabou, e quem entretanto mexeu o rato ficava com uma pergunta esquecida a meio do ecra.
    pub follow_cursor: AtomicBool,
    /// `true` enquanto um ciclo de refinamento decorre (do hotkey ate esconder o orb).
    /// Uma segunda tecla enquanto isto e `true` cancela o ciclo em curso (ver `cancel`).
    pub busy: AtomicBool,
    /// Pedido de cancelamento do ciclo em curso. Posto a `true` pela segunda tecla; o fluxo
    /// verifica-o entre fases e no `select!` do refine. Reposto a `false` no arranque do ciclo.
    pub cancel: AtomicBool,
    /// Acorda o `select!` do refine quando um cancelamento e pedido a meio da chamada HTTP.
    pub cancel_notify: Notify,
    /// O access token da sessao ChatGPT, em memoria, e o mutex que serializa a sua renovacao.
    ///
    /// Duas coisas no mesmo sitio porque sao a mesma seccao critica:
    /// - **serializar**: a OpenAI RODA o refresh token a cada renovacao, portanto duas renovacoes
    ///   ao mesmo tempo (um refine e um probe das settings) faziam a segunda gravar um token que a
    ///   primeira ja tinha invalidado, e a sessao morria sozinha;
    /// - **em memoria**: o access token nao cabe numa credencial do Credential Manager do Windows
    ///   (medido, nao teorico). Sem cache, cada refine comecava por uma renovacao, ou seja um
    ///   pedido extra e uma rotacao de token a mais, so para chegar ao mesmo sitio. Guarda-lo aqui
    ///   e alias o mais correto: expira em horas e nao faz falta nenhuma sobreviver ao fecho da app.
    pub oauth_access: tokio::sync::Mutex<Option<CachedAccess>>,
    /// Os tres tons do projeto ativo, para o orb tomar a cor dele. `None` = sem projeto, e o orb
    /// fica como sempre foi.
    ///
    /// Vive aqui e nao e lido da config a cada emissao de propósito: o `flow::emit` corre varias
    /// vezes por refine (uma por tentativa) e ler o ficheiro de config nesse caminho era pagar
    /// disco por uma coisa que so muda quando o utilizador troca de projeto.
    pub orb_accent: Mutex<Option<[String; 3]>>,
    /// O nome do projeto ativo. A cor diz que ha um projeto; o nome diz QUAL. Sem ele, quem tem
    /// varios projetos de cores parecidas fica a adivinhar, e adivinhar era o problema.
    pub orb_project: Mutex<Option<String>>,
    /// O picker de projetos esta aberto? Guarda de reentrancia + sinal para o atalho de refine.
    pub picker_open: AtomicBool,
    /// Quando o picker abriu. Serve para distinguir a SEGUNDA pressao do atalho (que fecha) do
    /// auto-repeat da primeira, que o Windows entrega enquanto a combinacao esta premida e que
    /// fechava a lista no mesmo instante em que ela abria.
    pub picker_opened_at: Mutex<Option<std::time::Instant>>,
    /// Pedido de fecho do picker (segunda pressao do atalho dele, ou um refine a arrancar).
    pub picker_cancel: AtomicBool,
    /// O ultimo payload emitido para o overlay, para o poder re-emitir sem inventar estado.
    ///
    /// Existe para a travessia entre monitores com DPI diferente: ao redimensionar a janela, o
    /// WebView2 pode ficar com a superficie meio pintada, e re-emitir o estado forca o repaint.
    /// Guardar o payload (em vez de reconstruir) garante que a re-emissao nao muda nada do que
    /// esta no ecra: mesma fase, mesma mensagem, mesma cor.
    pub last_state: Mutex<Option<serde_json::Value>>,
}

/// O que uma renovacao devolve e vale a pena guardar ate expirar.
#[derive(Debug, Clone)]
pub struct CachedAccess {
    pub token: String,
    pub account_id: Option<String>,
    pub expires_at_ms: u64,
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
            model_lists: Mutex::new(HashMap::new()),
            quitting: AtomicBool::new(false),
            orb_visible: AtomicBool::new(true),
            follow_cursor: AtomicBool::new(true),
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            cancel_notify: Notify::new(),
            oauth_access: tokio::sync::Mutex::new(None),
            orb_accent: Mutex::new(None),
            orb_project: Mutex::new(None),
            picker_open: AtomicBool::new(false),
            picker_opened_at: Mutex::new(None),
            picker_cancel: AtomicBool::new(false),
            last_state: Mutex::new(None),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
