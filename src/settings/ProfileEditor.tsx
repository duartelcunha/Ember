import { Feedback } from "../components/Feedback";
import { SourcePath } from "../components/SourcePath";
import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  Dialog,
  DialogBody,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { ipc, type EmberSettings, type ProfileProvenance } from "@/lib/ipc";

function Pane({ title, text }: { title: string; text: string }) {
  return (
    <div className="min-w-0">
      <p className="text-xs font-medium text-fg">{title}</p>
      <pre className="mt-1 whitespace-pre-wrap break-words rounded-md border border-[color:var(--border-subtle)] bg-surface-2 p-3 font-mono text-[11px] leading-relaxed text-fg-muted">{text}</pre>
    </div>
  );
}

/**
 * The profile editor fills its tab: the textarea takes whatever height is left and is the only
 * thing that scrolls. Everything optional (the archived profile, the review of an imported
 * file, the provenance list) opens on top instead of pushing the editor down.
 */
export function ProfileEditor({ settings, onSaved }: { settings: EmberSettings; onSaved: (settings: EmberSettings) => void }) {
  const [error, setError] = useState<string | null>(null);
  const [text, setText] = useState(settings.profileText);
  const [sources, setSources] = useState<ProfileProvenance[]>(settings.profileSources);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [importing, setImporting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveTarget, setSaveTarget] = useState<"save" | "reset" | null>(null);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [restoreOpen, setRestoreOpen] = useState(false);
  const epoch = useRef(0);
  const persisted = JSON.stringify([settings.profileText, settings.profileSources]);
  useEffect(() => {
    epoch.current++;
    setText(settings.profileText);
    setSources(settings.profileSources);
    setWarnings([]);
    // Only a persisted profile change replaces the draft; unrelated settings refreshes do not.
  }, [persisted]);
  useEffect(() => () => { epoch.current++; }, []);
  const profileBytes = new TextEncoder().encode(text.trim()).length;
  const tooLong = profileBytes > settings.profileLimitBytes;

  async function importFiles() {
    const operation = ++epoch.current;
    setError(null);
    setImporting(true);
    try {
      const picked = await open({ multiple: true, directory: false, title: "Choose profile sources to review",
        filters: [{ name: "Markdown or text", extensions: ["md", "markdown", "txt"] }] });
      if (!picked || operation !== epoch.current) return;
      const draft = await ipc.importProfileFiles(Array.isArray(picked) ? picked : [picked]);
      if (operation !== epoch.current) return;
      setText(draft.text);
      setSources(draft.sources);
      setWarnings(draft.warnings);
    } catch (error) {
      if (operation === epoch.current) setError(typeof error === "string" ? error : "Profile import failed.");
    } finally { setImporting(false); }
  }

  async function persist(reset: boolean) {
    const operation = ++epoch.current;
    setError(null);
    setSaving(true);
    setSaveTarget(reset ? "reset" : "save");
    try {
      let updated: EmberSettings;
      if (reset) updated = await ipc.resetProfileToDefault();
      else { await ipc.setProfile(text, sources); updated = await ipc.getSettings(); }
      if (operation !== epoch.current) return;
      onSaved(updated);
      setText(updated.profileText);
      setSources(updated.profileSources);
      setWarnings([]);
      toast.success(reset ? "Using Ember's default profile." : "Reviewed profile saved.");
    } catch (error) {
      if (operation === epoch.current) setError(typeof error === "string" ? error : "Profile could not be saved.");
    } finally { setSaving(false); setSaveTarget(null); }
  }

  /** Puts text in the editor as a draft. Nothing is saved until "Save reviewed profile". */
  function useAsDraft(next: string, warning: string) {
    epoch.current++;
    setText(next);
    setWarnings([warning]);
  }

  const source = settings.profileSource === "default"
    ? "Ember default"
    : settings.profileSources.length ? "reviewed file import, with your edits" : "your saved preferences";
  const excluded = settings.profileText.split("\n")
    .filter(line => line.trim() && !settings.profileReview?.includes(line.trim())).join("\n");

  return (
    <section data-tab-body="" className="flex min-h-0 flex-1 flex-col gap-3" aria-labelledby="profile-heading">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 id="profile-heading" className="text-sm font-semibold">Personalization profile</h2>
          <p className="mt-0.5 text-xs leading-snug text-fg-muted" title={`Writing preferences and technical context, included in every refinement. Files are used only after you import, review and save a snapshot. Current source: ${source}.`}>
            Included in every refinement. Current source: {source}.
          </p>
        </div>
        <div className="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
          {sources.length > 0 && (
            <Popover>
              <PopoverTrigger asChild>
                <Button variant="ghost" size="sm">{sources.length} {sources.length === 1 ? "source" : "sources"}</Button>
              </PopoverTrigger>
              <PopoverContent className="w-96">
                <p>These fingerprints identify the imported snapshots. Changes to the files do not change the saved profile. Import again to review new content.</p>
                <ul className="mt-2 flex flex-col gap-2">
                  {sources.map(source => (
                    <li key={source.path} className="flex flex-col gap-0.5 break-all">
                      <SourcePath path={source.path} />
                      <span className="font-mono text-[10px]">{source.fingerprint}</span>
                    </li>
                  ))}
                </ul>
              </PopoverContent>
            </Popover>
          )}
          {settings.profileArchive && (
            <Dialog open={restoreOpen} onOpenChange={setRestoreOpen}>
              <DialogTrigger asChild>
                <Button variant="ghost" size="sm">Restore previous…</Button>
              </DialogTrigger>
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>Previous profile</DialogTitle>
                  <DialogDescription>The profile that was in use before the last save. Restoring puts it in the editor as a draft; nothing changes until you save.</DialogDescription>
                </DialogHeader>
                <DialogBody>
                  <pre className="whitespace-pre-wrap break-words rounded-md border border-[color:var(--border-subtle)] bg-surface-2 p-3 font-mono text-[11px] leading-relaxed text-fg-muted">{settings.profileArchive}</pre>
                </DialogBody>
                <DialogFooter>
                  <DialogClose asChild>
                    <Button variant="ghost">Cancel</Button>
                  </DialogClose>
                  <Button variant="primary" disabled={saving} onClick={() => {
                    useAsDraft(settings.profileArchive ?? "", "Restored the previous profile as a draft. Save to keep it.");
                    setRestoreOpen(false);
                  }}>Restore as draft</Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
          )}
          {settings.profileReview != null && (
            <Dialog open={reviewOpen} onOpenChange={setReviewOpen}>
              <DialogTrigger asChild>
                <Button variant="ghost" size="sm">Review imported instructions…</Button>
              </DialogTrigger>
              <DialogContent size="lg">
                <DialogHeader>
                  <DialogTitle>Review imported instructions</DialogTitle>
                  <DialogDescription>Operational instructions are excluded from requests. Your saved original is preserved.</DialogDescription>
                </DialogHeader>
                <DialogBody className="flex flex-col gap-3">
                  <div className="grid gap-3 sm:grid-cols-2">
                    <Pane title="Saved original" text={settings.profileText} />
                    <Pane title="Writing and technical context" text={settings.profileReview || "No relevant preferences found."} />
                  </div>
                  <Pane title="Excluded content" text={excluded || "Nothing was excluded."} />
                </DialogBody>
                <DialogFooter>
                  <DialogClose asChild>
                    <Button variant="ghost">Close</Button>
                  </DialogClose>
                  <Button variant="primary" disabled={saving} onClick={() => {
                    useAsDraft(settings.profileReview ?? "", "Review this draft before saving. The previous profile will remain archived.");
                    setReviewOpen(false);
                  }}>Use as draft</Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
          )}
        </div>
      </div>
      {settings.legacyAutoProfileDisabled && (
        <p role="status" className="text-xs leading-snug text-fg-muted" title="Automatic agent-profile loading has been disabled. Import the files you want to use, or keep Ember's default profile.">
          Automatic agent-profile loading has been disabled. Import the files you want to use, or keep Ember's default profile.
        </p>
      )}
      {error && <Feedback tone="error">{error}</Feedback>}
      <Textarea data-scroll-pane="" aria-invalid={tooLong} aria-labelledby="profile-heading" className="min-h-[120px] flex-1"
        value={text} disabled={saving} onChange={event => { epoch.current++; setText(event.target.value); }}
        placeholder="Your writing preferences and technical facts." />
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <p className="shrink-0 text-xs text-fg-muted" title="This profile is included in every refinement.">
          {profileBytes.toLocaleString()} / {settings.profileLimitBytes.toLocaleString()} bytes
        </p>
        {tooLong && <p role="alert" className="min-w-0 flex-1 text-xs leading-snug text-error" title="This profile is too long. Shorten it before saving. The imported draft has not been truncated.">This profile is too long. Shorten it before saving. The imported draft has not been truncated.</p>}
        {!tooLong && warnings.length > 0 && (
          <p role="status" className="min-w-0 flex-1 text-xs leading-snug text-fg-muted" title={warnings.join(" ")}>{warnings.join(" ")}</p>
        )}
        <div className="ml-auto flex flex-wrap gap-2">
          <Button variant="ghost" size="sm" loading={importing} disabled={saving || importing} onClick={() => void importFiles()}>Import files...</Button>
          <Button variant="ghost" size="sm" loading={saveTarget === "reset"} disabled={saving} onClick={() => void persist(true)}>Use Ember default</Button>
          <Button variant="primary" size="sm" loading={saveTarget === "save"} disabled={saving || importing || tooLong} onClick={() => void persist(false)}>Save reviewed profile</Button>
        </div>
      </div>
    </section>
  );
}
