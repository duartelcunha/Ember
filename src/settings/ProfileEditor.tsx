import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { ipc, type EmberSettings, type ProfileProvenance } from "@/lib/ipc";

export function ProfileEditor({ settings, onSaved }: { settings: EmberSettings; onSaved: (settings: EmberSettings) => void }) {
  const [text, setText] = useState(settings.profileText);
  const [sources, setSources] = useState<ProfileProvenance[]>(settings.profileSources);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [importing, setImporting] = useState(false);
  const [saving, setSaving] = useState(false);
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
  const tooLong = new TextEncoder().encode(text.trim()).length > 2000;

  async function importFiles() {
    const operation = ++epoch.current;
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
      if (operation === epoch.current) toast.error(typeof error === "string" ? error : "Profile import failed.");
    } finally { setImporting(false); }
  }

  async function persist(reset: boolean) {
    const operation = ++epoch.current;
    setSaving(true);
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
      if (operation === epoch.current) toast.error(typeof error === "string" ? error : "Profile could not be saved.");
    } finally { setSaving(false); }
  }

  return <section className="space-y-4" aria-labelledby="profile-heading">
    <div><h2 id="profile-heading" className="text-sm font-semibold">Personalization profile</h2>
      <p className="mt-1 text-sm text-fg-muted">Writing preferences and technical context for every refinement. Files are used only after you import, review and save a snapshot.</p></div>
    {settings.legacyAutoProfileDisabled && <p role="status" className="text-sm text-fg-muted">Automatic agent-profile loading has been disabled. Import the files you want to use, or keep Ember's default profile.</p>}
    <p className="text-xs text-fg-muted">Current source: {settings.profileSource === "default" ? "Ember default" : settings.profileSources.length ? "reviewed file import, with your edits" : "your saved preferences"}.</p>
    <Textarea aria-labelledby="profile-heading" className="h-[clamp(140px,38vh,420px)] min-h-0 resize-none overflow-y-auto"
      value={text} disabled={saving} onChange={event => { epoch.current++; setText(event.target.value); }}
      placeholder="Your writing preferences and technical facts." />
    {tooLong && <p role="alert" className="text-sm text-error">This profile is too long. Shorten it before saving. The imported draft has not been truncated.</p>}
    {warnings.length > 0 && <div role="status" className="space-y-2 text-xs text-fg-muted">{warnings.map((warning, index) => <p key={index}>{warning}</p>)}</div>}
    {sources.length > 0 && <details className="text-xs text-fg-muted"><summary className="cursor-pointer">Import provenance ({sources.length} sources)</summary>
      <p className="mt-2">These fingerprints identify the imported snapshots. Changes to the files do not change the saved profile. Import again to review new content.</p>
      <ul className="mt-2 space-y-2">{sources.map(source => <li key={source.path} className="break-all"><span>{source.path}</span><br /><span className="font-mono">{source.fingerprint}</span></li>)}</ul>
    </details>}
    <div className="flex flex-wrap gap-2">
      <Button variant="primary" disabled={saving || importing || tooLong} onClick={() => void persist(false)}>{saving ? "Saving..." : "Save reviewed profile"}</Button>
      <Button variant="ghost" disabled={saving || importing} onClick={() => void importFiles()}>{importing ? "Reading selected files..." : "Import files..."}</Button>
      <Button variant="ghost" disabled={saving} onClick={() => void persist(true)}>Use Ember default</Button>
    </div>
  </section>;
}
