import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/**
 * The column every settings tab lives in, and the container the tabs query.
 *
 * The settings window never scrolls as a page. This column takes whatever height the window
 * gives it (`flex-1 min-h-0` under an `h-screen` shell) and declares itself a size container
 * named `settings` (globals.css), so the tabs decide their layout from the space they actually
 * have: two columns from `@xl/settings`, compact spacing under 560px of height. Fixtures mount
 * the tabs inside this same component, so the tests measure the real layout rules.
 *
 * `pt-12` covers the fixed 36px TitleBar plus 12px of air; `px-8` is all the gutter the cards
 * need now that nothing lives outside them.
 */
export function SettingsViewport({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn("settings-viewport mx-auto flex min-h-0 w-full max-w-6xl flex-1 flex-col px-8 pb-4 pt-12", className)}>
      <div className="settings-density flex min-h-0 flex-1 flex-col">{children}</div>
    </div>
  );
}
