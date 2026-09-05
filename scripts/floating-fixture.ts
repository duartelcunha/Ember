import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
const testWindow = window as typeof window & { __emit: typeof emit; __pickerReady: boolean };
testWindow.__pickerReady = false;
mockIPC((cmd) => {
  if (cmd === "picker_snapshot") testWindow.__pickerReady = true;
  if (cmd === "floating_position") return { x: -10, y: 540, originX: -640, originY: 0, sequence: 1 };
  if (cmd === "overlay_snapshot") return { sequence: 10, runId: 3, phase: "hint", message: "Snapshot ready" };
  return null;
}, { shouldMockEvents: true });
testWindow.__emit = emit;
if (location.pathname.includes("profile")) void import("./profile-fixture");
else if (location.pathname.includes("picker")) void import("../src/picker/main");
else void import("../src/overlay/main");
