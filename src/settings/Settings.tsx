import { useEffect, useState } from "react";
import { motion, AnimatePresence, MotionConfig } from "motion/react";
import { toast } from "sonner";
import {
  GearSix,
  Keyboard,
  Plugs,
  Sliders,
  Sparkle,
  UserCircleGear,
} from "@phosphor-icons/react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Logo } from "@/components/Logo";
import { UpdateChecker } from "./UpdateChecker";
import {
  DEFAULT_SETTINGS,
  ipc,
  type EmberSettings,
  type ProviderKind,
  type RefineMode,
  type ThinkingLevel,
} from "@/lib/ipc";

const GEMINI_PRESETS = ["gemini-3.5-flash", "gemini-2.5-flash", "gemini-2.5-flash-lite"];
const CLAUDE_PRESETS = ["claude-sonnet-4-6"];
const CUSTOM = "__custom__";

function Section({
  title,
  hint,
  children,
}: {
  title: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-lg border border-[color:var(--border-subtle)] bg-surface-1 p-5">
      <h3 className="text-sm font-semibold text-fg">{title}</h3>
      {hint && <p className="mt-1 text-xs text-fg-muted">{hint}</p>}
      <div className="mt-4 flex flex-col gap-4">{children}</div>
    </div>
  );
}

function ModelPicker({
  kind,
  presets,
  model,
  onCommit,
}: {
  kind: ProviderKind;
  presets: string[];
  model: string;
  onCommit: (model: string) => Promise<void>;
}) {
  const [picked, setPicked] = useState(presets.includes(model) ? model : CUSTOM);
  const [custom, setCustom] = useState(model);

  return (
    <div className="flex flex-col gap-2">
      <Label htmlFor={`${kind}-model`}>Model</Label>
      <Select
        value={picked}
        onValueChange={(v) => {
          setPicked(v);
          if (v !== CUSTOM) onCommit(v);
        }}
      >
        <SelectTrigger id={`${kind}-model`}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {presets.map((p) => (
            <SelectItem key={p} value={p}>
              {p}
            </SelectItem>
          ))}
          <SelectItem value={CUSTOM}>Custom…</SelectItem>
        </SelectContent>
      </Select>
      {picked === CUSTOM && (
        <Input
          value={custom}
          onChange={(e) => setCustom(e.target.value)}
          onBlur={() => custom.trim() && onCommit(custom.trim())}
          placeholder="exact model id"
        />
      )}
    </div>
  );
}

function ProviderConfig({
  kind,
  title,
  subtitle,
  hasKey,
  model,
  presets,
}: {
  kind: ProviderKind;
  title: string;
  subtitle: string;
  hasKey: boolean;
  model: string;
  presets: string[];
}) {
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(hasKey);

  const saveKey = async () => {
    if (!key.trim()) return;
    setBusy(true);
    try {
      await ipc.setApiKey(kind, key.trim());
      const ok = await ipc.validateKey(kind);
      setSaved(true);
      setKey("");
      toast[ok ? "success" : "error"](
        ok ? `${title} key is valid and saved.` : `${title} key saved, but validation failed.`,
      );
    } catch {
      toast.error("Couldn't save the key (app not running?).");
    } finally {
      setBusy(false);
    }
  };

  const commitModel = async (m: string) => {
    try {
      await ipc.setModel(kind, m);
      toast.success(`${title} model updated.`);
    } catch {
      /* outside Tauri */
    }
  };

  return (
    <Section title={title} hint={subtitle}>
      <div className="flex flex-col gap-2">
        <Label htmlFor={`${kind}-key`}>API key</Label>
        <div className="flex gap-2">
          <Input
            id={`${kind}-key`}
            type="password"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={saved ? "•••••••• (saved)" : "paste your key"}
          />
          <Button variant="primary" onClick={saveKey} disabled={busy || !key.trim()}>
            Save
          </Button>
        </div>
      </div>
      <ModelPicker kind={kind} presets={presets} model={model} onCommit={commitModel} />
    </Section>
  );
}

function NumberField({
  label,
  value,
  onChange,
  min,
  max,
}: {
  label: string;
  value: number;
  onChange: (n: number) => void;
  min: number;
  max: number;
}) {
  return (
    <div className="flex flex-col gap-2">
      <Label>{label}</Label>
      <Input
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </div>
  );
}

const MODE_COPY: Record<RefineMode, { title: string; hint: string }> = {
  adaptive: {
    title: "Adaptive",
    hint: "Scales to the input: short asks get polished, tasks get structured.",
  },
  polish: {
    title: "Polish",
    hint: "Only fixes grammar and clarity. Keeps your structure and length.",
  },
  turbo: {
    title: "Turbo",
    hint: "Restructures as much as possible: role, context, requirements, format.",
  },
};

const THINKING_LEVELS: ThinkingLevel[] = ["minimal", "low", "medium", "high"];

export function Settings() {
  const [s, setS] = useState<EmberSettings>(DEFAULT_SETTINGS);
  const [profileText, setProfileText] = useState("");
  const [hotkey, setHotkey] = useState(DEFAULT_SETTINGS.hotkey);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [polls, setPolls] = useState(DEFAULT_SETTINGS.capturePolls);
  const [stepMs, setStepMs] = useState(DEFAULT_SETTINGS.captureStepMs);
  const [settleMs, setSettleMs] = useState(DEFAULT_SETTINGS.pasteSettleMs);

  useEffect(() => {
    ipc
      .getSettings()
      .then((res) => {
        setS(res);
        setProfileText(res.profileText);
        setHotkey(res.hotkey);
        setPolls(res.capturePolls);
        setStepMs(res.captureStepMs);
        setSettleMs(res.pasteSettleMs);
      })
      .catch(() => {
        /* outside Tauri: use defaults */
      });
  }, []);

  const sourceLabel: Record<EmberSettings["profileSource"], string> = {
    claude_md: "auto-detected from CLAUDE.md",
    user_edited: "edited by you",
    default: "built-in quality profile",
  };

  const setMode = (mode: RefineMode) => {
    setS({ ...s, mode });
    ipc
      .setMode(mode)
      .then(() => toast.success(`Refine mode: ${MODE_COPY[mode].title}.`))
      .catch(() => toast.error("Couldn't update the mode."));
  };

  const setThinking = (enabled: boolean, level: ThinkingLevel) => {
    setS({ ...s, thinkingEnabled: enabled, thinkingLevel: level });
    ipc
      .setThinking(enabled, level)
      .catch(() => toast.error("Couldn't update extended thinking."));
  };

  const saveTiming = () => {
    ipc
      .setCaptureTiming(polls, stepMs, settleMs)
      .then(() => toast.success("Capture timing saved."))
      .catch(() => toast.error("Couldn't save the timing."));
  };

  return (
    <MotionConfig reducedMotion="user">
      <main className="min-h-screen bg-panel text-fg">
        <motion.div
          className="mx-auto max-w-3xl px-8 py-12"
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.25, ease: "easeOut" }}
        >
          <header className="mb-10 flex items-center gap-3">
            <Logo size={34} />
            <div>
              <h1 className="text-2xl font-semibold tracking-tight">Ember</h1>
              <p className="text-sm text-fg-muted">
                Refine your prompts in the moment, in any app.
              </p>
            </div>
          </header>
  
          <Tabs defaultValue="providers">
            <TabsList>
              <TabsTrigger value="providers">
                <Plugs size={16} /> Providers
              </TabsTrigger>
              <TabsTrigger value="refining">
                <Sliders size={16} /> Refining
              </TabsTrigger>
              <TabsTrigger value="hotkey">
                <Keyboard size={16} /> Shortcut
              </TabsTrigger>
              <TabsTrigger value="profile">
                <UserCircleGear size={16} /> Profile
              </TabsTrigger>
              <TabsTrigger value="appearance">
                <GearSix size={16} /> Appearance
              </TabsTrigger>
              <TabsTrigger value="about">
                <Sparkle size={16} /> About
              </TabsTrigger>
            </TabsList>
  
            <TabsContent value="providers">
              <div className="flex flex-col gap-4">
                <p className="text-xs text-fg-muted">
                  BYOK: bring your own keys. Gemini is primary; Claude is the fallback (different
                  families fail for different reasons). Keys live in the Windows Credential Manager,
                  never in plain text.
                </p>
                <ProviderConfig
                  kind="gemini"
                  title="Gemini (primary)"
                  subtitle="Fast, with a generous free tier."
                  hasKey={s.hasGeminiKey}
                  model={s.geminiModel}
                  presets={GEMINI_PRESETS}
                />
                <ProviderConfig
                  kind="claude"
                  title="Claude (fallback)"
                  subtitle="Optional. Kicks in when Gemini fails, or for max quality."
                  hasKey={s.hasClaudeKey}
                  model={s.claudeModel}
                  presets={CLAUDE_PRESETS}
                />
              </div>
            </TabsContent>
  
            <TabsContent value="refining">
              <div className="flex flex-col gap-4">
                <Section title="Refine mode" hint={MODE_COPY[s.mode].hint}>
                  <Select value={s.mode} onValueChange={(v) => setMode(v as RefineMode)}>
                    <SelectTrigger>
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
                </Section>
  
                <Section
                  title="Extended thinking"
                  hint="Gemini reasons longer before answering. Higher quality, a bit slower."
                >
                  <div className="flex items-center justify-between">
                    <Label>Enable extended thinking</Label>
                    <Switch
                      checked={s.thinkingEnabled}
                      onCheckedChange={(v) => setThinking(v, s.thinkingLevel)}
                    />
                  </div>
                  {s.thinkingEnabled && (
                    <div className="flex flex-col gap-2">
                      <Label>Thinking level</Label>
                      <Select
                        value={s.thinkingLevel}
                        onValueChange={(v) => setThinking(true, v as ThinkingLevel)}
                      >
                        <SelectTrigger>
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
                    </div>
                  )}
                </Section>
  
                <Section
                  title="Terminals"
                  hint="Use Ctrl+Shift+C/V in terminal apps, since Ctrl+C sends an interrupt there."
                >
                  <div className="flex items-center justify-between">
                    <Label>Detect terminals automatically</Label>
                    <Switch
                      checked={s.terminalHandling}
                      onCheckedChange={(v) => {
                        setS({ ...s, terminalHandling: v });
                        ipc
                          .setTerminalHandling(v)
                          .catch(() => setS((prev) => ({ ...prev, terminalHandling: !v })));
                      }}
                    />
                  </div>
                </Section>
  
                <Section
                  title="Advanced"
                  hint="Capture timing, for power users. The defaults work for almost everyone."
                >
                  <Button
                    className="self-start"
                    variant="ghost"
                    size="sm"
                    onClick={() => setShowAdvanced((v) => !v)}
                  >
                    {showAdvanced ? "Hide" : "Show"} advanced
                  </Button>
                  <AnimatePresence initial={false}>
                    {showAdvanced && (
                      <motion.div
                        initial={{ opacity: 0, height: 0 }}
                        animate={{ opacity: 1, height: "auto" }}
                        exit={{ opacity: 0, height: 0 }}
                        transition={{ duration: 0.2, ease: "easeOut" }}
                        className="overflow-hidden"
                      >
                        <div className="grid grid-cols-3 gap-3 pt-1">
                          <NumberField
                            label="Capture polls"
                            value={polls}
                            onChange={setPolls}
                            min={5}
                            max={200}
                          />
                          <NumberField
                            label="Poll interval (ms)"
                            value={stepMs}
                            onChange={setStepMs}
                            min={1}
                            max={100}
                          />
                          <NumberField
                            label="Paste settle (ms)"
                            value={settleMs}
                            onChange={setSettleMs}
                            min={0}
                            max={1000}
                          />
                        </div>
                        <Button className="mt-3" variant="ghost" size="sm" onClick={saveTiming}>
                          Save timing
                        </Button>
                      </motion.div>
                    )}
                  </AnimatePresence>
                </Section>
              </div>
            </TabsContent>
  
            <TabsContent value="hotkey">
              <Section title="Global shortcut" hint="The combo that summons Ember in any app.">
                <div className="flex gap-2">
                  <Input value={hotkey} onChange={(e) => setHotkey(e.target.value)} />
                  <Button
                    onClick={() =>
                      ipc
                        .setHotkey(hotkey)
                        .then(() => toast.success("Shortcut updated."))
                        .catch(() => toast.error("Couldn't apply the shortcut."))
                    }
                  >
                    Apply
                  </Button>
                </div>
              </Section>
              <div className="mt-4">
                <Section title="Startup" hint="Launch Ember automatically with Windows.">
                  <div className="flex items-center justify-between">
                    <Label>Start with Windows</Label>
                    <Switch
                      checked={s.autostart}
                      onCheckedChange={(v) => {
                        setS({ ...s, autostart: v });
                        ipc.setAutostart(v).catch(() => setS((prev) => ({ ...prev, autostart: !v })));
                      }}
                    />
                  </div>
                </Section>
              </div>
            </TabsContent>
  
            <TabsContent value="profile">
              <Section
                title="Personalization profile"
                hint={`Current source: ${sourceLabel[s.profileSource]}.`}
              >
                {s.profilePath && <p className="font-mono text-xs text-fg-muted">{s.profilePath}</p>}
                <Textarea
                  rows={12}
                  value={profileText}
                  onChange={(e) => setProfileText(e.target.value)}
                  placeholder="Your style and tone preferences (language, rules like 'no em-dashes'…)."
                />
                <div className="flex flex-wrap gap-2">
                  <Button
                    variant="primary"
                    onClick={() =>
                      ipc
                        .setProfile(profileText)
                        .then(() => toast.success("Profile saved."))
                        .catch(() => toast.error("Couldn't save."))
                    }
                  >
                    Save
                  </Button>
                  <Button
                    variant="ghost"
                    onClick={() =>
                      ipc
                        .reloadProfileFromClaudeMd()
                        .then((res) => {
                          setS(res);
                          setProfileText(res.profileText);
                          toast.success("Reloaded from CLAUDE.md.");
                        })
                        .catch(() => toast.error("Couldn't reload."))
                    }
                  >
                    Reload from CLAUDE.md
                  </Button>
                  <Button
                    variant="ghost"
                    onClick={() =>
                      ipc
                        .resetProfileToDefault()
                        .then((res) => {
                          setS(res);
                          setProfileText(res.profileText);
                          toast.success("Reset to default.");
                        })
                        .catch(() => toast.error("Couldn't reset."))
                    }
                  >
                    Reset to default
                  </Button>
                </div>
              </Section>
            </TabsContent>
  
            <TabsContent value="appearance">
              <Section
                title="Appearance"
                hint="Premium dark theme. Respects the system's reduced-motion setting."
              >
                <p className="text-sm text-fg-muted">
                  Ember uses a dark, glassy theme with orange as the accent. More theme options coming
                  later.
                </p>
              </Section>
            </TabsContent>
  
            <TabsContent value="about">
              <div className="flex flex-col gap-4">
                <Section title="Ember">
                  <p className="text-sm text-fg-muted">
                    In-the-moment prompt refiner for any app. Gemini primary + Claude fallback, guided
                    by your profile. Built with Tauri.
                  </p>
                </Section>
                <Section title="Updates" hint="Checks against the latest GitHub release, signed and verified.">
                  <UpdateChecker />
                </Section>
              </div>
            </TabsContent>
          </Tabs>
        </motion.div>
      </main>
    </MotionConfig>
  );
}
