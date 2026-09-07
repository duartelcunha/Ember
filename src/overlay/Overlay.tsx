import { domAnimation, LazyMotion, MotionConfig } from "motion/react";
import { useLayoutEffect, useRef } from "react";
import { useOverlayState } from "./useOverlayController";
import { useFloatingPosition } from "../components/useFloatingPosition";
import { ORB_PX } from "../components/floatingGeometry";
import { Orb } from "./Orb";
import { Pill } from "./Pill";
import { Preview } from "./Preview";
import type { OverlayState } from "./types";

/** Texto anunciado a leitores de ecra por fase. O orb e as pills sao puramente visuais
 *  (aria-hidden); sem isto, um utilizador de tecnologia de apoio nao sabia que o refine
 *  arrancou, acabou ou falhou. `null` = nada a anunciar (fase escondida). */
function announcement(s: OverlayState): string | null {
  switch (s.phase) {
    case "refining":
      return s.message ?? "Refining your selection";
    case "success":
      return s.message ?? "Paste sent. Check your text.";
    case "error":
      return s.message ?? "Refine failed";
    case "hint":
      return s.message ?? "Select text first";
    case "preview":
      return `${s.confirmationScope === "field" ? "Whole field. " : ""}Press Enter to apply, Escape to cancel`;
    default:
      return null;
  }
}

/** Raiz do overlay junto ao cursor: orb (refining) ou pilha (success/error/hint). */
export function Overlay() {
  const s = useOverlayState();
  // The mark's drawing pixel decides the ink box the placement measures, so both surfaces get
  // it: the labels sit under the ring and have to clear whatever size it is.
  const px = s.orbPx ?? ORB_PX;
  const floating = useFloatingPosition("ember://overlay-at", s.phase === "refining" ? "orb" : "card", px);
  const labels = useFloatingPosition("ember://overlay-at", "labels", px);
  const previousPhase = useRef(s.phase);
  useLayoutEffect(() => {
    // Morph only the incoming surface. Keeping the departing artwork mounted
    // creates a second loading layer, and scaling moves the cursor-facing edge.
    const surface = floating.current?.firstElementChild;
    if (previousPhase.current === "refining" && s.phase !== "refining") {
      surface?.setAttribute("data-morph-from-orb", "true");
    }
    // The closing state arrives on the same phase, so this node is the one that grew out of the
    // ring and it is still here to collapse back into it. Removed again when it is not closing:
    // two runs of the same phase in a row reuse the node, and a stale attribute would leave the
    // second one already shut.
    if (s.closing) surface?.setAttribute("data-leave", "");
    else surface?.removeAttribute("data-leave");
    previousPhase.current = s.phase;
  }, [s.phase, s.closing, floating]);
  const status = announcement(s);
  return (
    <LazyMotion features={domAnimation} strict>
      <MotionConfig reducedMotion="user">
        {/* Regiao de estado so para leitores de ecra. `assertive` para erros (o utilizador tem
            de saber ja que nada mudou); `polite` para o resto. O orb/pills ficam aria-hidden. */}
        <div
          role="status"
          aria-live={s.phase === "error" ? "assertive" : "polite"}
          className="sr-only"
        >
          {status}
        </div>
        <div
          ref={floating}
          className="ember-floating fixed left-0 top-0 w-max max-w-[calc(100vw-16px)]"
          aria-hidden={s.phase !== "preview"}
          // Redefine as três paragens do gradiente aqui em cima: tudo o que pinta o orb lê estas
          // variáveis, portanto a cor do projeto entra sem cada peça saber que ela existe.
          style={
            s.accent
              ? ({
                  "--color-ember-raw": s.accent[0],
                  "--color-accent": s.accent[1],
                  "--color-ember-glow": s.accent[2],
                } as React.CSSProperties)
              : undefined
          }
        >
            {/* Replace phases immediately. Nested exit animations otherwise retain
                the loading orb above the incoming review even with a zero-duration parent. */}
            {s.phase !== "hidden" && <div key={s.phase} data-enter="">
            {s.phase === "refining" && (
              // Independent labels cannot change the visible ring's cursor anchor.
              <div key="orb" className="ember-orb-row flex items-start gap-2">
                <Orb variant={s.message ? "retry" : "work"} skin={s.orbSkin ?? "ember"} px={px} />
              </div>
            )}
            {s.phase === "success" && (
              // Mostra o provider: torna visivel quando o Gemini falhou e o fallback salvou.
              <Pill key="ok" kind="success" text={s.message ?? "Paste sent. Check your text."} />
            )}
            {s.phase === "error" && (
              <Pill key="err" kind="error" text={s.message ?? "Something went wrong."} />
            )}
            {s.phase === "hint" && (
              <Pill key="hint" kind="hint" text={s.message ?? "Select text first"} />
            )}
            {s.phase === "preview" && <Preview key="preview" scope={s.confirmationScope ?? "selection"} />}
            </div>}
        </div>
        <div ref={labels} className="ember-floating fixed left-0 top-0 w-max max-w-[min(280px,calc(100vw-16px))]" aria-hidden>
          {s.phase === "refining" && <div className="flex flex-col items-start gap-1">
            {s.project && <span className="ember-bubble ember-chip ember-label-in max-w-full font-medium truncate">{s.project}</span>}
            {s.message && <span className="ember-bubble ember-chip ember-label-in max-w-full line-clamp-2">{s.message}</span>}
          </div>}
        </div>
      </MotionConfig>
    </LazyMotion>
  );
}
