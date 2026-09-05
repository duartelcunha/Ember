/** Keep source names scannable; disclose full paths to keyboard and pointer users. */
export function SourcePath({ path }: { path: string }) {
  const name = path.split(/[\\/]/).filter(Boolean).pop() || path;
  return <details className="min-w-0 flex-1 text-xs">
    <summary className="break-words font-medium">{name}</summary>
    <p className="mt-1 break-all text-fg-muted">{path}</p>
  </details>;
}
