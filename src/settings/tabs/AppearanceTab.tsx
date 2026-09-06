import { useEffect, useState } from "react";
import { LazyMotion, domAnimation, useReducedMotion } from "motion/react";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { FieldRow, Section } from "../Section";
import { Orb } from "../../overlay/Orb";
import { Pill } from "../../overlay/Pill";
import { Preview } from "../../overlay/Preview";
import type { EmberSettings, Theme } from "@/lib/ipc";
import { cn } from "@/lib/utils";

type StageState = "refining" | "confirm" | "applied" | "problem";

const STAGE_COPY: Record<StageState, { label: string; caption: string }> = {
  refining: { label: "Working", caption: "The ember sits by your cursor while the model answers." },
  confirm: { label: "Confirm", caption: "With Confirm before applying on, Enter pastes and Esc keeps your original." },
  applied: { label: "Applied", caption: "A short note naming the service that answered, then it fades." },
  problem: { label: "Problem", caption: "When something fails, Ember says so and never touches your text." },
};

/** The loop skips `problem`: a demo that keeps flashing an error reads as a real error. */
const CYCLE: StageState[] = ["refining", "confirm", "applied"];

/**
 * A stand-in for someone else's application: light on one side, dark on the other, with faint
 * skeleton lines. It is the argument for why the overlay does not follow this window's theme,
 * made by showing rather than by claiming it in prose.
 */
function StageBackdrop() {
  const lines = [90, 64, 96, 44];
  return (
    <div aria-hidden="true" className="absolute inset-0 flex">
      {(["#eceae6", "#1e2124"] as const).map((background, half) => (
        <div key={background} className="flex flex-1 flex-col justify-center gap-2.5 px-5" style={{ background }}>
          {lines.map((width, i) => (
            <span
              key={i}
              className="block h-1 rounded-full"
              style={{ width: `${width}%`, background: half === 0 ? "rgba(46,37,25,0.13)" : "rgba(255,246,235,0.13)" }}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

/**
 * What Ember puts next to your cursor, in this window.
 *
 * These are the real components from `src/overlay`, not a drawing of them, so the preview cannot
 * drift from the thing it depicts. Two mechanics make that work:
 *
 * - `Orb` renders through `m.div`, whose features only load under a `LazyMotion` ancestor.
 *   Without one it mounts at opacity 0 and stays invisible for everyone who has not turned
 *   reduced motion on. Non-strict on purpose: `strict` throws for any `motion.*` descendant, and
 *   the settings tree uses plain `motion.*` in several places.
 * - The stage opts out of the settings theme (`.ember-overlay-preview` in globals.css). The
 *   overlay window never gets a `data-theme` attribute, so it is always dark; a preview that went
 *   cream in a cream window would be showing something the user will never see.
 */
function OverlayStage() {
  const still = useReducedMotion();
  const [state, setState] = useState<StageState>("refining");
  // A pick is intent and it beats the demo: the loop stops for good, rather than moving the
  // thing the user just chose to look at.
  const [picked, setPicked] = useState(false);
  const [paused, setPaused] = useState(false);

  useEffect(() => {
    if (still || picked || paused) return;
    const timer = setInterval(
      () => setState((current) => CYCLE[(CYCLE.indexOf(current) + 1) % CYCLE.length]),
      2400,
    );
    return () => clearInterval(timer);
  }, [still, picked, paused]);

  return (
    <Section
      title="Next to your cursor"
      elastic
      hint="The overlay stays dark on purpose: it has to read over any app, light or dark."
      detail={
        <p>
          This is the real overlay, rendered here. It never follows the theme above, because it
          appears over whatever application you were typing in, and a surface that changed with
          your settings would be unreadable over half of them.
        </p>
      }
    >
      <LazyMotion features={domAnimation}>
        <div
          data-overlay-preview=""
          className="ember-overlay-preview relative min-h-[9rem] flex-auto overflow-hidden rounded-md"
          onPointerEnter={() => setPaused(true)}
          onPointerLeave={() => setPaused(false)}
        >
          <StageBackdrop />
          {/* A picture of a UI, not the UI: the caption below carries the meaning. */}
          <div aria-hidden="true" className="absolute inset-0 grid place-items-center">
            {state === "refining" && <Orb variant="work" />}
            {state === "confirm" && <Preview scope="selection" />}
            {state === "applied" && <Pill kind="success" text="Refined with Gemini" />}
            {state === "problem" && <Pill kind="error" text="Gemini hit its quota. Ember used the fallback." />}
          </div>
        </div>
      </LazyMotion>
      <div role="radiogroup" aria-label="Overlay state" className="flex shrink-0 flex-wrap gap-1">
        {(Object.keys(STAGE_COPY) as StageState[]).map((value) => (
          <label
            key={value}
            className={cn(
              "cursor-pointer rounded-full border px-2.5 py-1 text-[11px] transition-colors",
              "focus-within:outline-none focus-within:ring-2 focus-within:ring-[color:var(--border-accent)]",
              state === value
                ? "border-[color:var(--border-accent)] bg-surface-3 text-fg"
                : "border-[color:var(--border-subtle)] text-fg-muted hover:text-fg",
            )}
          >
            <input
              type="radio"
              name="preview-state"
              value={value}
              className="sr-only"
              checked={state === value}
              onChange={() => { setPicked(true); setState(value); }}
            />
            {STAGE_COPY[value].label}
          </label>
        ))}
      </div>
      <p className="shrink-0 text-xs text-fg-muted [display:var(--hint,block)]">{STAGE_COPY[state].caption}</p>
    </Section>
  );
}

export function AppearanceTab({ s, setTheme }: { s: EmberSettings; setTheme: (theme: Theme) => void }) {
  return (
    <div data-tab-body="" className="settings-two-col min-h-0 flex-1">
      <div data-settings-col="" className="settings-col">
        <Section
          title="Theme"
          titleId="theme-heading"
          hint="For this window only."
          detail={
            <p>
              Only the settings window changes. Motion everywhere follows the system's
              reduced-motion setting.
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

      <div data-settings-col="" className="settings-col settings-col-grow settings-col-wide">
        <OverlayStage />
      </div>
    </div>
  );
}
