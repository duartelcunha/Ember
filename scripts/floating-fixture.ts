import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
const testWindow = window as typeof window & { __emit: typeof emit; __pickerReady: boolean; __closed?: string; __trayActions: string[]; __trayAlreadyOpen?: boolean };
testWindow.__pickerReady = false;
testWindow.__trayActions = [];
// The tray page asks on mount whether it missed an open; `?trayOpen` makes the answer yes.
testWindow.__trayAlreadyOpen = new URLSearchParams(location.search).has("trayOpen");
mockIPC((cmd, args) => {
  if (cmd === "close_splash" || cmd === "finalize_quit") testWindow.__closed = cmd;
  if (cmd === "picker_snapshot") testWindow.__pickerReady = true;
  if (cmd === "tray_action") {
    const action = String((args as { action?: string } | undefined)?.action);
    testWindow.__trayActions.push(action);
    return action === "ready" ? testWindow.__trayAlreadyOpen === true : false;
  }
  if (cmd === "plugin:app|version") return "1.2.3";
  if (cmd === "floating_position") return { x: -10, y: 540, originX: -640, originY: 0, sequence: 1 };
  if (cmd === "overlay_snapshot") return { sequence: 10, runId: 3, phase: "hint", message: "Snapshot ready" };
  return null;
}, { shouldMockEvents: true });
testWindow.__emit = emit;
if (location.pathname.includes("splash")) void import("../src/splash/main");
else if (location.pathname.includes("settings")) void import("./settings-fixture");
else if (location.pathname.includes("context")) void import("./context-fixture");
else if (location.pathname.includes("projects")) void import("./projects-fixture");
else if (location.pathname.includes("profile")) void import("./profile-fixture");
else if (location.pathname.includes("picker")) void import("../src/picker/main");
else if (location.pathname.includes("tray")) void import("../src/tray/main");
else void import("../src/overlay/main");
