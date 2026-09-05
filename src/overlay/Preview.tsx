import type { ConfirmationScope } from "./types";

export function Preview({ scope }: { scope: ConfirmationScope }) {
  return <div className="ember-bubble ember-confirmation text-fg">
    {scope === "field" && <span>Whole field · </span>}
    <span>Enter apply · Esc cancel</span>
  </div>;
}
