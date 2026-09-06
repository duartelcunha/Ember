import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { FieldRow, Section } from "../Section";
import type { EmberSettings, Theme } from "@/lib/ipc";

export function AppearanceTab({ s, setTheme }: { s: EmberSettings; setTheme: (theme: Theme) => void }) {
  return (
    <div data-tab-body="" className="settings-two-col min-h-0 flex-1 items-start gap-[var(--card-gap,1rem)]">
      <Section
        title="Theme"
        titleId="theme-heading"
        hint="For this window. The cursor overlay keeps its own solid surface."
        detail={
          <p>
            The overlay next to your cursor stays dark on purpose: it has to read over any
            application, light or dark. Motion follows the system's reduced-motion setting.
          </p>
        }
      >
        <FieldRow label="Theme" htmlFor="theme-select">
          <Select value={s.theme} onValueChange={(v) => setTheme(v as Theme)}>
            <SelectTrigger id="theme-select" aria-labelledby="theme-heading" className="flex-1">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="dark">Dark (glassy, orange accent)</SelectItem>
              <SelectItem value="cream">Cream (warm light)</SelectItem>
            </SelectContent>
          </Select>
        </FieldRow>
      </Section>
    </div>
  );
}
