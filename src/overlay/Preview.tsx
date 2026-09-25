import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useLayoutEffect, useRef } from "react";
import type { ConfirmationScope } from "./types";

export function Preview({ scope, refined, runId }: { scope: ConfirmationScope; refined?: string | null; runId?: number }) {
  const text = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (runId === undefined) return;
    // The native gate consumes Enter until this run's review has reached a painted frame.
    let paintedFrame = 0;
    const frame = requestAnimationFrame(() => {
      // A second frame leaves a paint opportunity before Enter can approve this result.
      paintedFrame = requestAnimationFrame(() => { void invoke("preview_ready", { runId }).catch(() => {}); });
    });
    return () => { cancelAnimationFrame(frame); cancelAnimationFrame(paintedFrame); };
  }, [runId, refined]);
  useEffect(() => {
    if (!refined) return;
    let disposed = false;
    const unlisten = listen<{ runId: number; direction: number }>("ember://preview-scroll", ({ payload }) => {
      if (!disposed && payload.runId === runId) {
        text.current?.scrollBy({ top: payload.direction * Math.max(80, text.current.clientHeight - 32), behavior: "instant" });
      }
    });
    return () => { disposed = true; void unlisten.then((stop) => stop()).catch(() => {}); };
  }, [refined, runId]);

  if (!refined) {
    return <div className="ember-bubble ember-chip ember-confirmation text-fg">
      {scope === "field" && <span>Whole field · </span>}
      <span><kbd>Enter</kbd> apply · <kbd>Esc</kbd> cancel</span>
    </div>;
  }

  return <div className="ember-bubble ember-chip ember-review text-fg" role="region" aria-label="Refined result review">
    <div className="font-semibold">Review {scope === "field" ? "whole field" : "selection"}</div>
    <div ref={text} className="ember-review-text" aria-label="Refined result" aria-live="polite" aria-atomic="true">{refined}</div>
    <div className="ember-review-actions">
      <span><kbd>PgUp</kbd>/<kbd>PgDn</kbd> read all</span>
      <span><kbd>Enter</kbd> apply · <kbd>Esc</kbd> keep original</span>
    </div>
  </div>;
}
