import { Button } from "../components/ui/button";
import { Feedback } from "../components/Feedback";
import { SourcePath } from "../components/SourcePath";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ContextSource, ProfileProvenance } from "@/lib/ipc";

type Snapshot = {
  runId: number; selection: string; project: string | null; reason: string;
  profile: string; profileSources: ProfileProvenance[]; profileReviewNeeded: boolean; profileInvalid: boolean;
  projectContext: string | null; sources: ContextSource[]; sourceStatus: string;
  delivery: "prepared" | "sending" | "sent" | "cached" | "unconfirmed";
};
const delivery = { prepared: "Prepared", sending: "Sending", sent: "Sent", cached: "Reused result", unconfirmed: "Delivery unconfirmed" };
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
  return <section className="rounded-lg border border-[color:var(--border-subtle)] p-4">
    <div className="flex items-center justify-between gap-3">
      <span className="text-sm font-medium">{snapshot ? `${snapshot.project ?? "Personal preferences"} · ${mode}` : "Context"}</span>
      <Button variant="ghost" size="sm" loading={loading} onClick={() => void refresh()}>Refresh</Button>
    </div>
    {error && <Feedback tone="error">Context unavailable. Try again.</Feedback>}
    <details className="mt-2 text-xs"><summary className="cursor-pointer text-fg-muted">View context</summary>
      {!snapshot ? <p className="mt-3 text-fg-muted">{loading ? "Loading context..." : "Available after your first request."}</p> : <div className="mt-3 space-y-3">
        <p>{delivery[snapshot.delivery]} · {snapshot.reason}</p>
        <p className="text-fg-muted">Request {snapshot.runId}</p>
        <p className="text-fg-muted">{snapshot.sourceStatus}</p>
        {snapshot.profileReviewNeeded && <p>Personal preferences need review. Operational instructions were excluded.</p>}
        {snapshot.profileInvalid && <p role="alert">Profile exceeds the limit. No refinement was sent.</p>}
        <details><summary>Personal preferences</summary><pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-words">{snapshot.profile || "None"}</pre></details>
        {snapshot.projectContext && <details><summary>Project context</summary><pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-words">{snapshot.projectContext}</pre></details>}
        <details><summary>Sources and exclusions</summary><ul className="mt-2 space-y-2">
          {(snapshot.sources ?? []).map(source => <li key={source.path} className="break-all"><SourcePath path={source.path} /><p className="text-fg-muted">{source.excludedLines} lines excluded</p><code>{source.fingerprint}</code></li>)}
          {(snapshot.profileSources ?? []).map(source => <li key={source.path} className="break-all"><SourcePath path={source.path} /> (reviewed profile snapshot)</li>)}
        </ul></details>
      </div>}
    </details>
  </section>;
}
