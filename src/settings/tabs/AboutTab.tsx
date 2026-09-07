import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { toast } from "sonner";
import { GithubLogo } from "@phosphor-icons/react";
import { Logo } from "@/components/Logo";
import { Section, SwitchRow } from "../Section";
import { UpdateChecker } from "../UpdateChecker";
import { releaseFor } from "@/lib/release";
import { ipc, type EmberSettings } from "@/lib/ipc";

/**
 * What changed in the build that is running.
 *
 * Nothing at all when the list has no entry for this version, which is the point: a hand-written
 * list that outlives its release would otherwise describe a build it was never written for, and
 * a wrong changelog is worse than none. The card below simply gets shorter.
 */
function WhatsNew() {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    getVersion()
      .then(v => { if (live) setVersion(v); })
      .catch(() => {});
    return () => { live = false; };
  }, []);

  const release = releaseFor(version);
  if (!release) return null;

  return (
    <div className="shrink-0">
      <h3 className="text-xs font-semibold text-fg">New in this version</h3>
      <ul className="mt-2 space-y-1.5">
        {release.lines.map(line => (
          <li key={line} className="flex gap-2 text-xs leading-relaxed text-fg-muted">
            <span aria-hidden="true" className="mt-1.5 size-1 shrink-0 rounded-full bg-accent" />
            <span>{line}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * About: what Ember is, what this build changed, and the one switch that opens everything else.
 *
 * The mark used to float in the middle of an otherwise empty card, which is what a page looks
 * like when it has nothing to say. It now anchors a band at the top, beside the name, and the
 * height below it goes to content instead of to air. Everything technical moved to the Developer
 * tab, which appears in the strip while the switch here is on.
 */
export function AboutTab({
  s,
  setS,
}: {
  s: EmberSettings;
  setS: React.Dispatch<React.SetStateAction<EmberSettings>>;
}) {
  const dev = s.debugMode;
  const setDev = (v: boolean) => {
    setS({ ...s, debugMode: v });
    ipc.setDebugMode(v).catch(() => {
      setS((prev) => ({ ...prev, debugMode: !v }));
      toast.error("Couldn't change developer tools.");
    });
  };

  return (
    <div data-tab-body="" className="settings-col min-h-0 flex-1">
      <Section title="Ember" elastic hint="Refines the text you select in any app: prompts, emails, messages, docs.">
        <div className="flex min-h-0 flex-1 flex-col gap-4">
          {/* The band takes the slack, which is the whole point: the mark centred in a tall
              warm field reads as composition, while the same emptiness sitting between the notes
              and the version line reads as a page that ran out of things to say. */}
          <div className="relative flex min-h-[9rem] flex-1 flex-col items-center justify-center gap-3 overflow-hidden rounded-md border border-[color:var(--border-subtle)] bg-surface-2 p-5 text-center">
            <span
              aria-hidden="true"
              className="pointer-events-none absolute inset-0"
              style={{
                background:
                  "radial-gradient(60% 70% at 50% 42%, color-mix(in oklab, var(--color-accent) 20%, transparent), transparent 72%)",
              }}
            />
            <span className="relative shrink-0">
              <Logo size={88} />
            </span>
            <span className="relative min-w-0">
              <span className="block text-xl font-semibold text-fg">Ember</span>
              <span className="mt-1 block text-xs text-fg-muted">
                Gemini first, one fallback of your choosing, guided by your profile.
              </span>
            </span>
          </div>

          <WhatsNew />

          <div className="flex shrink-0 flex-col gap-2 border-t border-[color:var(--border-subtle)] pt-3">
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
          hint="Adds a Developer tab with diagnostics and logs, and opens the devtools on this window."
          detail={
            <p>
              Also captures verbose logs. Leave it off unless you are debugging Ember itself;
              nothing behind it is needed to use the app.
            </p>
          }
          checked={dev}
          onCheckedChange={setDev}
        />
      </div>
    </div>
  );
}
