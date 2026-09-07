import { useEffect, useState } from "react";
import { motion, useReducedMotion } from "motion/react";
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

// Os nomes visiveis sao VERBOS, nao adjetivos. "Adaptive", "Polish" e "Turbo" descreviam o
// comportamento interno e obrigavam a ler tres frases para perceber a diferenca; "Fix", "Improve"
// e "Rebuild" dizem o que sai do outro lado. Os ids continuam adaptive/polish/turbo: sao contrato
// com o Rust e com a config em disco, e renomear isso partia as definicoes de quem ja tem a app.
//
// A ordem e por intensidade crescente, que e como as pessoas escolhem: mexe pouco, mexe o
// necessario, mexe tudo.
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

/** A ordem em que se leem: os tres que reescrevem o teu texto, por intensidade crescente, e
 *  depois o que faz outra coisa com ele. */
const MODES: RefineMode[] = ["polish", "adaptive", "turbo", "reply"];

/** O mesmo texto tratado por cada modo, para a diferenca se VER em vez de se ler. Sao exemplos
 *  escritos a mao, nao refines ao vivo, e a UI diz isso: mostrar uma amostra colada como se fosse
 *  output real seria uma promessa que nao podemos garantir.
 *
 *  O Reply parte de outro input (uma mensagem recebida, nao um rascunho teu), por isso a linha
 *  dele traz o seu proprio "antes"; os outros tres partilham o que esta no cabecalho. */
const MODE_EXAMPLE = {
  input: "set up meeting tomorrow with john",
  replyInput: "Hi, can you confirm the meeting time and who is joining?",
  outputs: {
    polish: "Set up a meeting tomorrow with John.",
    adaptive: "Schedule a meeting with John for tomorrow and confirm the time with him.",
    turbo:
      [
        "You are my scheduling assistant.",
        "Goal: schedule a meeting with John, time to confirm.",
        "Output: a ready-to-send invite.",
      ].join("\n"),
    reply: "The meeting is at {time}. {names} are joining.",
  } as Record<RefineMode, string>,
};

/**
 * The four modes, as one list you read down.
 *
 * They were bordered panels, each holding a bordered block for its example, inside a bordered
 * card: three frames deep before a word of text. Boxes inside boxes are out of this UI. A row per
 * mode on the card's own surface, hairlines between them, and the chosen one marked by a rule
 * down its left edge and by its title taking the accent.
 *
 * Reply is a row like the others now. Side by side it could not be, because it starts from a
 * different kind of input and the columns would have been comparing nothing; read down a list,
 * each row carries its own before and after and the question of what a mode does is answered
 * the same way for all four.
 *
 * Native radios in visually hidden inputs, not a role=radio grid: arrow-key roving focus, Space,
 * wrapping and a single tab stop all come for free and cannot drift. The ring sits on the label
 * through `focus-within` rather than `:has()`, which shipped in exactly the build-target
 * Chromium. The checked styling is driven from React state, not `:checked`, because a border
 * colour alone disappears in the cream theme's lower-contrast borders.
 */
function ModeComparison({ mode, onPick }: { mode: RefineMode; onPick: (mode: RefineMode) => void }) {
  return (
    // The safety valve, not the plan: the four rows fit at every size in the matrix. Scrolling a
    // little beats budgeting pixels that the next copy change would break.
    <fieldset data-scroll-pane="" className="mode-compare">
      <legend className="sr-only">Refine mode</legend>
      {MODES.map((m) => {
        const on = mode === m;
        return (
          <label
            key={m}
            className={cn(
              "mode-option cursor-pointer transition-[color,background-color,box-shadow]",
              "focus-within:outline-none focus-within:ring-2 focus-within:ring-inset focus-within:ring-[color:var(--border-accent)]",
              on
                ? "bg-surface-2 shadow-[inset_2px_0_0_var(--color-accent)]"
                : "hover:bg-surface-2/60",
            )}
          >
            <input
              type="radio"
              name="refine-mode"
              value={m}
              className="sr-only"
              checked={on}
              onChange={() => onPick(m)}
              aria-describedby={`mode-${m}-out`}
            />
            {/* The name alone. The chosen row already says so three times over: a rule down its
                left edge, its own ground, and the name in the accent. A tick beside that is a
                fourth way of saying the same thing. */}
            <span className={cn("truncate text-sm font-semibold", on ? "text-accent" : "text-fg")}>
              {MODE_COPY[m].title}
            </span>
            <span className="min-w-0">
              <span className="text-xs leading-snug text-fg-muted [display:var(--mode-hint,block)]">
                {MODE_COPY[m].hint}
              </span>
              {m === "reply" && (
                <span className="font-mono text-xs text-fg-muted line-through decoration-1 [display:var(--mode-input,block)]">
                  {MODE_EXAMPLE.replyInput}
                </span>
              )}
              <span
                id={`mode-${m}-out`}
                className="mode-example whitespace-pre-line font-mono text-xs text-fg"
                title={MODE_EXAMPLE.outputs[m]}
              >
                {MODE_EXAMPLE.outputs[m]}
              </span>
            </span>
          </label>
        );
      })}
    </fieldset>
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
 * O tamanho, no cabecalho do cartao dos modos.
 *
 * No cabecalho e nao por baixo da grelha por uma razao medida: a grelha e a regiao elastica
 * deste separador, e qualquer linha nova por baixo dela sai-lhe da altura. Um controlo na barra
 * do titulo custa zero pixeis verticais.
 *
 * Nao e um quarto modo nem uma quarta coluna: aplica-se aos quatro modos, incluindo o Reply.
 * Radios nativos escondidos, o mesmo padrao da comparacao e do segmento de tema, para as setas,
 * o Space e uma unica paragem de tab virem de graca.
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
              "focus-within:outline-none focus-within:ring-2 focus-within:ring-[color:var(--border-accent)]",
              on ? "text-fg" : "text-fg-muted hover:text-fg",
            )}
          >
            {on && (
              <motion.span
                layoutId="length-thumb"
                transition={still ? { duration: 0 } : THUMB_SPRING}
                aria-hidden="true"
                className="absolute inset-0 rounded-full border border-[color:var(--border-default)] bg-surface-3 shadow-[inset_0_1px_0_var(--sheen)]"
              />
            )}
            <input
              type="radio"
              name="refine-length"
              value={l.value}
              className="sr-only"
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
        // O backend clampa os valores; reflete o que ficou mesmo gravado (ex: 500 -> 100),
        // senao a UI mostrava um numero fora da gama diferente do que esta em disco.
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
        hint="What your main shortcut does. Pick one; the examples are written by hand."
        detail={
          <div className="space-y-2">
            <p>
              The examples are written by hand to show the difference, not live refines: the
              first three are one sentence through each mode, and Reply answers a message of its
              own. Length applies on top of whichever mode is
              running, Reply included. Bind a shortcut to a mode under Shortcut to switch as you
              press.
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
        <div className="flex min-h-0 flex-1 flex-col gap-1.5">
          <p className="shrink-0 text-xs text-fg-muted [display:var(--mode-input,block)]">
            <span className="mr-1.5 font-medium text-fg">Before</span>
            <span className="font-mono line-through decoration-1">{MODE_EXAMPLE.input}</span>
          </p>
          <ModeComparison mode={s.mode} onPick={setMode} />
        </div>
      </Section>
      </div>

      <div data-settings-col="" className="settings-col">
      <Section title="Behaviour" hint="What happens around each refine.">
        <SwitchRow
          id="thinking-enabled"
          label="Extended thinking"
          hint="Gemini reasons longer before answering. Higher quality, a bit slower."
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
          label="Confirm before applying"
          hint="A prompt by your cursor; Ember pastes only when you press Enter."
          detail={
            <p>
              After refining, a small prompt appears by your cursor and Ember pastes only when
              you press Enter. Esc, or your shortcut, keeps your original. Windows only.
            </p>
          }
          checked={s.previewBeforePaste}
          onCheckedChange={toggle("previewBeforePaste", ipc.setPreviewBeforePaste)}
        />
        <div className="flex justify-end">
          <CaptureTimingDialog s={s} setS={setS} />
        </div>
      </Section>
      </div>
    </div>
  );
}
