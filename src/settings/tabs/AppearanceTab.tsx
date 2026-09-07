import { useEffect, useState } from "react";
import { LazyMotion, domAnimation, motion, useReducedMotion } from "motion/react";
import { Section } from "../Section";
import { Orb } from "../../overlay/Orb";
import { Pill } from "../../overlay/Pill";
import { Preview } from "../../overlay/Preview";
import { CURSOR_GAP, orbInk } from "../../components/floatingGeometry";
import {
  ORB_SIZE_PX,
  type EmberSettings,
  type NoticeSpeed,
  type OrbSize,
  type OrbSkin,
  type Theme,
} from "@/lib/ipc";
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

/** The demo paces with the choice, so Notices is something you feel rather than read about. */
const CYCLE_MS: Record<NoticeSpeed, number> = { quick: 1700, normal: 2400, relaxed: 3600 };

/** Quick and settled: the thumb explains a move, it does not perform. */
const THUMB_SPRING = { type: "spring" as const, stiffness: 520, damping: 40 };

/**
 * A two or three way choice, as a segment.
 *
 * One component for the theme, the mark, the size and the notices: four segments written four
 * times is four chances for the keyboard behaviour to drift apart. Native radios in visually
 * hidden inputs, so arrow keys, Space and a single tab stop come for free, and the moving thumb
 * is a shared-layout element carrying the surface colour rather than the accent, because these
 * are settings and not calls to action.
 */
function Segment<T extends string>({
  name,
  label,
  value,
  options,
  onChange,
}: {
  name: string;
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  const still = useReducedMotion();
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="inline-flex shrink-0 rounded-full border border-[color:var(--border-subtle)] bg-surface-2 p-0.5"
    >
      {options.map((option) => {
        const on = value === option.value;
        return (
          <label
            key={option.value}
            className={cn(
              "relative cursor-pointer rounded-full px-2.5 py-1 text-[11px] font-medium transition-colors",
              on ? "text-fg" : "text-fg-muted hover:text-fg",
            )}
          >
            {on && (
              <motion.span
                layoutId={`${name}-thumb`}
                transition={still ? { duration: 0 } : THUMB_SPRING}
                aria-hidden="true"
                style={{
                  borderRadius: 9999,
                  boxShadow: "inset 0 0 0 1px var(--border-default), inset 0 1px 0 var(--sheen)",
                }}
                className="absolute inset-0 bg-surface-3"
              />
            )}
            <input
              type="radio"
              name={name}
              value={option.value}
              className="ember-seg-hit"
              aria-label={option.label}
              checked={on}
              onChange={() => onChange(option.value)}
            />
            <span className="relative z-10">{option.label}</span>
          </label>
        );
      })}
    </div>
  );
}

/** A label and its segment on one line. Three of these read as one control panel. */
function StyleRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex shrink-0 items-center justify-between gap-3">
      <span className="min-w-0 truncate text-xs text-fg-muted">{label}</span>
      {children}
    </div>
  );
}

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
 * Where the pointer stands in the stage. Left of the seam on purpose: the mark and the surface
 * grow rightwards from it, so the pair crosses from the light half into the dark one and makes
 * the card's own argument (the overlay does not follow your theme) without a word of prose.
 */
const CURSOR_X = "38%";

/**
 * The standard arrow, with its hotspot at the top-left of the box.
 *
 * The card is called "Next to your cursor" and there was no cursor: the mark floated in the
 * middle of the stage with nothing to be next to, which made every choice about its size and its
 * mark a choice about an object with no scale. Everything else on the stage is placed from this
 * point with the same constants the real overlay uses, so what the preview shows about the gap
 * is true.
 */
function Pointer({ x, y }: { x: string; y: string }) {
  return (
    <svg
      width={13}
      height={19}
      viewBox="0 0 13 19"
      className="absolute"
      style={{ left: x, top: y, filter: "drop-shadow(0 1px 2px rgba(0,0,0,0.45))" }}
      aria-hidden="true"
    >
      <path
        d="M0.5 0.9 L0.5 15.4 L4.3 11.9 L6.6 17.4 L8.9 16.4 L6.6 11 L11.3 10.7 Z"
        fill="#ffffff"
        stroke="#1c1a17"
        strokeWidth={1}
        strokeLinejoin="round"
      />
    </svg>
  );
}

const THEMES: { value: Theme; label: string }[] = [
  { value: "dark", label: "Dark" },
  { value: "cream", label: "Cream" },
];

const SKINS: { value: OrbSkin; label: string }[] = [
  { value: "ember", label: "Ember" },
  { value: "pulse", label: "Pulse" },
  { value: "ring", label: "Ring" },
];

const SIZES: { value: OrbSize; label: string }[] = [
  { value: "small", label: "Small" },
  { value: "normal", label: "Normal" },
  { value: "large", label: "Large" },
];

const SPEEDS: { value: NoticeSpeed; label: string }[] = [
  { value: "quick", label: "Quick" },
  { value: "normal", label: "Normal" },
  { value: "relaxed", label: "Relaxed" },
];

/**
 * What Ember puts next to your cursor, and the four choices about it, in one card.
 *
 * These are the real components from `src/overlay`, not a drawing of them, so the preview cannot
 * drift from the thing it depicts and the mark you pick is literally the mark you will get. Two
 * mechanics make that work:
 *
 * - `Orb` renders through `m.div`, whose features only load under a `LazyMotion` ancestor.
 *   Without one it mounts at opacity 0 and stays invisible for everyone who has not turned
 *   reduced motion on. Non-strict on purpose: `strict` throws for any `motion.*` descendant, and
 *   the settings tree uses plain `motion.*` in several places.
 * - The stage opts out of the settings theme (`.ember-overlay-preview` in globals.css). The
 *   overlay window never gets a `data-theme` attribute, so it is always dark; a preview that went
 *   cream in a cream window would be showing something the user will never see.
 */
function OverlayStage({
  s,
  onTheme,
  onStyle,
}: {
  s: EmberSettings;
  onTheme: (theme: Theme) => void;
  onStyle: (skin: OrbSkin, size: OrbSize, notice: NoticeSpeed) => void;
}) {
  const still = useReducedMotion();
  const [state, setState] = useState<StageState>("refining");
  // A pick is intent and it beats the demo: the loop stops for good, rather than moving the
  // thing the user just chose to look at.
  const [picked, setPicked] = useState(false);
  const [paused, setPaused] = useState(false);
  /** Where the pointer is over the stage, in stage pixels. `null` while it is somewhere else. */
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  const originX = at ? `${at.x}px` : CURSOR_X;
  const originY = at ? `${at.y}px` : "50%";
  // The real offsets, imported rather than copied: the mark's ink lands `CURSOR_GAP.x` to the
  // right of the hotspot and its centre `CURSOR_GAP.y` below it, and a surface centres on that
  // same point because the morph grows it out of the ring.
  const ink = orbInk(ORB_SIZE_PX[s.orbSize]);

  useEffect(() => {
    if (still || picked || paused) return;
    const timer = setInterval(
      () => setState((current) => CYCLE[(CYCLE.indexOf(current) + 1) % CYCLE.length]),
      CYCLE_MS[s.noticeSpeed],
    );
    return () => clearInterval(timer);
  }, [still, picked, paused, s.noticeSpeed]);

  return (
    <Section
      title="Next to your cursor"
      titleId="theme-heading"
      elastic
      hint="Always dark, so it reads over any app."
      detail={
        <p>
          The theme control changes only this window. The overlay never follows it: it appears over
          whatever application you were typing in, and a surface that changed with your settings
          would be unreadable over half of them. The mark and its size are what you get beside the
          cursor during a refine; Notices is how long the line after it stays on screen, and no
          choice there can make a message too short to read. Motion everywhere follows the
          system&apos;s reduced-motion setting.
        </p>
      }
      action={<Segment name="settings-theme" label="Theme" value={s.theme} options={THEMES} onChange={onTheme} />}
    >
      <LazyMotion features={domAnimation}>
        <div
          data-overlay-preview=""
          className="ember-overlay-preview relative min-h-[7rem] flex-auto overflow-hidden rounded-md"
          onPointerEnter={() => setPaused(true)}
          onPointerMove={(event) => {
            const box = event.currentTarget.getBoundingClientRect();
            setAt({ x: event.clientX - box.left, y: event.clientY - box.top });
          }}
          onPointerLeave={() => { setPaused(false); setAt(null); }}
        >
          <StageBackdrop />
          {/* A picture of a UI, not the UI: the caption below carries the meaning. */}
          <Pointer x={originX} y={originY} />
          {state === "refining" ? (
            <div
              aria-hidden="true"
              className="absolute"
              style={{
                left: `calc(${originX} + ${CURSOR_GAP.x - ink.x}px)`,
                top: `calc(${originY} + ${CURSOR_GAP.y - ink.y - ink.height / 2}px)`,
              }}
            >
              <Orb variant="work" skin={s.orbSkin} px={ORB_SIZE_PX[s.orbSize]} />
            </div>
          ) : (
            <div
              aria-hidden="true"
              className="absolute w-max"
              style={{
                left: `calc(${originX} + ${CURSOR_GAP.x}px)`,
                top: `calc(${originY} + ${CURSOR_GAP.y}px)`,
                transform: "translateY(-50%)",
                // The overlay clamps to the screen; here the stage is the screen.
                maxWidth: `calc(100% - ${originX} - ${CURSOR_GAP.x + 12}px)`,
              }}
            >
              {state === "confirm" && <Preview scope="selection" />}
              {state === "applied" && <Pill kind="success" text="Refined with Gemini" />}
              {state === "problem" && <Pill kind="error" text="Gemini hit its quota. Ember used the fallback." />}
            </div>
          )}
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
      <div className="flex shrink-0 flex-col gap-1.5 border-t border-[color:var(--border-subtle)] pt-3">
        <StyleRow label="Mark">
          <Segment
            name="orb-skin"
            label="Mark"
            value={s.orbSkin}
            options={SKINS}
            onChange={(skin) => { setPicked(true); setState("refining"); onStyle(skin, s.orbSize, s.noticeSpeed); }}
          />
        </StyleRow>
        <StyleRow label="Size">
          <Segment
            name="orb-size"
            label="Size"
            value={s.orbSize}
            options={SIZES}
            onChange={(size) => { setPicked(true); setState("refining"); onStyle(s.orbSkin, size, s.noticeSpeed); }}
          />
        </StyleRow>
        <StyleRow label="Notices stay">
          <Segment
            name="notice-speed"
            label="Notices stay"
            value={s.noticeSpeed}
            options={SPEEDS}
            onChange={(notice) => onStyle(s.orbSkin, s.orbSize, notice)}
          />
        </StyleRow>
      </div>
    </Section>
  );
}

export function AppearanceTab({
  s,
  setTheme,
  setOverlayStyle,
}: {
  s: EmberSettings;
  setTheme: (theme: Theme) => void;
  setOverlayStyle: (skin: OrbSkin, size: OrbSize, notice: NoticeSpeed) => void;
}) {
  return (
    <div data-tab-body="" className="settings-col min-h-0 flex-1">
      <OverlayStage s={s} onTheme={setTheme} onStyle={setOverlayStyle} />
    </div>
  );
}
