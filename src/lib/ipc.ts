import { invoke } from "@tauri-apps/api/core";

export type ProviderKind = "gemini" | "openai" | "claude";

/** Consolas de chave que o Rust sabe abrir (ver `open_key_console`). O provider "openai" e
 *  OpenAI-COMPATIBLE: a consola depende do endpoint escolhido, nao do provider. */
export type KeyConsole = "gemini" | "groq" | "openai" | "openrouter" | "claude";
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

/** Estado das definicoes exposto pelo nucleo Rust (sem chaves em claro). */
export interface EmberSettings {
  geminiModel: string;
  claudeModel: string;
  openaiModel: string;
  openaiBaseUrl: string;
  hotkey: string;
  autostart: boolean;
  hasGeminiKey: boolean;
  hasClaudeKey: boolean;
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
  projectContext: boolean;
  previewBeforePaste: boolean;
  theme: Theme;
}

export const DEFAULT_SETTINGS: EmberSettings = {
  geminiModel: "gemini-2.5-flash",
  claudeModel: "claude-haiku-4-5",
  // Espelham os defaults do Rust (ember_core::providers). O fallback e o Groq: free tier com
  // ~14 000 pedidos/dia, contra os ~50/dia dos modelos gratuitos do OpenRouter.
  openaiModel: "llama-3.3-70b-versatile",
  openaiBaseUrl: "https://api.groq.com/openai/v1",
  hotkey: "CmdOrCtrl+Shift+Space",
  autostart: false,
  hasGeminiKey: false,
  hasClaudeKey: false,
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
  projectContext: false,
  previewBeforePaste: false,
  theme: "cream",
};

/** Comandos Tauri das settings. Implementados no nucleo Rust. */
export const ipc = {
  getSettings: () => invoke<EmberSettings>("get_settings"),
  setApiKey: (provider: ProviderKind, key: string) =>
    invoke<void>("set_api_key", { provider, key }),
  clearApiKey: (provider: ProviderKind) => invoke<void>("clear_api_key", { provider }),
  validateKey: (provider: ProviderKind) => invoke<KeyCheck>("validate_key", { provider }),
  getProviderHealth: () => invoke<ProviderHealth>("get_provider_health"),
  setModel: (provider: ProviderKind, model: string) =>
    invoke<void>("set_model", { provider, model }),
  setOpenAiBaseUrl: (baseUrl: string) =>
    invoke<void>("set_openai_base_url", { baseUrl }),
  setHotkey: (hotkey: string) => invoke<void>("set_hotkey", { hotkey }),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  setMode: (mode: RefineMode) => invoke<void>("set_mode", { mode }),
  setTheme: (theme: Theme) => invoke<void>("set_theme", { theme }),
  setThinking: (enabled: boolean, level: ThinkingLevel) =>
    invoke<void>("set_thinking", { enabled, level }),
  setTerminalHandling: (enabled: boolean) => invoke<void>("set_terminal_handling", { enabled }),
  setProjectContext: (enabled: boolean) => invoke<void>("set_project_context", { enabled }),
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
