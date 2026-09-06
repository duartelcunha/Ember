import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
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
import { ipc, type EmberSettings, type RefineMode, type ThinkingLevel } from "@/lib/ipc";

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
};

/** O mesmo texto refinado pelos tres modos, para a diferenca se VER em vez de se ler. E um
 *  exemplo escrito a mao, nao um refine ao vivo, e a UI diz isso: mostrar uma amostra colada
 *  como se fosse output real seria uma promessa que nao podemos garantir. */
const MODE_EXAMPLE = {
  input: "set up meeting tomorrow with john",
  outputs: {
    polish: "Set up a meeting tomorrow with John.",
    adaptive: "Schedule a meeting with John for tomorrow and confirm the time with him.",
    turbo:
      [
        "You are my scheduling assistant.",
        "Goal: Schedule a meeting with John.",
        "When: Tomorrow, time to be confirmed.",
        "Output: Ready-to-send invite and note.",
      ].join("\n"),
  } as Record<RefineMode, string>,
};

/** Mostra o exemplo do modo escolhido, antes e depois. */
function ModeExample({ mode }: { mode: RefineMode }) {
  return (
    <div className="rounded-sm border border-[color:var(--border-subtle)] bg-surface-2 p-3">
      <p className="text-[10px] uppercase tracking-wide text-fg-muted">Example</p>
      <p className="mt-1.5 font-mono text-xs text-fg-muted line-through decoration-1">
        {MODE_EXAMPLE.input}
      </p>
      <p className="mt-1.5 whitespace-pre-line font-mono text-xs text-fg">
        {MODE_EXAMPLE.outputs[mode]}
      </p>
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
  setThinking,
}: {
  s: EmberSettings;
  setS: React.Dispatch<React.SetStateAction<EmberSettings>>;
  setMode: (mode: RefineMode) => void;
  setThinking: (enabled: boolean, level: ThinkingLevel) => void;
}) {
  /** Optimistic toggle with rollback, the pattern every switch here follows. */
  const toggle = (key: "terminalHandling" | "selectAllFallback" | "previewBeforePaste", call: (v: boolean) => Promise<unknown>) =>
    (v: boolean) => {
      setS({ ...s, [key]: v });
      call(v).catch(() => setS((prev) => ({ ...prev, [key]: !v })));
    };

  return (
    <div data-tab-body="" className="grid min-h-0 flex-1 grid-cols-1 items-start gap-[var(--card-gap,1rem)] @xl/settings:grid-cols-2">
      <Section
        title="Refine mode"
        titleId="refine-mode-heading"
        hint={MODE_COPY[s.mode].hint}
        detail={
          <p>
            This is what your main shortcut does. The example is written by hand to show the
            difference between the three, not a live refine. Bind a shortcut to Fix or Rebuild
            under Shortcut to switch as you press.
          </p>
        }
      >
        <Select value={s.mode} onValueChange={(v) => setMode(v as RefineMode)}>
          <SelectTrigger aria-labelledby="refine-mode-heading">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {(Object.keys(MODE_COPY) as RefineMode[]).map((m) => (
              <SelectItem key={m} value={m}>
                {MODE_COPY[m].title}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {/* Inline when there is height for it; a popover button under compact density. */}
        <div className="[display:var(--example,block)]">
          <ModeExample mode={s.mode} />
        </div>
        {/* The wrapper carries the density variable: the button's own `inline-flex` would win
            over a display set on the button itself. */}
        <div className="self-start [display:var(--example-button,none)]">
          <Popover>
            <PopoverTrigger asChild>
              <Button variant="ghost" size="sm">
                Example
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-80 p-1">
              <ModeExample mode={s.mode} />
            </PopoverContent>
          </Popover>
        </div>
      </Section>

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
  );
}
