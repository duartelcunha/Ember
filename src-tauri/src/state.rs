//! Estado partilhado da app (managed state do Tauri).

use ember_core::health::KeyCheck;
use ember_core::model::Provider;
use ember_core::models::ModelInfo;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::Notify;

pub struct AppState {
    /// Um unico `reqwest::Client` partilhado (pool de conexoes interno).
    pub http: Client,
    pub connection_generation: Mutex<u64>,
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
    /// Serializes run ownership and phase transitions, independently of background requests.
    pub execution: Mutex<ember_core::cycle::ExecutionCoordinator>,
    pub event_seq: AtomicU64,
    pub last_feedback: Mutex<Option<String>>,
    pub resolved_context: Mutex<Option<ember_core::context::Snapshot>>,
    pub context_sources: Mutex<std::collections::HashMap<String, crate::context::Cached>>,
    pub retention_generation: AtomicU64,
    pub prompt_generation: AtomicU64,
    /// Acorda quem espera (o `select!` do refine, a espera pela chamada em curso) quando o
    /// utilizador dispensa.
    pub cancel_notify: Notify,
    /// Geracao do ciclo de seguimento do cursor. Cada exibicao da overlay incrementa-a e o
    /// ciclo anterior reforma-se ao ver que mudou: dois refines sobrepostos deixavam dois loops
    /// vivos a disputar a mesma janela a 120fps, cada um com a sua suavizacao.
    pub follow_gen: AtomicU64,
    /// A overlay tem etiquetas a direita da brasa (nome do projeto, legenda de retry)?
    ///
    /// Muda o que e preciso garantir visivel: sem etiquetas, reservar-lhes espaco afastava a
    /// brasa do ponteiro em centenas de pixeis junto a borda direita do ecra.
    pub orb_labels: AtomicBool,
    /// Ciclo dono da pilula que esta no ecra. Um `hide_after` de um ciclo antigo compara com
    /// isto e nao faz nada se ja houver um ciclo novo: senao escondia a orb do ciclo seguinte.
    pub hide_gen: AtomicU64,
    /// Refinados ja pagos, por texto normalizado. E o que garante que uma interrupcao nao custa
    /// dinheiro: o resultado fica aqui e o atalho seguinte sobre o mesmo texto nao paga.
    pub store: Mutex<ember_core::RefineCache>,
    pub persisted_store: Mutex<ember_core::RefineCache>,
    /// Sobe a cada escrita no `store`. Quem espera por uma chamada em curso subscreve ANTES de
    /// consultar a cache e so depois espera, que e o que evita perder o sinal.
    pub store_gen: tokio::sync::watch::Sender<u64>,
    /// As chamadas ao modelo que estao a decorrer. Um ciclo novo com a MESMA chave junta-se a
    /// uma delas em vez de fazer (e pagar) a mesma chamada outra vez.
    ///
    /// Lista e nao um slot unico: dispensar um refine e comecar outro com texto diferente deixa
    /// os dois a decorrer, e com um slot so o segundo apagava o registo do primeiro. O terceiro
    /// atalho sobre o texto do primeiro deixava de o encontrar e pagava-o de novo, que e
    /// exatamente o que isto existe para evitar. Sao no maximo um punhado de entradas.
    pub inflight: Mutex<Vec<InFlight>>,
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
    pub oauth_generation: AtomicU64,
    pub oauth_logged_out: AtomicBool,
    pub oauth_commit: Mutex<()>,
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
    pub picker_state: Mutex<Option<serde_json::Value>>,
    pub floating_positions: Mutex<HashMap<String, crate::floating::Position>>,
}

/// Uma chamada ao modelo a decorrer.
#[derive(Debug, Clone)]
pub struct InFlight {
    pub key: ember_core::CacheKey,
    pub run_id: u64,
}

impl AppState {
    pub fn begin_run(&self) -> Result<u64, u64> {
        let mut execution = self.execution.lock().unwrap_or_else(|e| e.into_inner());
        let id = execution.begin()?;
        self.hide_gen.store(id, Ordering::SeqCst);
        Ok(id)
    }

    pub fn is_busy(&self) -> bool {
        self.execution
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_busy()
    }

    pub fn advance_run(&self, id: u64, phase: ember_core::cycle::RunPhase) -> bool {
        self.execution
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .advance(id, phase)
    }

    pub fn may_apply(&self, id: u64) -> bool {
        self.execution
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_phase(id, ember_core::cycle::RunPhase::Applying)
    }

    pub fn complete_run(&self, id: u64) {
        self.execution
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .complete(id);
    }

    /// Ha uma chamada a decorrer com esta chave?
    pub fn inflight_with(&self, key: &ember_core::CacheKey) -> Option<InFlight> {
        self.inflight
            .lock()
            .ok()?
            .iter()
            .find(|f| f.key == *key)
            .cloned()
    }

    /// Regista uma chamada a decorrer.
    pub fn inflight_add(&self, entry: InFlight) {
        if let Ok(mut list) = self.inflight.lock() {
            list.push(entry);
        }
    }

    /// Tira uma chamada do registo e acorda quem esperava por ela.
    ///
    /// O acordar TEM de acontecer aqui e nao no caminho feliz: se a tarefa morresse sem isto
    /// (um panico, um erro), quem se juntou a ela ficava a espera de um sinal que nunca chegava.
    pub fn inflight_done(&self, run_id: u64) {
        if let Ok(mut list) = self.inflight.lock() {
            list.retain(|f| f.run_id != run_id);
        }
        self.store_gen.send_modify(|v| *v += 1);
    }

    /// Pede para dispensar o ciclo `run_id`. NAO mata a chamada ao modelo: ela segue ate ao fim
    /// e o resultado fica guardado. Antes matava, e o dinheiro ja gasto ia com ela.
    pub fn request_dismiss(&self, run_id: u64) {
        if self
            .execution
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancel(run_id)
        {
            self.cancel_notify.notify_waiters();
        }
    }

    /// O ciclo `run_id` foi dispensado?
    pub fn dismissed(&self, run_id: u64) -> bool {
        self.execution
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancelled(run_id)
    }

    /// Guarda um refinado pago e acorda quem estava a espera dele.
    pub fn remember(&self, key: ember_core::CacheKey, entry: ember_core::CacheEntry, now_ms: u64) {
        if let Ok(mut store) = self.store.lock() {
            store.insert(key, entry, now_ms);
        }
        self.store_gen.send_modify(|v| *v += 1);
    }
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
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(180))
            .build()
            .unwrap_or_default();
        Self {
            http,
            connection_generation: Mutex::new(0),
            key_checks: Mutex::new(HashMap::new()),
            model_lists: Mutex::new(HashMap::new()),
            quitting: AtomicBool::new(false),
            orb_visible: AtomicBool::new(true),
            follow_cursor: AtomicBool::new(true),
            execution: Mutex::new(ember_core::cycle::ExecutionCoordinator::default()),
            event_seq: AtomicU64::new(0),
            last_feedback: Mutex::new(None),
            resolved_context: Mutex::new(None),
            context_sources: Mutex::new(std::collections::HashMap::new()),
            retention_generation: AtomicU64::new(0),
            prompt_generation: AtomicU64::new(0),
            cancel_notify: Notify::new(),
            follow_gen: AtomicU64::new(0),
            orb_labels: AtomicBool::new(false),
            hide_gen: AtomicU64::new(0),
            store: Mutex::new(ember_core::RefineCache::default()),
            persisted_store: Mutex::new(ember_core::RefineCache::default()),
            store_gen: tokio::sync::watch::channel(0).0,
            inflight: Mutex::new(Vec::new()),
            oauth_access: tokio::sync::Mutex::new(None),
            oauth_generation: AtomicU64::new(0),
            oauth_logged_out: AtomicBool::new(false),
            oauth_commit: Mutex::new(()),
            orb_accent: Mutex::new(None),
            orb_project: Mutex::new(None),
            picker_open: AtomicBool::new(false),
            picker_opened_at: Mutex::new(None),
            picker_cancel: AtomicBool::new(false),
            last_state: Mutex::new(None),
            picker_state: Mutex::new(None),
            floating_positions: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
