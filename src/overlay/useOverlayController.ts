import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { STATE_EVENT, type OverlayState } from "./types";

/** Ouve o evento de estado do nucleo Rust. Sem accoes: o fluxo e automatico. */
export function useOverlayState(): OverlayState {
  const [state, setState] = useState<OverlayState>({ phase: "hidden" });
  useEffect(() => {
    const unlisten = listen<OverlayState>(STATE_EVENT, (e) => setState(e.payload));
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);
  return state;
}
