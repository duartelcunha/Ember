import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { STATE_EVENT, type OverlayState } from "./types";

export function useOverlayState(): OverlayState {
  const [state, setState] = useState<OverlayState>({ phase: "hidden" });
  useEffect(() => {
    let disposed = false;
    const accept = (next: OverlayState | null) => {
      if (!next || disposed) return;
      setState((current) => (next.runId ?? 0) >= (current.runId ?? 0) && (next.sequence ?? 0) > (current.sequence ?? -1) ? next : current);
    };
    const unlisten = listen<OverlayState>(STATE_EVENT, (e) => accept(e.payload));
    void unlisten.then(() => invoke<OverlayState | null>("overlay_snapshot")).then(accept).catch(() => {});
    return () => { disposed = true; void unlisten.then((stop) => stop()).catch(() => {}); };
  }, []);
  return state;
}
