/** Estado do overlay junto ao cursor. */

export type OverlayPhase = "hidden" | "refining" | "success" | "error" | "hint";

export interface OverlayState {
  phase: OverlayPhase;
  /** Mensagem (fase error/hint). */
  message?: string | null;
  /** Provider usado ("Gemini"/"Claude"), fase success. */
  provider?: string | null;
}

/** Evento emitido pelo nucleo Rust com o novo estado do overlay. */
export const STATE_EVENT = "ember://state";
