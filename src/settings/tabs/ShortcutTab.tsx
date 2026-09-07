import { Label } from "@/components/ui/label";
import { HotkeyCapture } from "../HotkeyCapture";
import { Section, SwitchRow } from "../Section";
import { MODE_COPY } from "./RefiningTab";
import { ipc, type EmberSettings, type HotkeySlot } from "@/lib/ipc";

/** O aviso do macOS so faz sentido no macOS; no Windows seria ruido sobre um problema que la
 *  nao existe (o RegisterHotKey do Windows recusa mesmo os conflitos). */
const IS_MAC = /Mac|iPhone|iPad/.test(navigator.userAgent);

type SlotKey = "hotkey" | "hotkeyPolish" | "hotkeyTurbo" | "hotkeyPicker";

/**
 * All four shortcuts, in one list, each saying what it does.
 *
 * They used to be split one card of one capture beside another of three, which was both
 * unbalanced and quietly wrong: the split implied the main shortcut was a different kind of
 * thing. It is not, it is the one that follows the mode picked in Refining. The descriptions of
 * Fix and Rebuild come from `MODE_COPY`, the same strings the Refining tab shows, so nobody has
 * to open another tab to learn what Rebuild would do to their text.
 */
const SHORTCUTS: { slot: HotkeySlot; key: SlotKey; label: string; description: string; clearable: boolean }[] = [
  {
    slot: "main",
    key: "hotkey",
    label: "Global shortcut",
    description: "Uses the mode picked in Refining. Press it again while Ember works to cancel.",
    clearable: false,
  },
  { slot: "polish", key: "hotkeyPolish", label: "Fix", description: MODE_COPY.polish.hint, clearable: true },
  { slot: "turbo", key: "hotkeyTurbo", label: "Rebuild", description: MODE_COPY.turbo.hint, clearable: true },
  {
    slot: "picker",
    key: "hotkeyPicker",
    label: "Project picker",
    description: "Opens the project list next to your cursor.",
    clearable: true,
  },
];

export function ShortcutTab({
  s,
  setS,
  hotkey,
  commitHotkey,
}: {
  s: EmberSettings;
  setS: React.Dispatch<React.SetStateAction<EmberSettings>>;
  hotkey: string;
  commitHotkey: (which: HotkeySlot, accel: string) => Promise<string | null>;
}) {
  const valueOf = (key: SlotKey) => (key === "hotkey" ? hotkey : s[key]);
  return (
    // One column, top to bottom: the shortcuts card takes the height, Startup sits under it. No
    // centring. A centred block left a gap above it that read as broken, not as roomy.
    <div data-tab-body="" className="settings-col min-h-0 flex-1">
      <Section
        title="Shortcuts"
        titleId="hotkey-heading"
        elastic
        hint="Click a box, press the combination. Esc cancels."
        detail={
          <>
            <p>
              Each one is saved the moment you press it. A combination already taken by another
              app is refused on the spot and nothing is saved, so you can try another right away.
              Leave one of the mode shortcuts empty and Ember does not claim that combination at
              all.
            </p>
            {IS_MAC && (
              <p className="mt-2">
                On macOS some system shortcuts win over any app without reporting a conflict.
                Ember knows the common ones and refuses them, but if a shortcut saves and then
                never fires, that is what happened: pick another.
              </p>
            )}
          </>
        }
      >
        <div className="shortcut-rows shrink-0">
          {SHORTCUTS.map(({ slot, key, label, description, clearable }) => (
            <div key={slot} className="flex min-w-0 flex-col gap-1.5">
              <div className="flex min-w-0 items-baseline gap-2">
                <Label className="shrink-0">{label}</Label>
                <span
                  className="min-w-0 truncate text-xs text-fg-muted [display:var(--hint,block)]"
                  title={description}
                >
                  {description}
                </span>
              </div>
              <HotkeyCapture
                value={valueOf(key)}
                slot={slot}
                label={label}
                clearable={clearable}
                ariaLabel={`${label} shortcut`}
                onCommit={(accel) => commitHotkey(slot, accel)}
              />
            </div>
          ))}
        </div>
      </Section>
      <Section title="Startup" hint="Launch Ember automatically with Windows.">
        <SwitchRow
          id="autostart"
          label="Start with Windows"
          checked={s.autostart}
          onCheckedChange={(v) => {
            setS({ ...s, autostart: v });
            ipc.setAutostart(v).catch(() => setS((prev) => ({ ...prev, autostart: !v })));
          }}
        />
      </Section>
    </div>
  );
}
