import type { ReactNode } from "react";

export function Feedback({ children, tone = "info" }: { children: ReactNode; tone?: "error" | "success" | "info" }) {
  return <p role={tone === "error" ? "alert" : "status"} aria-atomic="true"
    className={`border-l-2 border-l-current pl-3 text-xs leading-relaxed ${tone === "error" ? "text-error" : tone === "success" ? "text-success" : "text-fg-muted"}`}>
    {children}
  </p>;
}
