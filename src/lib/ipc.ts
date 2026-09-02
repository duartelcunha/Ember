import { invoke } from "@tauri-apps/api/core";

/** Os dois slots. O "openai" e o slot de FALLBACK e fala o protocolo OpenAI: serve o Groq, o
 *  OpenAI, o OpenRouter, a Anthropic, o DeepSeek ou um Ollama local, conforme a Base URL. */
export type ProviderKind = "gemini" | "openai";

/**
 * Como o slot de fallback se autentica. `chat_gpt` faz os refines saírem do plano ChatGPT que a
 * pessoa já paga, por um caminho não oficial que a OpenAI pode cortar sem aviso (ver
 * `ember_core::codex`); `api_key` é o BYOK de sempre e não depende de nada disso.
 */
export type OpenAiAuth = "api_key" | "chat_gpt";

/** Consolas de chave que o Rust sabe abrir (ver `open_key_console`). O provider "openai" e
 *  OpenAI-COMPATIBLE: a consola depende do endpoint escolhido, nao do provider. */
export type KeyConsole = "gemini" | "groq" | "openai" | "openrouter" | "anthropic";
export type ProfileSource = "claude_md" | "user_edited" | "default";
export type RefineMode = "adaptive" | "polish" | "turbo";
export type ThinkingLevel = "minimal" | "low" | "medium" | "high";
export type Theme = "dark" | "cream";
/** Resultado do probe de chave: distingue "chave recusada" de "sem rede agora". */
export type KeyCheck = "valid" | "invalid" | "network_error";

/** Veredicto de saude dos providers (fallback pre-validado?). Espelha ember_core::health. */
export type SystemHealth = "healthy" | "degraded" | "down";
export interface ProviderHealth {
  health: SystemHealth;
  configuredCount: number;
  prevalidatedCount: number;
  hasPrevalidatedFallback: boolean;
  needsRevalidation: ProviderKind[];
}

/** Qual dos tres atalhos. O "main" usa o modo escolhido nas settings; os outros fixam o seu. */
export type HotkeySlot = "main" | "polish" | "turbo" | "picker";

/** Veredicto sobre uma combinacao ANTES de a gravar. Espelha `ember_core::hotkey`. */
export type HotkeyVerdict =
  | { kind: "available" }
  /** Reservada pelo SO, ou ja tomada por outra aplicacao. `owner` diz por quem. */
  | { kind: "reserved_by_os"; owner: string }
  /** Ja atribuida a outro atalho do proprio Ember. */
  | { kind: "used_by_ember"; slot: HotkeySlot }
  /** So modificadores, sem tecla principal. */
  | { kind: "incomplete" }
  /** Uma tecla sozinha que e precisa para escrever. Um atalho global rouba-a ao sistema todo. */
  | { kind: "needs_modifier"; key: string }
  /** So no slot do picker: a tecla principal e uma das que a lista usa para navegar. */
  | { kind: "clashes_with_picker"; key: string };

/** Um modelo que o provider disse servir, ja normalizado pelo Rust (ember_core::models). */
export interface ModelInfo {
  id: string;
  displayName: string;
  generation: number;
  freeTier: boolean;
  preview: boolean;
}

/** A listagem de modelos de um provider, ja ordenada do melhor default para o pior candidato. */
export interface ModelCatalog {
  models: ModelInfo[];
  /** Epoch ms da descoberta, ou `null` se nunca houve uma com sucesso. */
  fetchedAtMs: number | null;
  /** `false` = estamos a servir a lista embutida no binario porque ainda nao houve descoberta
   *  (sem chave, offline, endpoint sem `/models`). A UI diz isso em vez de fingir frescura. */
  live: boolean;
}

/** Um projeto registado. O `brief` e o que entra no prompt; o ficheiro e so a semente. */
export interface Project {
  id: string;
  name: string;
  /** Indice na paleta que vem em `EmberSettings.accents`. */
  accent: number;
  icon: string;
  brief: string;
  folder: string | null;
  sourcePath: string | null;
}

/** O que uma pasta tem, antes de se enviar seja o que for. */
export interface ProjectScan {
  sourcePath: string | null;
  fileName: string | null;
  lines: number;
  /** Todos os candidatos com o peso de cada um: e o que torna a escolha explicável. */
  candidates: { fileName: string; score: number; chosen: boolean }[];
  /** Subpastas que têm convenções, quando a pasta escolhida não tem. Um nível só. */
  subfolders: { name: string; path: string; fileName: string }[];
}

/** Uma cor da paleta: tres tons, porque o orb e um gradiente e nao uma cor chapada. */
export interface Accent {
  raw: string;
  mid: string;
  glow: string;
  label: string;
}

/** Estado das definicoes exposto pelo nucleo Rust (sem chaves em claro). */
export interface EmberSettings {
  geminiModel: string;
  openaiModel: string;
  /** O modelo do Gemini e escolhido pelo Ember (o melhor gratuito) ou fixado a mao? */
  geminiModelAuto: boolean;
  openaiBaseUrl: string;
  /** Como o slot de fallback se autentica: chave de API, ou a subscrição ChatGPT. */
  openaiAuth: OpenAiAuth;
  /** Qual dos dois é tentado primeiro. O outro é o fallback. */
  primaryProvider: ProviderKind;
  /**
   * Há uma sessão ChatGPT gravada. Independente do `openaiAuth`: quem faz login e depois volta a
   * um serviço por chave não perde a sessão.
   */
  chatgptSignedIn: boolean;
  /** A conta ligada, quando o token a diz. `null` não quer dizer que não há sessão. */
  chatgptAccount: string | null;
  hotkey: string;
  /** Atalhos que fixam um modo. String vazia = nao registado. */
  hotkeyPolish: string;
  hotkeyTurbo: string;
  /** Atalho do picker de projetos. String vazia = nao registado. */
  hotkeyPicker: string;
  autostart: boolean;
  hasGeminiKey: boolean;
  hasOpenAiKey: boolean;
  /** `null` em condições normais; mensagem quando o cofre de credenciais está ilegível. */
  keyStoreError: string | null;
  profileText: string;
  profileSource: ProfileSource;
  profilePath: string | null;
  mode: RefineMode;
  thinkingEnabled: boolean;
  thinkingLevel: ThinkingLevel;
  terminalHandling: boolean;
  capturePolls: number;
  captureStepMs: number;
  pasteSettleMs: number;
  debugMode: boolean;
  /** Grava prompt e resposta num ficheiro, para se poder melhorar o prompting com casos reais. */
  savePrompts: boolean;
  keepResults: boolean;
  projectContext: boolean;
  previewBeforePaste: boolean;
  theme: Theme;
  /** Sem seleccao, seleciona o campo em foco e refina-o todo. */
  selectAllFallback: boolean;
  selectAllMaxChars: number;
  projects: Project[];
  /** Id do projeto ativo, ou `null` para nenhum. */
  activeProject: string | null;
  /** Paleta e icones vem do Rust para nao existirem duas verdades. */
  accents: Accent[];
  icons: string[];
}

export const DEFAULT_SETTINGS: EmberSettings = {
  geminiModel: "gemini-2.5-flash",
  geminiModelAuto: true,
  // Espelham os defaults do Rust (ember_core::providers). O fallback e o Groq: free tier com
  // ~14 000 pedidos/dia, contra os ~50/dia dos modelos gratuitos do OpenRouter.
  openaiModel: "llama-3.3-70b-versatile",
  openaiBaseUrl: "https://api.groq.com/openai/v1",
  openaiAuth: "api_key",
  primaryProvider: "gemini",
  chatgptSignedIn: false,
  chatgptAccount: null,
  // Placeholder ate o getSettings chegar. O atalho real e escolhido pelo Rust no primeiro
  // arranque, testando candidatos ate encontrar um que o sistema aceite.
  hotkey: "CmdOrCtrl+Shift+E",
  hotkeyPolish: "",
  hotkeyTurbo: "",
  hotkeyPicker: "",
  autostart: false,
  hasGeminiKey: false,
  hasOpenAiKey: false,
  keyStoreError: null,
  profileText: "",
  profileSource: "default",
  profilePath: null,
  mode: "adaptive",
  thinkingEnabled: true,
  thinkingLevel: "high",
  terminalHandling: true,
  capturePolls: 30,
  captureStepMs: 10,
  pasteSettleMs: 90,
  debugMode: false,
  savePrompts: false,
  keepResults: true,
  projectContext: false,
  previewBeforePaste: false,
  theme: "cream",
  selectAllFallback: true,
  selectAllMaxChars: 8000,
  projects: [],
  activeProject: null,
  accents: [],
  icons: [],
};

/** Comandos Tauri das settings. Implementados no nucleo Rust. */
export const ipc = {
  getSettings: () => invoke<EmberSettings>("get_settings"),
  setApiKey: (provider: ProviderKind, key: string) =>
    invoke<void>("set_api_key", { provider, key }),
  clearApiKey: (provider: ProviderKind) => invoke<void>("clear_api_key", { provider }),
  validateKey: (provider: ProviderKind) => invoke<KeyCheck>("validate_key", { provider }),
  /**
   * Abre o browser e faz login com a conta ChatGPT. Resolve quando a sessão estiver gravada, e
   * rejeita com uma mensagem já legível (login cancelado, portas ocupadas, OpenAI recusou).
   * Também passa o slot de fallback para o modo subscrição: fazer login é escolhê-lo.
   */
  chatgptLogin: () => invoke<EmberSettings>("chatgpt_login"),
  /** Termina a sessão, apaga os tokens do cofre e volta ao modo de chave de API. */
  chatgptLogout: () => invoke<EmberSettings>("chatgpt_logout"),
  /** Troca o modo de autenticação do slot de fallback sem passar pelo login. */
  setOpenAiAuth: (mode: OpenAiAuth) => invoke<EmberSettings>("set_openai_auth", { mode }),
  /**
   * Escolhe qual provider é tentado primeiro; o outro passa a ser o fallback. Não mexe em chaves
   * nem em modelos, só na ordem, por isso é reversível a custo zero.
   */
  setPrimaryProvider: (provider: ProviderKind) =>
    invoke<EmberSettings>("set_primary_provider", { provider }),
  getProviderHealth: () => invoke<ProviderHealth>("get_provider_health"),
  /** Fixa um modelo a mao. No Gemini isto desliga o automatico: a partir daqui a escolha e do
   *  utilizador e a descoberta deixa de lhe mexer. */
  setModel: (provider: ProviderKind, model: string) =>
    invoke<void>("set_model", { provider, model }),
  /** Devolve o modelo do Gemini ao automatico (o melhor gratuito que o provider anunciar). */
  setGeminiModelAuto: (enabled: boolean) =>
    invoke<EmberSettings>("set_gemini_model_auto", { enabled }),
  setOpenAiBaseUrl: (baseUrl: string) =>
    invoke<void>("set_openai_base_url", { baseUrl }),
  setHotkey: (which: HotkeySlot, hotkey: string) =>
    invoke<void>("set_hotkey", { which, hotkey }),
  /** Pergunta se a combinacao pode ser gravada, sem a gravar. Junta a lista de atalhos que o
   *  SO reserva (a unica defesa no macOS, onde o registo passa e o sistema ganha depois) com um
   *  teste de registo real (a unica defesa contra outra app qualquer). */
  checkHotkey: (which: HotkeySlot, hotkey: string) =>
    invoke<HotkeyVerdict>("check_hotkey", { which, hotkey }),
  setSelectAllFallback: (enabled: boolean) =>
    invoke<void>("set_select_all_fallback", { enabled }),
  /** Modelos que o provider diz servir hoje. Ver `ModelCatalog.live` antes de os apresentar
   *  como frescos: sem descoberta, isto e a lista embutida no binario. */
  listModels: (provider: ProviderKind) => invoke<ModelCatalog>("list_models", { provider }),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  setMode: (mode: RefineMode) => invoke<void>("set_mode", { mode }),
  setTheme: (theme: Theme) => invoke<void>("set_theme", { theme }),
  setThinking: (enabled: boolean, level: ThinkingLevel) =>
    invoke<void>("set_thinking", { enabled, level }),
  setTerminalHandling: (enabled: boolean) => invoke<void>("set_terminal_handling", { enabled }),
  setProjectContext: (enabled: boolean) => invoke<void>("set_project_context", { enabled }),
  setSavePrompts: (enabled: boolean) => invoke<void>("set_save_prompts", { enabled }),
  setKeepResults: (enabled: boolean) => invoke<void>("set_keep_results", { enabled }),
  /** Cria (id vazio) ou atualiza um projeto. O Rust devolve o estado ja sanitizado. */
  saveProject: (project: Project) => invoke<EmberSettings>("save_project", { project }),
  deleteProject: (id: string) => invoke<EmberSettings>("delete_project", { id }),
  /** `null` = nenhum projeto ativo (volta a valer a detecao pela janela, se ligada). */
  setActiveProject: (id: string | null) =>
    invoke<EmberSettings>("set_active_project", { id }),
  /** Lê a pasta e diz que ficheiro serviria. Não envia nada para lado nenhum. */
  scanProjectFolder: (path: string) => invoke<ProjectScan>("scan_project_folder", { path }),
  /** Lê o ficheiro escolhido e devolve um brief. Não grava: volta como rascunho para rever. */
  distillProject: (path: string) => invoke<string>("distill_project", { path }),
  setPreviewBeforePaste: (enabled: boolean) =>
    invoke<void>("set_preview_before_paste", { enabled }),
  setCaptureTiming: (polls: number, stepMs: number, settleMs: number) =>
    invoke<EmberSettings>("set_capture_timing", { polls, stepMs, settleMs }),
  setProfile: (text: string) => invoke<void>("set_profile", { text }),
  reloadProfileFromClaudeMd: () => invoke<EmberSettings>("reload_profile"),
  resetProfileToDefault: () => invoke<EmberSettings>("reset_profile"),
  setDebugMode: (enabled: boolean) => invoke<void>("set_debug_mode", { enabled }),
  readRecentLogs: (lines: number) => invoke<string>("read_recent_logs", { lines }),
  revealLogDir: () => invoke<void>("reveal_log_dir"),
  openRepo: () => invoke<void>("open_repo"),
  /** Le um ficheiro de perfil escolhido no seletor nativo. Devolve o texto para a textarea. */
  readProfileFile: (path: string) => invoke<string>("read_profile_file", { path }),
  /** Abre a consola onde se cria a chave. `console` e o NOME de uma consola conhecida, nunca um
   *  URL: os URLs vivem no Rust, para o webview nunca mandar o SO abrir um endereco arbitrario.
   *  O provider de fallback e OpenAI-compatible e serve varios servicos, por isso a consola vem
   *  do endpoint escolhido (groq/openai/openrouter), nao do nome do provider. */
  openKeyConsole: (console: KeyConsole) => invoke<void>("open_key_console", { provider: console }),
  getDiagnostics: () => invoke<string>("get_diagnostics"),
};
