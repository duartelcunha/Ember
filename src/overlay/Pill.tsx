import { m } from "motion/react";
import { WarningCircle, Cursor, Check } from "@phosphor-icons/react";

type Kind = "error" | "hint" | "success";

const ICON = {
  error: <WarningCircle weight="fill" size={14} />,
  hint: <Cursor weight="fill" size={14} />,
  success: <Check weight="bold" size={14} />,
};

/** Pilha de feedback junto ao cursor (erro/hint/sucesso). A bolha (com backdrop-filter) faz
 *  SO fade: mexer-lhe em transform re-amostra o fundo desfocado cada frame. O movimento fica
 *  no conteudo interno (icone+texto), que nao tem blur, para um enter fluido a 120fps. */
export function Pill({ kind, text }: { kind: Kind; text: string }) {
  const color =
    kind === "error"
      ? "var(--color-error)"
      : kind === "success"
        ? "var(--color-orb-accent)"
        : "var(--color-fg-muted)";
  return (
    <m.div
      // The measured surface contains long messages within the active work area.
      className="ember-bubble flex w-fit max-w-[min(460px,calc(100vw-16px))] items-center gap-1.5 px-2.5 py-1.5"
      // Segundo tempo do morph: a faisca colapsou para o centro (exit do Orb) e a pilula
      // ACENDE a partir desse ponto (origem a esquerda, scale com spring curto). E uma
      // entrada one-shot de ~200ms: re-amostrar o backdrop-filter durante esses frames e
      // barato; o que continua proibido e transform CONTINUO na bolha (o seguimento do
      // cursor a 120fps re-amostrava o blur a cada frame).
      style={{ borderRadius: 12, willChange: "opacity, transform", transformOrigin: "left center" }}
      // Sem deslize lateral (`x: -14 -> 0`) desde que TODAS as fases seguem o cursor: a propria
      // janela ja traz o movimento, e os dois somados liam-se como a pilula a escorregar para
      // um sitio onde nao ficava. Fica so o acender: opacidade + escala a partir do ponteiro.
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0, transition: { duration: 0.3, ease: [0.22, 1, 0.36, 1] } }}
      transition={{
        opacity: { duration: 0.16, ease: "easeOut" },
        scale: { type: "spring", stiffness: 520, damping: 30 },
      }}
    >
      <m.span
        className="shrink-0"
        style={{ color }}
        initial={{ opacity: 0, y: 3 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1], delay: 0.12 }}
      >
        {ICON[kind]}
      </m.span>
      <m.span
        className="min-w-0 whitespace-pre-wrap break-words text-xs text-fg"
        initial={{ opacity: 0, y: 3 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1], delay: 0.2 }}
      >
        {text}
      </m.span>
    </m.div>
  );
}
