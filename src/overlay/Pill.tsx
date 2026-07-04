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
      className="ember-bubble flex max-w-[280px] items-center gap-1.5 px-2.5 py-1.5"
      style={{ borderRadius: 12, willChange: "opacity" }}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.16, ease: [0.22, 1, 0.36, 1] }}
    >
      <m.span
        className="shrink-0"
        style={{ color }}
        initial={{ opacity: 0, y: 2 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.2, ease: "easeOut", delay: 0.02 }}
      >
        {ICON[kind]}
      </m.span>
      <m.span
        className="text-xs text-fg"
        initial={{ opacity: 0, y: 2 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.2, ease: "easeOut", delay: 0.05 }}
      >
        {text}
      </m.span>
    </m.div>
  );
}
