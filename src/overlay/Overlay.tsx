import { AnimatePresence, domAnimation, LazyMotion, m, MotionConfig } from "motion/react";
import { useOverlayState } from "./useOverlayController";
import { useFloatingPosition } from "../components/useFloatingPosition";
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
      return s.message ?? "Apply refined text? Press Enter to apply, Escape to keep your original";
    default:
      return null;
  }
}

/** Raiz do overlay junto ao cursor: orb (refining) ou pilha (success/error/hint). */
export function Overlay() {
  const s = useOverlayState();
  const floating = useFloatingPosition("ember://overlay-at", s.phase === "refining" ? "orb" : "card");
  const labels = useFloatingPosition("ember://overlay-at", "labels");
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
          {s.phase === "preview" && s.preview && ` Original: ${s.preview.original[s.preview.page] ?? ""}. Result: ${s.preview.result[s.preview.page] ?? ""}. Page Up and Page Down to read, Enter to apply, Escape to keep the original.`}
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
          <AnimatePresence mode="popLayout">
            {/* A direct motion child gives popLayout the DOM ref needed to remove
                exiting content from measurement during phase changes. */}
            {s.phase !== "hidden" && <m.div key={s.phase} exit={{ opacity: 0, transition: { duration: 0 } }}>
            {s.phase === "refining" && (
              // Independent labels cannot change the visible ring's cursor anchor.
              <div key="orb" className="ember-orb-row flex items-start gap-2">
                <Orb variant={s.message ? "retry" : "work"} />
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
            {s.phase === "preview" && s.preview && <Preview key="preview" value={s.preview} />}
            </m.div>}
          </AnimatePresence>
        </div>
        <div ref={labels} className="ember-floating fixed left-0 top-0 w-max max-w-[min(280px,calc(100vw-16px))]" aria-hidden>
          {s.phase === "refining" && <div className="flex flex-col items-start gap-1">
            {s.project && <span className="ember-bubble max-w-full rounded-lg px-2 py-1 text-xs font-medium truncate">{s.project}</span>}
            {s.message && <span className="ember-bubble max-w-full rounded-lg px-2 py-1 text-xs line-clamp-2">{s.message}</span>}
          </div>}
        </div>
      </MotionConfig>
    </LazyMotion>
  );
}
