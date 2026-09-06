import { useState } from "react";

/** Keep source names scannable; disclose full paths to keyboard and pointer users. A button
 *  rather than <details>: the settings have no native disclosures left, and a button gets the
 *  shared focus ring and Enter/Space for free. */
export function SourcePath({ path }: { path: string }) {
  const [open, setOpen] = useState(false);
  const name = path.split(/[\\/]/).filter(Boolean).pop() || path;
  return (
    <div className="min-w-0 flex-1 text-xs">
      <button
        type="button"
        aria-expanded={open}
        title={open ? undefined : path}
        onClick={() => setOpen((v) => !v)}
        className="break-words text-left font-medium text-fg hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[color:var(--border-accent)]"
      >
        {name}
      </button>
      {open && <p className="mt-1 break-all text-fg-muted">{path}</p>}
    </div>
  );
}
