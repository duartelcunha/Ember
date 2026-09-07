import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../components/ui/button";
import { SourcePath } from "../components/SourcePath";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "../components/ui/dialog";
import type { ContextSource, ProfileProvenance } from "@/lib/ipc";

type Snapshot = {
  runId: number; selection: string; project: string | null; reason: string;
  profile: string; profileSources: ProfileProvenance[]; profileReviewNeeded: boolean; profileInvalid: boolean;
  projectContext: string | null; sources: ContextSource[]; sourceStatus: string;
  delivery: "prepared" | "sending" | "sent" | "cached" | "unconfirmed";
};
const delivery = { prepared: "Prepared", sending: "Sending", sent: "Sent", cached: "Reused result", unconfirmed: "Delivery unconfirmed" };

/** A labelled row of the details dialog. */
function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-3 text-xs">
      <dt className="w-28 shrink-0 text-fg-muted">{label}</dt>
      <dd className="min-w-0 flex-1 break-words text-fg">{children}</dd>
    </div>
  );
}

function Preview({ title, text }: { title: string; text: string }) {
  return (
    <div className="min-w-0">
      <p className="text-xs font-medium text-fg">{title}</p>
      <pre className="mt-1 whitespace-pre-wrap break-words rounded-md border border-[color:var(--border-subtle)] bg-surface-2 p-3 font-mono text-[11px] leading-relaxed text-fg-muted">{text || "None"}</pre>
    </div>
  );
}

/**
 * What the last refine sent along with the text, on one line, with the full breakdown behind a
 * Details dialog. It used to be a card with three levels of native disclosures and raw status
 * lines, which read as debug output and pushed the project list down.
 */
export function ContextInspector() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);
  const epoch = useRef(0);
  async function refresh() {
    const request = ++epoch.current;
    setLoading(true);
    try {
      const next = await invoke<Snapshot | null>("get_context_snapshot");
      if (request === epoch.current) { setSnapshot(current => next && (!current || next.runId >= current.runId) ? next : current); setError(false); }
    } catch { if (request === epoch.current) setError(true); }
    finally { if (request === epoch.current) setLoading(false); }
  }
  useEffect(() => { void refresh(); return () => { epoch.current++; }; }, []);
  const mode = snapshot?.selection === "pinned" ? "Pinned" : snapshot?.selection === "auto" ? "Automatic" : "No project";
  const title = snapshot ? `${snapshot.project ?? "Personal preferences"} · ${mode}` : "Context";
  const status = error
    ? "Context unavailable. Try again."
    : !snapshot
      ? loading ? "Loading context…" : "Available after your first request."
      : `${delivery[snapshot.delivery]} · ${snapshot.reason}`;
  const attention = snapshot?.profileInvalid
    ? "Profile exceeds the limit. No refinement was sent."
    : snapshot?.profileReviewNeeded
      ? "Personal preferences need review. Operational instructions were excluded."
      : null;

  return (
    <section className="flex h-8 shrink-0 items-center gap-3 text-xs" aria-label="Context of the last refine">
      <span className="shrink-0 text-sm font-medium text-fg">{title}</span>
      <span
        role={error ? "alert" : undefined}
        className={`min-w-0 flex-1 truncate ${error ? "text-error" : "text-fg-muted"}`}
        title={attention ? `${status}. ${attention}` : status}
      >
        {status}
        {attention && <span className="text-warning"> · Needs review</span>}
      </span>
      <Dialog>
        <DialogTrigger asChild>
          <Button variant="ghost" size="sm" aria-label="Context details" disabled={!snapshot}>
            Details
          </Button>
        </DialogTrigger>
        {snapshot && (
          <DialogContent size="lg">
            <DialogHeader>
              <DialogTitle>{title}</DialogTitle>
              <DialogDescription>{status}</DialogDescription>
            </DialogHeader>
            <DialogBody className="flex flex-col gap-4">
              <dl className="flex flex-col gap-1.5">
                <Row label="Request">{snapshot.runId}</Row>
                <Row label="Delivery">{delivery[snapshot.delivery]}</Row>
                <Row label="Reason">{snapshot.reason}</Row>
                <Row label="Sources">{snapshot.sourceStatus}</Row>
                {attention && (
                  <Row label="Attention">
                    <span role={snapshot.profileInvalid ? "alert" : undefined} className="text-warning">{attention}</span>
                  </Row>
                )}
              </dl>
              <div className="grid gap-3 sm:grid-cols-2">
                <Preview title="Personal preferences" text={snapshot.profile} />
                {snapshot.projectContext && <Preview title="Project context" text={snapshot.projectContext} />}
              </div>
              <div>
                <p className="text-xs font-medium text-fg">Sources and exclusions</p>
                {(snapshot.sources?.length ?? 0) + (snapshot.profileSources?.length ?? 0) === 0 ? (
                  <p className="mt-1 text-xs text-fg-muted">No files were read for this request.</p>
                ) : (
                  <ul className="mt-1 flex flex-col gap-2">
                    {(snapshot.sources ?? []).map(source => (
                      <li key={source.path} className="flex flex-col gap-0.5 text-xs">
                        <SourcePath path={source.path} />
                        <span className="text-fg-muted">{source.excludedLines} lines excluded · <code className="font-mono">{source.fingerprint}</code></span>
                      </li>
                    ))}
                    {(snapshot.profileSources ?? []).map(source => (
                      <li key={source.path} className="flex flex-col gap-0.5 text-xs">
                        <SourcePath path={source.path} />
                        <span className="text-fg-muted">Reviewed profile snapshot</span>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </DialogBody>
          </DialogContent>
        )}
      </Dialog>
      <Button variant="ghost" size="sm" aria-label="Refresh context" loading={loading} onClick={() => void refresh()}>
        Refresh
      </Button>
    </section>
  );
}
