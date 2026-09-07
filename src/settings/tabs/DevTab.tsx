import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Section, SwitchRow } from "../Section";
import { RELEASES } from "@/lib/release";
import { ipc, type EmberSettings } from "@/lib/ipc";

/** Prompt saving, result retention, the legacy plaintext row, logs. */
function DiagnosticsSection({ savePrompts, keepResults }: { savePrompts: boolean; keepResults: boolean }) {
  const [saving, setSaving] = useState(savePrompts);
  const [keeping, setKeeping] = useState(keepResults);
  const [legacyResults, setLegacyResults] = useState(false);
  useEffect(() => { void invoke<boolean>("legacy_results_present").then(setLegacyResults).catch(() => {}); }, []);
  const [logs, setLogs] = useState("");
  const [logsOpen, setLogsOpen] = useState(false);
  const [loadingLogs, setLoadingLogs] = useState(false);

  // The saved values arrive from the async getSettings; resync like the other toggles.
  useEffect(() => setSaving(savePrompts), [savePrompts]);
  useEffect(() => setKeeping(keepResults), [keepResults]);

  const togglePrompts = (v: boolean) => {
    setSaving(v);
    ipc.setSavePrompts(v).catch(() => {
      setSaving(!v);
      toast.error("Couldn't change prompt saving.");
    });
  };

  const toggleKeep = (v: boolean) => {
    setKeeping(v);
    ipc.setKeepResults(v).catch(() => {
      setKeeping(!v);
      toast.error("Couldn't change refine memory.");
    });
  };

  const refreshLogs = async () => {
    setLoadingLogs(true);
    try {
      setLogs(await ipc.readRecentLogs(200));
    } catch {
      toast.error("Couldn't read the logs.");
    } finally {
      setLoadingLogs(false);
    }
  };

  const openLogDir = () => ipc.revealLogDir().catch(() => toast.error("Couldn't open the log folder."));

  return (
    <Section title="Diagnostics" hint="Logs live in a rotating file on your machine and never leave it.">
      <SwitchRow
        id="save-prompts"
        label="Save prompts to a file"
        hint="Writes prompts and replies to prompts.jsonl. Off by default."
        detail={
          <p>
            Writes what was sent to the model and what came back to{" "}
            <span className="font-mono">prompts.jsonl</span>, next to the logs. Off by default:
            unlike the log, this file contains the text you refined. Open the log folder to read
            or delete it.
          </p>
        }
        checked={saving}
        onCheckedChange={togglePrompts}
      />
      <SwitchRow
        id="keep-results"
        label="Keep encrypted results"
        hint="Up to 24 hours, key in the system vault. Off by default."
        detail={
          <p>
            Off by default: results stay in session memory. On, eligible results are kept
            encrypted for up to 24 hours, with the key in the system vault. Turning it off
            deletes the saved results and stops older requests from restoring them.
          </p>
        }
        checked={keeping}
        onCheckedChange={toggleKeep}
      />
      {legacyResults && (
        <div className="flex items-center gap-3 text-xs">
          <span
            className="min-w-0 flex-1 truncate text-fg-muted"
            title="An older version left plaintext results in refine_cache.json. They are preserved and are not loaded automatically."
          >
            An older version left plaintext results in refine_cache.json. They are kept but never loaded.
          </span>
          <Button
            variant="ghost"
            size="sm"
            onClick={async () => {
              try {
                await invoke("delete_legacy_results");
                setLegacyResults(false);
                toast.success("Legacy plaintext results deleted.");
              } catch {
                toast.error("Legacy results could not be deleted.");
              }
            }}
          >
            Delete legacy plaintext results
          </Button>
        </div>
      )}
      <div className="flex flex-wrap gap-2">
        <Dialog
          open={logsOpen}
          onOpenChange={(open) => {
            setLogsOpen(open);
            if (open) void refreshLogs();
          }}
        >
          <DialogTrigger asChild>
            <Button variant="ghost" size="sm">
              View logs…
            </Button>
          </DialogTrigger>
          <DialogContent size="lg">
            <DialogHeader>
              <DialogTitle>Recent logs</DialogTitle>
              <DialogDescription>The last 200 lines of Ember.log. Refresh reads the file again.</DialogDescription>
            </DialogHeader>
            <DialogBody>
              <pre className="whitespace-pre-wrap rounded-md border border-[color:var(--border-subtle)] bg-surface-2 p-3 font-mono text-[11px] leading-relaxed text-fg-muted">
                {logs || (loadingLogs ? "Loading…" : "No log lines yet.")}
              </pre>
            </DialogBody>
            <DialogFooter>
              <Button variant="ghost" size="sm" onClick={openLogDir}>
                Open log folder
              </Button>
              <Button variant="ghost" size="sm" onClick={refreshLogs} loading={loadingLogs}>
                Refresh
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
        <Button variant="ghost" size="sm" onClick={openLogDir}>
          Open log folder
        </Button>
      </div>
    </Section>
  );
}

/**
 * The report Ember would attach to a bug report, on screen.
 *
 * `get_diagnostics` has always existed and its output only ever went to the clipboard, so the
 * one thing you could not do was read it before pasting it somewhere public. Safe to render: the
 * Rust side prints `key_state`, which is only set, missing or unreadable, never key material.
 * Mounted only under Developer tools, so nothing is read until asked for.
 */
function DiagnosticsReport() {
  const [report, setReport] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let live = true;
    ipc
      .getDiagnostics()
      .then((text) => { if (live) setReport(text); })
      .catch(() => { if (live) setReport(null); })
      .finally(() => { if (live) setLoading(false); });
    return () => { live = false; };
  }, []);

  const copy = async () => {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(report);
      toast.success("Diagnostics copied.");
    } catch {
      toast.error("Couldn't copy diagnostics.");
    }
  };

  return (
    <Section
      title="Diagnostics report"
      elastic
      hint="What Ember would attach to a bug report. Nothing here leaves your machine."
      action={
        <Button variant="ghost" size="sm" onClick={copy} disabled={!report}>
          Copy
        </Button>
      }
    >
      <pre
        data-scroll-pane=""
        tabIndex={0}
        aria-label="Diagnostics report"
        className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap break-words rounded-md border border-[color:var(--border-subtle)] bg-surface-2 p-3 font-mono text-[11px] leading-relaxed text-fg-muted"
      >
        {report ?? (loading ? "Reading…" : "Diagnostics are unavailable right now.")}
      </pre>
    </Section>
  );
}

/**
 * Every release Ember can describe, not just the one running.
 *
 * About shows the current entry and stops there, because that is the question someone actually
 * has after an update. The rest is history, and history is a developer's question.
 */
function VersionHistory() {
  return (
    <Section
      title="Version history"
      elastic
      hint="Written by hand at each release, so it says what changed rather than which commits landed."
    >
      <ol data-scroll-pane="" className="min-h-0 flex-1 space-y-3 overflow-auto pr-1">
        {RELEASES.map((release) => (
          <li key={release.version}>
            <span className="font-mono text-xs font-semibold text-fg">{release.version}</span>
            <ul className="mt-1 space-y-0.5">
              {release.lines.map((line) => (
                <li key={line} className="text-xs leading-relaxed text-fg-muted">
                  {line}
                </li>
              ))}
            </ul>
          </li>
        ))}
      </ol>
    </Section>
  );
}

/**
 * Everything technical, in one place, and only while Developer tools is on.
 *
 * It used to live in a second column of About, which made About a product page and a debugging
 * console at the same time and left neither with room. A tab appears and disappears with the
 * switch instead: About stays About, and the technical surface gets a whole panel.
 */
export function DevTab({ s }: { s: EmberSettings }) {
  return (
    <div data-tab-body="" className="settings-two-col min-h-0 flex-1">
      <div data-settings-col="" className="settings-col settings-col-grow settings-col-wide">
        <DiagnosticsSection savePrompts={s.savePrompts} keepResults={s.keepResults} />
        <DiagnosticsReport />
      </div>
      <div data-settings-col="" className="settings-col">
        <VersionHistory />
      </div>
    </div>
  );
}
