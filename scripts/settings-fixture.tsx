import React from "react";
import { createRoot } from "react-dom/client";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { DEFAULT_SETTINGS } from "../src/lib/ipc";
import { RELEASES } from "../src/lib/release";
import { Settings } from "../src/settings/Settings";
import "@fontsource-variable/geist";
import "../src/styles/globals.css";

// The report `get_diagnostics` returns, shortened but the same shape: version, feedback, machine,
// key state per provider (never key material), settings, hotkeys, log path. The About tab renders
// it, so a `null` here would have the layout assertions measuring an empty state.
const DIAGNOSTICS = [
  "Ember 1.1.0-test",
  "Last feedback: none",
  "OS: windows 10.0.22631 (x86_64)",
  "Elevated: no",
  "Gemini key: set",
  "OpenAI key: missing",
  "ChatGPT session: none",
  "Mode: adaptive",
  "Thinking: off",
  "Debug mode: off",
  "Fallback endpoint: https://api.groq.com/openai/v1",
  "Shortcut: CmdOrCtrl+Shift+E",
  "Log: %LOCALAPPDATA%\\com.deleg8lab.ember\\logs\\Ember.log",
].join("\n");

const fixture = {
  resolveKey: (_value?: unknown) => {},
  keyPending: false,
  /** Every mode the UI committed, so the arrow-key walk through the comparison is checkable. */
  modes: [] as string[],
  diagnosticsReads: 0,
};
(window as unknown as { __settingsFixture: typeof fixture }).__settingsFixture = fixture;
mockWindows("settings");
mockIPC((command, args) => {
  if (command === "get_settings") return DEFAULT_SETTINGS;
  if (command === "list_models")
    return {
      models: [
        { id: "gemini-2.5-flash", displayName: "Gemini 2.5 Flash", generation: 25, freeTier: true, preview: false },
        { id: "gemini-3.1-flash-lite", displayName: "Gemini 3.1 Flash Lite", generation: 31, freeTier: true, preview: false },
        { id: "gemini-3.5-flash", displayName: "Gemini 3.5 Flash", generation: 35, freeTier: false, preview: true },
      ],
      live: true,
      fetchedAtMs: 1757000000000,
    };
  // One provider configured and validated, so the try-order strip renders its real "no
  // pre-validated fallback" verdict rather than an unknown state.
  if (command === "get_provider_health")
    return {
      health: "degraded",
      configuredCount: 1,
      prevalidatedCount: 1,
      hasPrevalidatedFallback: false,
      needsRevalidation: ["openai"],
    };
  if (command === "get_diagnostics") {
    fixture.diagnosticsReads++;
    return DIAGNOSTICS;
  }
  if (command === "set_primary_provider")
    return { ...DEFAULT_SETTINGS, primaryProvider: (args as { provider: string }).provider };
  if (command === "set_mode") {
    fixture.modes.push(String((args as { mode?: unknown }).mode));
    return null;
  }
  if (command === "set_api_key") {
    fixture.keyPending = true;
    return new Promise(resolve => { fixture.resolveKey = resolve; });
  }
  if (command === "validate_key") return "invalid";
  if (command === "plugin:app|version") return RELEASES[0].version;
  if (command === "plugin:updater|check") throw new Error("Offline fixture");
  return null;
}, { shouldMockEvents: true });
createRoot(document.getElementById("root")!).render(<React.StrictMode><Settings /></React.StrictMode>);
