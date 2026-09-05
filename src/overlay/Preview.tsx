import { m } from "motion/react";

// Highlight the changed span without dropping whitespace or splitting joined characters.
export function difference(original: string, result: string) {
  const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });
  const split = (s: string) => Array.from(segmenter.segment(s), part => part.segment);
  const a = split(original), b = split(result);
  let start = 0, end = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  while (end < a.length - start && end < b.length - start && a[a.length - end - 1] === b[b.length - end - 1]) end++;
  const spans = (parts: string[]) => [parts.slice(0, start).join(""), parts.slice(start, parts.length - end).join(""), end ? parts.slice(-end).join("") : ""];
  return [spans(a), spans(b)];
}

export function Preview({ value }: { value: { original: string[]; result: string[]; page: number } }) {
  const pages = Math.max(1, value.original.length, value.result.length);
  const page = Math.max(0, Math.min(value.page, pages - 1));
  const parts = difference(value.original[page] ?? "", value.result[page] ?? "");
  return <m.div className="ember-bubble ember-preview rounded-lg text-fg" initial={false} animate={{ opacity: 1 }} exit={{ opacity: 0, transition: { duration: 0 } }}>
    <div className="mb-2 flex justify-between gap-3 text-fg-muted"><span>Review changes</span>{pages > 1 && <span>{page + 1}/{pages}</span>}</div>
    <div className="grid gap-2">
      {parts.map(([before, changed, after], index) => <section key={index}>
        <h2 className="text-[10px] text-fg-muted">{index === 0 ? "Original" : "Result"}</h2>
        <p>{before}{changed && <mark>{changed}</mark>}{after}{!before && !changed && !after && <span className="text-fg-muted">End</span>}</p>
      </section>)}
    </div>
    <div className="mt-2 border-t border-current/10 pt-2 text-[10px] text-fg-muted">Enter apply · Esc cancel{pages > 1 && " · PgUp/PgDn"}</div>
  </m.div>;
}
