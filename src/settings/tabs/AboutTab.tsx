import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
import { Section, SwitchRow } from "../Section";
import { UpdateChecker } from "../UpdateChecker";
import { ipc, type EmberSettings } from "@/lib/ipc";

/** Diagnostico e modo debug: toggles, leitor de logs recentes (num dialog), abrir a pasta,
 *  copiar report. */
function DiagnosticsSection({
  debugMode,
  savePrompts,
  keepResults,
}: {
  debugMode: boolean;
  savePrompts: boolean;
  keepResults: boolean;
}) {
  const [on, setOn] = useState(debugMode);
  const [saving, setSaving] = useState(savePrompts);
  const [keeping, setKeeping] = useState(keepResults);
  const [legacyResults, setLegacyResults] = useState(false);
  useEffect(() => { void invoke<boolean>("legacy_results_present").then(setLegacyResults).catch(() => {}); }, []);
  const [logs, setLogs] = useState("");
  const [logsOpen, setLogsOpen] = useState(false);
  const [loadingLogs, setLoadingLogs] = useState(false);

  // debugMode chega do getSettings assincrono; ressincroniza como os outros toggles.
  useEffect(() => setOn(debugMode), [debugMode]);
  useEffect(() => setSaving(savePrompts), [savePrompts]);
  useEffect(() => setKeeping(keepResults), [keepResults]);

  const toggle = (v: boolean) => {
    setOn(v);
    ipc.setDebugMode(v).catch(() => {
      setOn(!v);
      toast.error("Couldn't change debug mode.");
    });
  };

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

  const copyDiagnostics = async () => {
    try {
      await navigator.clipboard.writeText(await ipc.getDiagnostics());
      toast.success("Diagnostics copied.");
    } catch {
      toast.error("Couldn't copy diagnostics.");
    }
  };

  return (
    <Section
      title="Diagnostics"
      hint="Logs live in a rotating file on your machine and never leave it."
    >
      <SwitchRow
        id="debug-mode"
        label="Debug mode"
        hint="Opens the devtools and captures verbose logs."
        checked={on}
        onCheckedChange={toggle}
      />
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
        <Button variant="ghost" size="sm" onClick={copyDiagnostics}>
          Copy diagnostics
        </Button>
      </div>
    </Section>
  );
}

export function AboutTab({ s }: { s: EmberSettings }) {
  return (
    <div data-tab-body="" className="grid min-h-0 flex-1 grid-cols-1 items-start gap-[var(--card-gap,1rem)] @xl/settings:grid-cols-2">
      <Section
        title="Ember"
        hint="Refines the text you select in any app: prompts, emails, messages, docs."
        detail={
          <p>
            Gemini as the free primary, with one OpenAI-compatible fallback of your choosing,
            guided by your profile. Updates are checked against the latest GitHub release, signed
            and verified. Built with Tauri.
          </p>
        }
      >
        <UpdateChecker />
        <button
          onClick={() => ipc.openRepo().catch(() => toast.error("Couldn't open the repository."))}
          className="inline-flex w-fit items-center gap-1.5 text-xs text-fg-muted transition-colors hover:text-fg"
          aria-label="Open the Ember source repository on GitHub"
        >
          <GithubLogo size={15} weight="fill" />
          Source on GitHub
        </button>
      </Section>
      <DiagnosticsSection debugMode={s.debugMode} savePrompts={s.savePrompts} keepResults={s.keepResults} />
    </div>
  );
}
