import { useEffect, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
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
import { ipc, type EmberSettings, type Length, type RefineMode, type ThinkingLevel } from "@/lib/ipc";
import { cn } from "@/lib/utils";

// The visible names are VERBS, not adjectives. "Adaptive", "Polish" and "Turbo" described the
// internal behaviour and took three sentences to tell apart; "Fix", "Improve" and "Rebuild" say
// what comes out the other side. The ids stay adaptive/polish/turbo: they are the contract with
// Rust and with the config on disk, and renaming them would break everyone's saved settings.
//
// Ordered by how much they change, which is how people choose: touch little, touch what is
// needed, touch everything.
export const MODE_COPY: Record<RefineMode, { title: string; hint: string }> = {
  polish: {
    title: "Fix",
    hint: "Fixes spelling and wording. Same length, same shape.",
  },
  adaptive: {
    title: "Improve",
    hint: "Fixes it, and tidies the structure when the text needs it.",
  },
  turbo: {
    title: "Rebuild",
    hint: "Turns it into a full prompt: role, context, requirements, output format.",
  },
  reply: {
    title: "Reply",
    hint: "Answers the message instead of rewriting it.",
  },
};

/** Reading order: the three that rewrite your text, by how much they change it, then the one
 *  that does something else with it. */
const MODES: RefineMode[] = ["polish", "adaptive", "turbo", "reply"];

/**
 * One rough ask through the four modes, so the difference is SEEN rather than read.
 *
 * The input is a half-typed request in a chat composer, which is where Ember is most often
 * fired. It has to be a REQUEST, not a note to a colleague: Rebuild's job is to produce the
 * prompt an engineer would have written for it, and a message to a person rewritten as a prompt
 * comes out as nonsense with an invented role attached. That is exactly what the first pass
 * here shipped, and it is why the example is chosen against the mode rules rather than for
 * looking pretty.
 *
 * Each output follows the rule its mode actually sends to the model (`ember_core::prompt`):
 * Fix keeps the shape and the length, Improve surfaces the structure the request already
 * implies, Rebuild adds role, requirements and output format AND leaves a visible {placeholder}
 * where the input never supplied a detail, because inventing one is forbidden. Reply answers a
 * message instead, leading with the answer and matching its length.
 *
 * Written by hand, not live refines, and the card's (i) says so: a sample dressed as real
 * output would be a promise the model does not make.
 */
const MODE_EXAMPLE = {
  input:
    "need a script that reads our csv exports and flags the orders with no delivery date, something i can run every monday",
  replyInput:
    "Hi, are you free Thursday afternoon to walk us through the new export format? Half an hour should be enough.",
  outputs: {
    polish:
      "I need a script that reads our CSV exports and flags the orders with no delivery date, something I can run every Monday.",
    adaptive:
      "I need a script that reads our CSV exports and flags every order with no delivery date. It should run unattended every Monday and list what it flagged, so I can act on it.",
    turbo: [
      "You are a data engineer writing a small, dependable maintenance script.",
      "Read every CSV export in {folder} and find the orders with no delivery date.",
      "Requirements: skip malformed rows and report how many were skipped; run unattended on a weekly schedule.",
      "Output: the script, then one line on how to schedule it.",
    ].join("\n"),
    reply: "Thursday afternoon works. {time} is best on my side, and half an hour is plenty.",
  } as Record<RefineMode, string>,
};

/**
 * The four modes as a compact list: the name and one line on what it does.
 *
 * The rows used to carry the examples as well, and to share the card's leftover height between
 * them: four short rows spread over a tall column, each floating in its own air. Choosing is a
 * one-line decision per row; the example belongs to the stage below, where the chosen mode
 * shows its work at a readable size and the card's height goes to something worth reading.
 *
 * Native radios in visually hidden inputs, not a role=radio grid: arrow-key roving focus, Space,
 * wrapping and a single tab stop all come for free and cannot drift. The checked styling is
 * driven from React state, not `:checked`, because a border colour alone disappears in the
 * cream theme's lower-contrast borders.
 */
function ModeList({ mode, onPick }: { mode: RefineMode; onPick: (mode: RefineMode) => void }) {
  return (
    <fieldset className="mode-list">
      <legend className="sr-only">Refine mode</legend>
      {MODES.map((m) => {
        const on = mode === m;
        return (
          <label
            key={m}
            data-checked={on ? "" : undefined}
            className={cn(
              "mode-option cursor-pointer transition-[color,background-color,box-shadow]",
              "focus-within:outline-none focus-within:ring-2 focus-within:ring-inset focus-within:ring-[color:var(--border-accent)]",
              on ? "bg-surface-2" : "hover:bg-surface-2/60",
            )}
          >
            <input
              type="radio"
              name="refine-mode"
              value={m}
              className="sr-only"
              checked={on}
              onChange={() => onPick(m)}
            />
            <span className={cn("truncate text-sm font-semibold", on ? "text-accent" : "text-fg")}>
              {MODE_COPY[m].title}
            </span>
            <span className="min-w-0 text-xs leading-snug text-fg-muted [display:var(--mode-hint,block)]">
              {MODE_COPY[m].hint}
            </span>
          </label>
        );
      })}
    </fieldset>
  );
}

/** Quick and settled: a change of mode swaps the text, it does not perform. */
const STAGE_TRANSITION = { duration: 0.16, ease: [0.16, 1, 0.3, 1] as const };

/**
 * The chosen mode at work: the message on the left, what comes back on the right (stacked when
 * the column is narrow). This is the card's elastic region. A tall window gives the two texts
 * room; a short one clamps them, because a mode is still recognisable from its first lines.
 *
 * The swap is the tab's one authored moment. The text that leaves blurs out and the new one
 * settles in, the way the refined text replaces the original under the cursor; the "before"
 * only moves when Reply is chosen, because Reply starts from a different message.
 */
function ModeStage({ mode }: { mode: RefineMode }) {
  const still = useReducedMotion();
  const before = mode === "reply" ? MODE_EXAMPLE.replyInput : MODE_EXAMPLE.input;
  const swap = still
    ? { initial: false as const, animate: {}, exit: {} }
    : {
        initial: { opacity: 0, y: 3, filter: "blur(3px)" },
        animate: { opacity: 1, y: 0, filter: "blur(0px)" },
        exit: { opacity: 0, y: -2, filter: "blur(3px)" },
      };
  return (
    // The stage is its own size container: the panes go side by side when the STAGE is wide
    // enough for two measures of text, whatever the window is doing (stacked at 720 wide the
    // stage is wider than it is beside the Behaviour card at 1000).
    <div data-scroll-pane="" className="mode-stage" aria-live="polite">
      <div className="mode-stage-grid">
        <div className="mode-stage-pane" data-before="">
          {/* Reply does not rewrite a draft of yours, so "Before" would be a lie there: what it
              starts from is a message somebody sent you. */}
          <span className="mode-stage-label">{mode === "reply" ? "Their message" : "Before"}</span>
          <AnimatePresence mode="wait" initial={false}>
            <motion.p key={before} className="mode-clamp" transition={STAGE_TRANSITION} {...swap}>
              {before}
            </motion.p>
          </AnimatePresence>
        </div>
        <div className="mode-stage-pane" data-after="">
          <span className="mode-stage-label">
            {mode === "reply" ? (
              "Your reply"
            ) : (
              <>
                After <span aria-hidden="true">·</span> {MODE_COPY[mode].title}
              </>
            )}
          </span>
          <AnimatePresence mode="wait" initial={false}>
            <motion.p
              key={mode}
              className="mode-example mode-clamp"
              transition={STAGE_TRANSITION}
              {...swap}
            >
              {MODE_EXAMPLE.outputs[mode]}
            </motion.p>
          </AnimatePresence>
        </div>
      </div>
    </div>
  );
}

const LENGTHS: { value: Length; label: string }[] = [
  { value: "shorter", label: "Shorter" },
  { value: "same", label: "Same" },
  { value: "longer", label: "Longer" },
];

/** Quick and settled: the thumb explains a move, it does not perform. */
const THUMB_SPRING = { type: "spring" as const, stiffness: 520, damping: 40 };

/**
 * Length, in the header of the modes card.
 *
 * In the header and not under the list for a measured reason: the stage is this tab's elastic
 * region, and any new line under it comes out of the stage's height. A control in the title
 * bar costs zero vertical pixels.
 *
 * Not a fourth mode: it applies on top of all four, Reply included. Hidden native radios, the
 * same pattern as the list and the theme segment, so arrows, Space and one tab stop come free.
 */
function LengthSegment({ value, onChange }: { value: Length; onChange: (length: Length) => void }) {
  const still = useReducedMotion();
  return (
    <div
      role="radiogroup"
      aria-label="Length"
      className="inline-flex shrink-0 rounded-full border border-[color:var(--border-subtle)] bg-surface-2 p-0.5"
    >
      {LENGTHS.map((l) => {
        const on = value === l.value;
        return (
          <label
            key={l.value}
            className={cn(
              "relative cursor-pointer rounded-full px-2.5 py-1 text-[11px] font-medium transition-colors",
              on ? "text-fg" : "text-fg-muted hover:text-fg",
            )}
          >
            {on && (
              <motion.span
                layoutId="length-thumb"
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
              name="refine-length"
              value={l.value}
              className="ember-seg-hit"
              aria-label={l.label}
              checked={on}
              onChange={() => onChange(l.value)}
            />
            <span className="relative z-10">{l.label}</span>
          </label>
        );
      })}
    </div>
  );
}

const THINKING_LEVELS: ThinkingLevel[] = ["minimal", "low", "medium", "high"];

function NumberField({
  id,
  label,
  value,
  onChange,
  min,
  max,
}: {
  id: string;
  label: string;
  value: number;
  onChange: (n: number) => void;
  min: number;
  max: number;
}) {
  return (
    <div className="flex flex-col gap-2">
      <Label htmlFor={id}>{label}</Label>
      <Input
        id={id}
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </div>
  );
}

/** Capture timing, for power users. It used to be an "Advanced" card that revealed three number
 *  fields in place; as a dialog it costs the tab one button. */
function CaptureTimingDialog({
  s,
  setS,
}: {
  s: EmberSettings;
  setS: React.Dispatch<React.SetStateAction<EmberSettings>>;
}) {
  const [open, setOpen] = useState(false);
  const [polls, setPolls] = useState(s.capturePolls);
  const [stepMs, setStepMs] = useState(s.captureStepMs);
  const [settleMs, setSettleMs] = useState(s.pasteSettleMs);
  const [saving, setSaving] = useState(false);
  // The saved values arrive from the async getSettings and change on save (the backend clamps).
  useEffect(() => {
    setPolls(s.capturePolls);
    setStepMs(s.captureStepMs);
    setSettleMs(s.pasteSettleMs);
  }, [s.capturePolls, s.captureStepMs, s.pasteSettleMs]);

  const saveTiming = () => {
    setSaving(true);
    ipc
      .setCaptureTiming(polls, stepMs, settleMs)
      .then((res) => {
        // The backend clamps the values; show what was actually saved (e.g. 500 -> 100), or the
        // UI would display a number outside the range, different from the one on disk.
        setS(res);
        toast.success("Capture timing saved.");
        setOpen(false);
      })
      .catch(() => toast.error("Couldn't save the timing."))
      .finally(() => setSaving(false));
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm">
          Capture timing…
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Capture timing</DialogTitle>
          <DialogDescription>
            How long Ember waits for the copy and the paste to land. The defaults work for
            almost everyone; raise them only if a slow app keeps missing the selection.
          </DialogDescription>
        </DialogHeader>
        <DialogBody>
          <div className="grid grid-cols-3 gap-3 pt-1">
            <NumberField id="capture-polls" label="Capture polls" value={polls} onChange={setPolls} min={5} max={200} />
            <NumberField id="capture-step-ms" label="Poll interval (ms)" value={stepMs} onChange={setStepMs} min={1} max={100} />
            <NumberField id="paste-settle-ms" label="Paste settle (ms)" value={settleMs} onChange={setSettleMs} min={0} max={1000} />
          </div>
        </DialogBody>
        <DialogFooter>
          <Button variant="ghost" onClick={() => setOpen(false)} disabled={saving}>
            Cancel
          </Button>
          <Button variant="primary" onClick={saveTiming} loading={saving}>
            Save timing
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export function RefiningTab({
  s,
  setS,
  setMode,
  setLength,
  setThinking,
}: {
  s: EmberSettings;
  setS: React.Dispatch<React.SetStateAction<EmberSettings>>;
  setMode: (mode: RefineMode) => void;
  setLength: (length: Length) => void;
  setThinking: (enabled: boolean, level: ThinkingLevel) => void;
}) {
  /** Optimistic toggle with rollback, the pattern every switch here follows. */
  const toggle = (key: "terminalHandling" | "selectAllFallback" | "previewBeforePaste", call: (v: boolean) => Promise<unknown>) =>
    (v: boolean) => {
      setS({ ...s, [key]: v });
      call(v).catch(() => setS((prev) => ({ ...prev, [key]: !v })));
    };

  return (
    <div data-tab-body="" className="settings-two-col min-h-0 flex-1">
      <div data-settings-col="" className="settings-col settings-col-grow settings-col-wide">
      <Section
        title="Refine mode"
        titleId="refine-mode-heading"
        elastic
        hint="What your shortcut does to the text you select."
        detail={
          <div className="space-y-2">
            <p>
              Pick one and the example below shows it at work: the same message through Fix,
              Improve and Rebuild, and a message received through Reply. The examples are written
              by hand to show the difference, not live refines. Length applies on top of whichever
              mode is running, Reply included. Bind a shortcut to a mode under Shortcut to switch
              as you press.
            </p>
            <p>
              Reply answers the message instead of rewriting it, in the first person and in the
              message&apos;s language, and it never accepts, declines or promises anything for
              you: where the message gave no detail it leaves a visible placeholder. Ember only
              writes into fields you can edit, so selecting a message in a reading pane will not
              reach the model. Paste it into your reply box, select it there, then fire the
              shortcut.
            </p>
          </div>
        }
        action={<LengthSegment value={s.length} onChange={setLength} />}
      >
        <ModeList mode={s.mode} onPick={setMode} />
        <ModeStage mode={s.mode} />
      </Section>
      </div>

      {/* The card runs the full height beside the modes, with the timing button held at its
          foot: a card that stopped two thirds of the way down, over nothing, read as a page that
          had not finished loading. The rows are in the order the refine meets them: the model
          first, then what happens at capture, then what happens at paste. */}
      <div data-settings-col="" className="settings-col refining-behaviour">
      <Section title="Behaviour" hint="What happens around each refine." elastic>
        <SwitchRow
          id="thinking-enabled"
          label="Extended thinking"
          hint="The model reasons longer before answering. Better on hard text, a little slower."
          checked={s.thinkingEnabled}
          onCheckedChange={(v) => setThinking(v, s.thinkingLevel)}
          extra={
            s.thinkingEnabled && (
              <Select value={s.thinkingLevel} onValueChange={(v) => setThinking(true, v as ThinkingLevel)}>
                <SelectTrigger id="thinking-level" aria-label="Thinking level" className="h-8 w-24 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {THINKING_LEVELS.map((lvl) => (
                    <SelectItem key={lvl} value={lvl}>
                      {lvl}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )
          }
        />
        <SwitchRow
          id="terminal-handling"
          label="Detect terminals"
          hint="Uses Ctrl+Shift+C/V in terminals, where Ctrl+C interrupts instead of copying."
          checked={s.terminalHandling}
          onCheckedChange={toggle("terminalHandling", ipc.setTerminalHandling)}
        />
        <SwitchRow
          id="select-all-fallback"
          label="Refine the whole field"
          hint="Fire the shortcut with nothing selected and Ember refines the whole field."
          detail={
            <p>
              This is what makes it work in a chat composer, where you typed a prompt but never
              highlighted it. If what it grabs looks like a whole page instead of a field, Ember
              stops and pastes nothing, and a refine that came in this way always asks you to
              confirm before replacing. Windows only for now.
            </p>
          }
          checked={s.selectAllFallback}
          onCheckedChange={toggle("selectAllFallback", ipc.setSelectAllFallback)}
        />
        <SwitchRow
          id="project-context"
          label="Choose a project automatically"
          hint="Picks a registered project from the focused file path."
          detail={
            <p>
              Uses a reviewed brief from your registered projects when the focused window
              identifies a file inside its folder. Window titles never authorise reading
              additional files. Use Projects to review the sources and the context used.
            </p>
          }
          checked={s.projectContext}
          onCheckedChange={(v) => {
            setS({ ...s, projectContext: v, activeProject: v ? null : s.activeProject });
            ipc
              .setProjectContext(v)
              .then(() => ipc.getSettings())
              .then(setS)
              .catch(() => setS((prev) => ({ ...prev, projectContext: !v })));
          }}
        />
        <SwitchRow
          id="preview-before-paste"
          label="Review before applying"
          hint="Read the refined result by your cursor before pressing Enter to apply."
          detail={
            <p>
              The refined result appears by your cursor. Page Up and Page Down let you read it
              all without changing focus. Enter applies it; Esc or your shortcut keeps the
              original. The result is visible on your screen during review. Windows only.
            </p>
          }
          checked={s.previewBeforePaste}
          onCheckedChange={toggle("previewBeforePaste", ipc.setPreviewBeforePaste)}
        />
        <div className="mt-auto flex justify-end pt-1">
          <CaptureTimingDialog s={s} setS={setS} />
        </div>
      </Section>
      </div>
    </div>
  );
}
