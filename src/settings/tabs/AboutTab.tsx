import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, useReducedMotion } from "motion/react";
import { toast } from "sonner";
import { GithubLogo } from "@phosphor-icons/react";
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
import { Logo } from "@/components/Logo";
import { Section, SwitchRow } from "../Section";
import { UpdateChecker } from "../UpdateChecker";
import { ipc, type EmberSettings } from "@/lib/ipc";
import { cn } from "@/lib/utils";

/** Prompt saving, result retention, the legacy plaintext row, logs. Shown under Developer tools. */
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
 * About: the product first, the technical surface only on request.
 *
 * The hero card takes the height: the mark, the name, the version and updates, the source. The
 * gate is the existing Debug mode, relabelled Developer tools, because that is what it does:
 * opens the devtools on this window and, now, shows the diagnostics and the report here. Off,
 * the tab is the hero and one switch; everything technical exists only while it is on.
 */
export function AboutTab({
  s,
  setS,
}: {
  s: EmberSettings;
  setS: React.Dispatch<React.SetStateAction<EmberSettings>>;
}) {
  const still = useReducedMotion();
  const dev = s.debugMode;
  const setDev = (v: boolean) => {
    setS({ ...s, debugMode: v });
    ipc.setDebugMode(v).catch(() => {
      setS((prev) => ({ ...prev, debugMode: !v }));
      toast.error("Couldn't change developer tools.");
    });
  };

  return (
    <div data-tab-body="" className="settings-two-col min-h-0 flex-1">
      {/* One elastic region per tab. The hero takes the height while it is alone; once the
          report exists, the report is the thing that reads better tall, and a hero squeezed
          under it spilled its logo over its own header (measured stacked at 720x856). */}
      <div data-settings-col="" className={cn("settings-col", !dev && "settings-col-grow")}>
        <Section title="Ember" elastic={!dev} hint="Refines the text you select in any app: prompts, emails, messages, docs.">
          <div className="flex min-h-0 flex-1 flex-col">
            {/* With the developer surface on, the hero steps back to a row: stacked at 720x856
                the full mark plus the switch plus the diagnostics already exceeded the panel
                before the report got a single line. */}
            {dev ? (
              <div className="flex items-center gap-3">
                <Logo size={40} />
                <div className="min-w-0">
                  <div className="text-base font-semibold text-fg">Ember</div>
                  <p className="truncate text-xs text-fg-muted">
                    Gemini first, one fallback of your choosing, guided by your profile.
                  </p>
                </div>
              </div>
            ) : (
              <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 text-center">
                <Logo size={96} />
                <div>
                  <div className="text-xl font-semibold text-fg">Ember</div>
                  <p className="mt-1 text-xs text-fg-muted">
                    Gemini first, one fallback of your choosing, guided by your profile.
                  </p>
                </div>
              </div>
            )}
            <div className="mt-auto flex shrink-0 flex-col gap-2 border-t border-[color:var(--border-subtle)] pt-3">
              <UpdateChecker />
              <button
                onClick={() => ipc.openRepo().catch(() => toast.error("Couldn't open the repository."))}
                className="inline-flex w-fit items-center gap-1.5 text-xs text-fg-muted transition-colors hover:text-fg"
                aria-label="Open the Ember source repository on GitHub"
              >
                <GithubLogo size={15} weight="fill" />
                Source on GitHub
              </button>
            </div>
          </div>
        </Section>
        <div className="shrink-0 rounded-lg border border-[color:var(--border-subtle)] bg-surface-1 px-[var(--card-pad,1.25rem)] py-3">
          <SwitchRow
            id="debug-mode"
            label="Developer tools"
            hint="Opens the devtools on this window and shows diagnostics and logs here."
            detail={
              <p>
                Also captures verbose logs. Leave it off unless you are debugging Ember itself;
                nothing here is needed to use it.
              </p>
            }
            checked={dev}
            onCheckedChange={setDev}
          />
        </div>
      </div>
      {dev && (
        <motion.div
          data-settings-col=""
          className="settings-col settings-col-grow settings-col-wide"
          initial={still ? false : { opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: still ? 0 : 0.18, ease: [0.22, 1, 0.36, 1] }}
        >
          <DiagnosticsSection savePrompts={s.savePrompts} keepResults={s.keepResults} />
          <DiagnosticsReport />
        </motion.div>
      )}
    </div>
  );
}
