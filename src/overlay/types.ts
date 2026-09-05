/** Estado do overlay junto ao cursor. */

export type OverlayPhase = "hidden" | "refining" | "success" | "error" | "hint" | "preview";

export type ConfirmationScope = "selection" | "field";

export interface OverlayState {
  phase: OverlayPhase;
  runId?: number;
  sequence?: number;
  confirmationScope?: ConfirmationScope | null;
  /** Mensagem (fase error/hint). */
  message?: string | null;
  /** Provider usado ("Gemini"/"OpenAI-compatible"), fase success. */
  provider?: string | null;
  /**
   * Os três tons do projeto ativo (`raw`, `mid`, `glow`), ou ausente quando não há projeto.
   *
   * São três e não um: o orb é um gradiente de três paragens, e uma cor chapada achatava a
   * estrela num borrão. É o único sinal que diz, em cada refine, com que projeto ele está a ser
   * feito, e por isso não é decoração: é o que torna seguro um projeto ficar ativo durante dias.
   */
  accent?: [string, string, string] | null;
  /**
   * O nome do projeto ativo, ou ausente quando não há nenhum.
   *
   * A cor sozinha diz que há um projeto; não diz QUAL. Com dois projetos de cores parecidas isso
   * volta a ser adivinhar, que era exatamente o problema que a cor veio resolver.
   */
  project?: string | null;
}

/** Evento emitido pelo nucleo Rust com o novo estado do overlay. */
export const STATE_EVENT = "ember://state";
