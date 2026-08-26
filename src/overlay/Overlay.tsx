import { AnimatePresence, domAnimation, LazyMotion, m, MotionConfig } from "motion/react";
import { useOverlayState } from "./useOverlayController";
import { Orb } from "./Orb";
import { Pill } from "./Pill";
import type { OverlayState } from "./types";

/** Texto anunciado a leitores de ecra por fase. O orb e as pills sao puramente visuais
 *  (aria-hidden); sem isto, um utilizador de tecnologia de apoio nao sabia que o refine
 *  arrancou, acabou ou falhou. `null` = nada a anunciar (fase escondida). */
function announcement(s: OverlayState): string | null {
  switch (s.phase) {
    case "refining":
      return s.message ?? "Refining your selection";
    case "success":
      return s.provider ? `Refined by ${s.provider}` : "Refined";
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
        <div className="flex h-screen items-center justify-start p-2" aria-hidden="true">
          <AnimatePresence mode="popLayout">
            {s.phase === "refining" && (
              // Orb + legenda opcional: o nucleo emite "Trying <provider>..."/"Retrying..."
              // durante fallback/retry, e a cauda do texto a ser gerado durante o stream,
              // para o refine deixar de ser um orb mudo. Largura capada: a janela do
              // overlay so clampa a caixa minuscula do orb ao ecra nesta fase (nao a
              // legenda), por isso o texto tem de caber SEMPRE dentro da janela fixa.
              <div key="orb" className="flex items-center gap-2">
                <Orb />
                {s.message && (
                  <m.span
                    // .ember-bubble tem backdrop-filter: so opacidade anima (sem translate),
                    // senao o fundo desfocado re-amostrava a cada frame do movimento.
                    className="ember-bubble max-w-[190px] overflow-hidden text-ellipsis whitespace-nowrap px-2 py-1 text-xs text-fg"
                    style={{ borderRadius: 10, willChange: "opacity" }}
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={{ duration: 0.4, ease: [0.22, 1, 0.36, 1] }}
                  >
                    {s.message}
                  </m.span>
                )}
              </div>
            )}
            {s.phase === "success" && (
              // Mostra o provider: torna visivel quando o Gemini falhou e o fallback salvou.
              <Pill key="ok" kind="success" text={s.provider ? `Refined by ${s.provider}` : "Refined"} />
            )}
            {s.phase === "error" && (
              <Pill key="err" kind="error" text={s.message ?? "Something went wrong."} />
            )}
            {s.phase === "hint" && (
              <Pill key="hint" kind="hint" text={s.message ?? "Select text first"} />
            )}
            {s.phase === "preview" && (
              // Gate de aprovacao: a decisao (Enter/Esc) e capturada no Rust por keyboard hook,
              // este pill so mostra. `kind="hint"` da o registo neutro de prompt.
              <Pill
                key="preview"
                kind="hint"
                text={s.message ?? "Enter to apply · Esc to keep original"}
              />
            )}
          </AnimatePresence>
        </div>
      </MotionConfig>
    </LazyMotion>
  );
}
