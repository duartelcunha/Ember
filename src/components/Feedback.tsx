import type { ReactNode } from "react";

export function Feedback({ children, tone = "info" }: { children: ReactNode; tone?: "error" | "success" | "info" }) {
  return <p role={tone === "error" ? "alert" : "status"} aria-atomic="true"
    className={`rounded-sm border border-[color:var(--border-subtle)] bg-surface-1 px-3 py-2 text-xs leading-relaxed ${tone === "error" ? "text-error" : tone === "success" ? "text-success" : "text-fg-muted"}`}>
    {children}
  </p>;
}
