import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { mockIPC } from "@tauri-apps/api/mocks";
import { DEFAULT_SETTINGS, type ProfileImport } from "../src/lib/ipc";
import { ProfileEditor } from "../src/settings/ProfileEditor";
import "../src/styles/globals.css";

let stored = { ...DEFAULT_SETTINGS, profileText: "Tone: direct", legacyAutoProfileDisabled: true };
const fixture = { saved: [] as unknown[], imports: 0, resolveImport: (_draft: ProfileImport) => {} };
(window as unknown as { __profileFixture: typeof fixture }).__profileFixture = fixture;
mockIPC((command, args) => {
  if (command === "plugin:dialog|open") return ["/fixture/AGENTS.md"];
  if (command === "import_profile_files") {
    fixture.imports++;
    return new Promise<ProfileImport>(resolve => { fixture.resolveImport = resolve; });
  }
  if (command === "set_profile") {
    fixture.saved.push(args);
    stored = { ...stored, profileText: args.text as string, profileSources: args.sources as typeof stored.profileSources,
      profileSource: "user_edited", legacyAutoProfileDisabled: false };
    return null;
  }
  if (command === "get_settings") return stored;
  if (command === "reset_profile") {
    stored = { ...stored, profileText: "Tone: default", profileSource: "default", profileSources: [], legacyAutoProfileDisabled: false };
    return stored;
  }
  return null;
});
function Fixture() {
  const [settings, setSettings] = useState(stored);
  return <ProfileEditor settings={settings} onSaved={setSettings} />;
}
createRoot(document.getElementById("root")!).render(<React.StrictMode><Fixture /></React.StrictMode>);
