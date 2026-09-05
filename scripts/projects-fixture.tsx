import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { MotionConfig } from "motion/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { DEFAULT_SETTINGS, type AccentPreview, type Project } from "../src/lib/ipc";
import { ProjectsTab } from "../src/settings/ProjectsTab";
import "../src/styles/globals.css";

const project = (id: string, name: string): Project => ({ id, name, brief: `Brief for ${name}`,
  accent: 0, accentCustom: "#fd8c3c", icon: "sparkle", folder: `/fixture/${name}`, sourcePath: null });
let stored = { ...DEFAULT_SETTINGS, projects: [project("a", "Alpha"), project("b", "Beta")],
  accents: [{ label: "Ember", raw: "#aa4411", mid: "#fd8c3c", glow: "#ffcc88" }], icons: ["sparkle"] };
const fixture = { wheel: [] as ((value: AccentPreview) => void)[], saved: [] as Project[], distillations: 0,
  resolveDistillation: (_text: string) => {} };
(window as unknown as { __projectsFixture: typeof fixture }).__projectsFixture = fixture;
mockIPC((command, args) => {
  if (command === "preview_accent") return { raw: "#aa4411", mid: args.hex, glow: "#ffcc88", chroma: 0.1, hue: 30 };
  if (command === "accent_from_wheel") return new Promise<AccentPreview>(resolve => fixture.wheel.push(resolve));
  if (command === "scan_project_folder") return { sourceFingerprint: "a".repeat(64), sourcePaths: ["/fixture/AGENTS.md"],
    sourcePath: "/fixture/AGENTS.md", warnings: [], fileName: "AGENTS.md", lines: 3, candidates: [], subfolders: [] };
  if (command === "distill_project") { fixture.distillations++; return new Promise<string>(resolve => { fixture.resolveDistillation = resolve; }); }
  if (command === "save_project") {
    const next = args.project as Project;
    fixture.saved.push(next);
    stored = { ...stored, projects: stored.projects.map(p => p.id === next.id ? next : p) };
    return stored;
  }
  return null;
});
function Fixture() {
  const [settings, setSettings] = useState(stored);
  return <MotionConfig reducedMotion="user"><ProjectsTab s={settings} setS={setSettings} /></MotionConfig>;
}
createRoot(document.getElementById("root")!).render(<React.StrictMode><Fixture /></React.StrictMode>);
