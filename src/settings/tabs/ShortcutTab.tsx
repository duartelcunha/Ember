import { Label } from "@/components/ui/label";
import { HotkeyCapture } from "../HotkeyCapture";
import { Section, SwitchRow } from "../Section";
import { ipc, type EmberSettings, type HotkeySlot } from "@/lib/ipc";

/** O aviso do macOS so faz sentido no macOS; no Windows seria ruido sobre um problema que la
 *  nao existe (o RegisterHotKey do Windows recusa mesmo os conflitos). */
const IS_MAC = /Mac|iPhone|iPad/.test(navigator.userAgent);

const MODE_SLOTS: { slot: Extract<HotkeySlot, "polish" | "turbo" | "picker">; label: string; ariaLabel: string; key: "hotkeyPolish" | "hotkeyTurbo" | "hotkeyPicker" }[] = [
  { slot: "polish", label: "Fix", ariaLabel: "Fix shortcut", key: "hotkeyPolish" },
  { slot: "turbo", label: "Rebuild", ariaLabel: "Rebuild shortcut", key: "hotkeyTurbo" },
  { slot: "picker", label: "Project picker", ariaLabel: "Project picker shortcut", key: "hotkeyPicker" },
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
  return (
    <div data-tab-body="" className="settings-two-col min-h-0 flex-1 items-start gap-[var(--card-gap,1rem)]">
      <div className="flex min-w-0 flex-col gap-[var(--card-gap,1rem)]">
        <Section
          title="Global shortcut"
          titleId="hotkey-heading"
          hint="Press the combo you want. One key to four, modifiers optional."
          detail={
            <>
              <p>
                It is saved the moment you press it. A combo already taken by another app is
                refused on the spot and nothing is saved, so you can try another right away.
                Press your shortcut again while Ember is working to cancel that refine.
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
          <HotkeyCapture
            value={hotkey}
            slot="main"
            ariaLabel="Main shortcut"
            onCommit={(accel) => commitHotkey("main", accel)}
          />
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
      <Section
        title="Shortcuts per mode"
        hint="Optional. Fire one mode directly, without opening settings first."
        detail={
          <p>
            Off until you set them. The main shortcut keeps using the mode picked in Refining;
            these ignore it and always run their own. Leave one empty and Ember does not claim
            that combo at all.
          </p>
        }
      >
        {MODE_SLOTS.map(({ slot, label, ariaLabel, key }) => (
          <div key={slot} className="flex flex-col gap-1.5">
            <Label>{label}</Label>
            <HotkeyCapture
              value={s[key]}
              slot={slot}
              clearable
              ariaLabel={ariaLabel}
              onCommit={(accel) => commitHotkey(slot, accel)}
            />
          </div>
        ))}
      </Section>
    </div>
  );
}
