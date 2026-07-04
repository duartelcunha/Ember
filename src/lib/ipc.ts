import { invoke } from "@tauri-apps/api/core";

export type ProviderKind = "gemini" | "claude";
export type ProfileSource = "claude_md" | "user_edited" | "default";
export type RefineMode = "adaptive" | "polish" | "turbo";
export type ThinkingLevel = "minimal" | "low" | "medium" | "high";
/** Resultado do probe de chave: distingue "chave recusada" de "sem rede agora". */
export type KeyCheck = "valid" | "invalid" | "network_error";

/** Estado das definicoes exposto pelo nucleo Rust (sem chaves em claro). */
export interface EmberSettings {
  geminiModel: string;
  claudeModel: string;
  hotkey: string;
  autostart: boolean;
  hasGeminiKey: boolean;
  hasClaudeKey: boolean;
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
}

export const DEFAULT_SETTINGS: EmberSettings = {
  geminiModel: "gemini-3.5-flash",
  claudeModel: "claude-sonnet-4-6",
  hotkey: "CmdOrCtrl+Shift+Space",
  autostart: false,
  hasGeminiKey: false,
  hasClaudeKey: false,
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
};

/** Comandos Tauri das settings. Implementados no nucleo Rust. */
export const ipc = {
  getSettings: () => invoke<EmberSettings>("get_settings"),
  setApiKey: (provider: ProviderKind, key: string) =>
    invoke<void>("set_api_key", { provider, key }),
  clearApiKey: (provider: ProviderKind) => invoke<void>("clear_api_key", { provider }),
  validateKey: (provider: ProviderKind) => invoke<KeyCheck>("validate_key", { provider }),
  setModel: (provider: ProviderKind, model: string) =>
    invoke<void>("set_model", { provider, model }),
  setHotkey: (hotkey: string) => invoke<void>("set_hotkey", { hotkey }),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  setMode: (mode: RefineMode) => invoke<void>("set_mode", { mode }),
  setThinking: (enabled: boolean, level: ThinkingLevel) =>
    invoke<void>("set_thinking", { enabled, level }),
  setTerminalHandling: (enabled: boolean) => invoke<void>("set_terminal_handling", { enabled }),
  setCaptureTiming: (polls: number, stepMs: number, settleMs: number) =>
    invoke<EmberSettings>("set_capture_timing", { polls, stepMs, settleMs }),
  setProfile: (text: string) => invoke<void>("set_profile", { text }),
  reloadProfileFromClaudeMd: () => invoke<EmberSettings>("reload_profile"),
  resetProfileToDefault: () => invoke<EmberSettings>("reset_profile"),
  setDebugMode: (enabled: boolean) => invoke<void>("set_debug_mode", { enabled }),
  readRecentLogs: (lines: number) => invoke<string>("read_recent_logs", { lines }),
  revealLogDir: () => invoke<void>("reveal_log_dir"),
  getDiagnostics: () => invoke<string>("get_diagnostics"),
};
