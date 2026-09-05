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
  const floating = useFloatingPosition("ember://overlay-at");
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
            {s.phase === "refining" && (
              // Orb + legenda opcional: o nucleo emite "Trying <provider>..."/"Retrying..."
              // durante fallback/retry, e a cauda do texto a ser gerado durante o stream,
              // para o refine deixar de ser um orb mudo. Largura capada: a janela do
              // overlay so clampa a caixa minuscula do orb ao ecra nesta fase (nao a
              // legenda), por isso o texto tem de caber SEMPRE dentro da janela fixa.
              <div key="orb" className="ember-orb-row flex items-start gap-2">
                {/* A faísca cresce quando há mensagem, que é exatamente quando o núcleo está
                    a repetir ou a mudar de provider: o ponteiro diz "isto está a custar" antes
                    de a legenda ao lado ser lida. */}
                <Orb variant={s.message ? "retry" : "work"} />
                {/* O projeto ativo aparece SEMPRE que existe, e não só quando há mensagem: é a
                    resposta a "com que contexto é que este refine está a ser feito", e essa
                    pergunta faz-se em todos os refines, não só nos que fazem retry. A cor
                    diz que há um projeto; esta etiqueta diz qual. */}
                <div className="flex min-w-0 max-w-[min(280px,calc(100vw-64px))] flex-col items-start gap-1">
                {s.project && (
                  <m.span
                    className="ember-bubble min-w-0 max-w-full whitespace-normal break-words px-2 py-1 text-xs font-semibold"
                    title={s.project}
                    style={{
                      borderRadius: 10,
                      willChange: "opacity",
                      color: "var(--color-accent)",
                    }}
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={{ duration: 0.4, ease: [0.22, 1, 0.36, 1] }}
                  >
                    <span className="line-clamp-3">{s.project}</span>
                  </m.span>
                )}
                {s.message && (
                  <m.span
                    // .ember-bubble tem backdrop-filter: so opacidade anima (sem translate),
                    // senao o fundo desfocado re-amostrava a cada frame do movimento.
                    className="ember-bubble min-w-0 max-w-full whitespace-normal break-words px-2 py-1 text-xs text-fg"
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
          </AnimatePresence>
        </div>
      </MotionConfig>
    </LazyMotion>
  );
}
