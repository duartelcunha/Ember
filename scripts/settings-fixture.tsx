import React from "react";
import { createRoot } from "react-dom/client";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { DEFAULT_SETTINGS } from "../src/lib/ipc";
import { Settings } from "../src/settings/Settings";
import "@fontsource-variable/geist";
import "../src/styles/globals.css";

const fixture = { resolveKey: (_value?: unknown) => {}, keyPending: false };
(window as unknown as { __settingsFixture: typeof fixture }).__settingsFixture = fixture;
mockWindows("settings");
mockIPC((command) => {
  if (command === "get_settings") return DEFAULT_SETTINGS;
  if (command === "list_models") return { models: [], live: false, fetchedAtMs: null };
  if (command === "set_api_key") {
    fixture.keyPending = true;
    return new Promise(resolve => { fixture.resolveKey = resolve; });
  }
  if (command === "validate_key") return "invalid";
  if (command === "plugin:app|version") return "1.1.0-test";
  if (command === "plugin:updater|check") throw new Error("Offline fixture");
  return null;
}, { shouldMockEvents: true });
createRoot(document.getElementById("root")!).render(<React.StrictMode><Settings /></React.StrictMode>);
