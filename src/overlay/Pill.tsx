import { WarningCircle, Cursor, Check } from "@phosphor-icons/react";
type Kind = "error" | "hint" | "success";
const ICON = { error: WarningCircle, hint: Cursor, success: Check };
export function Pill({ kind, text }: { kind: Kind; text: string }) {
  const Icon = ICON[kind];
  return <div className="ember-bubble flex w-fit max-w-[min(320px,calc(100vw-16px))] items-center gap-1.5 rounded-lg px-2.5 py-1.5">
    <Icon className="shrink-0" weight={kind === "success" ? "bold" : "fill"} size={14}
      style={{ color: kind === "error" ? "var(--color-error)" : "var(--color-accent)" }} />
    <span className="min-w-0 whitespace-pre-wrap break-words text-xs text-fg line-clamp-2">{text}</span>
  </div>;
}
