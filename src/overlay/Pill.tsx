import { m } from "motion/react";
import { WarningCircle, Cursor, Check } from "@phosphor-icons/react";

type Kind = "error" | "hint" | "success";

const ICON = {
  error: <WarningCircle weight="fill" size={16} />,
  hint: <Cursor weight="fill" size={16} />,
  success: <Check weight="bold" size={16} />,
};

/** Pilha de feedback junto ao cursor (erro/hint/sucesso). */
export function Pill({ kind, text }: { kind: Kind; text: string }) {
  const color =
    kind === "error"
      ? "var(--color-error)"
      : kind === "success"
        ? "var(--color-success)"
        : "var(--color-fg-muted)";
  return (
    <m.div
      layoutId="refiner-surface"
      className="ember-bubble flex max-w-[280px] items-center gap-2 px-3 py-2"
      style={{ borderRadius: 14 }}
      initial={{ opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, scale: 0.92 }}
    >
      <span className="shrink-0" style={{ color }}>
        {ICON[kind]}
      </span>
      <span className="text-xs text-fg">{text}</span>
    </m.div>
  );
}
