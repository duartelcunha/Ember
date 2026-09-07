import type { ConfirmationScope } from "./types";

export function Preview({ scope }: { scope: ConfirmationScope }) {
  return <div className="ember-bubble ember-chip ember-confirmation text-fg">
    {scope === "field" && <span>Whole field · </span>}
    <span><kbd>Enter</kbd> apply · <kbd>Esc</kbd> cancel</span>
  </div>;
}
