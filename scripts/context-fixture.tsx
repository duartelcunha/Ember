import React from "react";
import { createRoot } from "react-dom/client";
import { mockIPC } from "@tauri-apps/api/mocks";
import { ContextInspector } from "../src/settings/ContextInspector";
import "../src/styles/globals.css";
const fixture = { pending: [] as ((value: unknown) => void)[] };
(window as unknown as { __contextFixture: typeof fixture }).__contextFixture = fixture;
mockIPC((command) => command === "get_context_snapshot" ? new Promise(resolve => fixture.pending.push(resolve)) : null);
createRoot(document.getElementById("root")!).render(<div style={{ width: 480, maxWidth: "100%", padding: 16 }}><ContextInspector /></div>);
