/** Estado do overlay junto ao cursor. */

export type OverlayPhase = "hidden" | "refining" | "success" | "error" | "hint" | "preview";

export interface OverlayState {
  phase: OverlayPhase;
  /** Mensagem (fase error/hint). */
  message?: string | null;
  /** Provider usado ("Gemini"/"OpenAI-compatible"), fase success. */
  provider?: string | null;
}

/** Evento emitido pelo nucleo Rust com o novo estado do overlay. */
export const STATE_EVENT = "ember://state";
