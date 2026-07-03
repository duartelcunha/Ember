import { AnimatePresence, domAnimation, LazyMotion, m, MotionConfig } from "motion/react";
import { useOverlayState } from "./useOverlayController";
import { Orb } from "./Orb";
import { Pill } from "./Pill";

/** Raiz do overlay junto ao cursor: orb (refining) ou pilha (success/error/hint). */
export function Overlay() {
  const s = useOverlayState();
  return (
    <LazyMotion features={domAnimation} strict>
      <MotionConfig reducedMotion="user">
        <div className="flex h-screen items-center justify-start p-2">
          <AnimatePresence mode="popLayout">
            {s.phase === "refining" && (
              // Orb + legenda opcional: o nucleo emite "Trying Claude...""/"Retrying..."
              // durante fallback/retry, e a cauda do texto a ser gerado durante o stream,
              // para o refine deixar de ser um orb mudo. Largura capada: a janela do
              // overlay so clampa a caixa minuscula do orb ao ecra nesta fase (nao a
              // legenda), por isso o texto tem de caber SEMPRE dentro da janela fixa.
              <div key="orb" className="flex items-center gap-2">
                <Orb />
                {s.message && (
                  <m.span
                    className="ember-bubble max-w-[190px] overflow-hidden text-ellipsis whitespace-nowrap px-2 py-1 text-xs text-fg"
                    style={{ borderRadius: 10 }}
                    initial={{ opacity: 0, x: -4 }}
                    animate={{ opacity: 1, x: 0 }}
                    exit={{ opacity: 0 }}
                  >
                    {s.message}
                  </m.span>
                )}
              </div>
            )}
            {s.phase === "success" && (
              // Mostra o provider: torna visivel quando o Gemini falhou e o Claude salvou.
              <Pill key="ok" kind="success" text={s.provider ? `Refined by ${s.provider}` : "Refined"} />
            )}
            {s.phase === "error" && (
              <Pill key="err" kind="error" text={s.message ?? "Something went wrong."} />
            )}
            {s.phase === "hint" && (
              <Pill key="hint" kind="hint" text={s.message ?? "Select text first"} />
            )}
          </AnimatePresence>
        </div>
      </MotionConfig>
    </LazyMotion>
  );
}
