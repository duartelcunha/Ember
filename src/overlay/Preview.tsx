import { m } from "motion/react";

export function Preview({ value }: { value: { original: string[]; result: string[]; page: number } }) {
  const original = value.original;
  const result = value.result;
  const pages = Math.max(1, Math.max(original.length, result.length));
  const page = Math.max(0, Math.min(value.page, pages - 1));
  const part = (text: string[]) => text[page] ?? "";
  return (
    <m.div className="ember-bubble w-[640px] max-w-[calc(100vw-16px)] rounded-xl p-4 text-xs text-fg"
      initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
      <div className="mb-3 flex justify-between gap-4 text-fg-muted">
        <span>Review changes</span><span>{page + 1} / {pages}</span>
      </div>
      <div className="grid grid-cols-1 gap-4 min-[480px]:grid-cols-2">
        <section><h2 className="mb-2 font-semibold">Original</h2><p className="whitespace-pre-wrap break-words font-mono leading-4">{part(original) || "End of original"}</p></section>
        <section><h2 className="mb-2 font-semibold">Result</h2><p className="whitespace-pre-wrap break-words font-mono leading-4">{part(result) || "End of result"}</p></section>
      </div>
      <p className="mt-4 border-t border-current/10 pt-3 text-fg-muted">Enter to apply · Esc to keep original{pages > 1 ? " · Page Up / Page Down to read" : ""}</p>
    </m.div>
  );
}
