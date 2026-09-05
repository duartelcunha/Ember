import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";

type Snapshot = {
  sourceChanged: boolean | null; selection: string; project: string | null; projectSource: string | null;
  profileSources?: { path: string; fingerprint: string }[]; profileSource: string | null; profile: string; projectContext: string | null;
  profileInvalid?: boolean; reason: string; profileTruncated: boolean; configRevision: number;
};

export function ContextInspector() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [error, setError] = useState("");
  async function refresh() {
    try { setSnapshot(await invoke<Snapshot | null>("get_context_snapshot")); setError(""); }
    catch { setError("Context could not be loaded. Try again."); }
  }
  useEffect(() => { void refresh(); }, []);
  return <details className="rounded-lg border border-[color:var(--border-subtle)] p-4">
    <summary className="cursor-pointer text-sm font-medium">Context resolved for the last request</summary>
    <div className="mt-3 space-y-3 text-xs">
      <Button variant="ghost" onClick={() => void refresh()}>Refresh</Button>
      {error && <p role="alert">{error}</p>}
      {!snapshot ? <p>No request has resolved its context in this session.</p> : <>
        <p>{snapshot.reason}. Selection: {snapshot.selection}. Configuration revision: {snapshot.configRevision}.</p>
        {snapshot.sourceChanged && <p role="status">Project sources changed after this brief was generated. Review and regenerate the brief before relying on it.</p>}
        {snapshot.profileInvalid && <p role="status">The global profile is too long. This request was stopped before contacting a model. Shorten the profile in Personalization.</p>}
        {!snapshot.profileInvalid && snapshot.profileTruncated && <p role="status">The global profile exceeds the prompt limit. Only the text shown below was prepared for inclusion. Review and shorten the profile in Personalization.</p>}
        <p>Global source: {snapshot.profileSource ?? "Ember preferences"}</p>
        {snapshot.profileSources?.map(source => <p key={source.path} className="break-all">Reviewed source: {source.path}. Fingerprint: {source.fingerprint}.</p>)}
        <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded bg-surface-1 p-3">{snapshot.profile}</pre>
        <p>Project: {snapshot.project ?? "None"}. Source: {snapshot.projectSource ?? "None"}</p>
        {snapshot.projectContext && <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded bg-surface-1 p-3">{snapshot.projectContext}</pre>}
      </>}
    </div>
  </details>;
}
