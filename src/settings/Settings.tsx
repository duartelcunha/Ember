import { useEffect, useState } from "react";
import { motion, MotionConfig } from "motion/react";
import { toast } from "sonner";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowSquareOut,
  Atom,
  GearSix,
  GithubLogo,
  Keyboard,
  Lightning,
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
import { BrandIcon } from "@/components/BrandIcon";
import { Logo } from "@/components/Logo";
import { TitleBar } from "@/components/TitleBar";
import { HotkeyCapture } from "./HotkeyCapture";
import { UpdateChecker } from "./UpdateChecker";
import {
  DEFAULT_SETTINGS,
  ipc,
  type EmberSettings,
  type ProviderHealth,
  type KeyConsole,
  type ProviderKind,
  type RefineMode,
  type Theme,
  type ThinkingLevel,
} from "@/lib/ipc";

// O `gemini-2.5-flash-lite` saiu daqui: a Google fechou-o a contas NOVAS ("no longer available to
// new users"), portanto qualquer utilizador novo que o escolhesse levava um 404. Continua a
// funcionar para quem ja o usava, por isso nao o migramos a forca; so deixamos de o oferecer.
//
// A quota gratuita do Gemini e POR MODELO (`GenerateRequestsPerDayPerProjectPerModel-FreeTier`),
// por isso trocar de modelo aqui da uma quota diaria nova. E a saida gratuita quando um deles
// esgota.
const GEMINI_PRESETS = ["gemini-2.5-flash", "gemini-3.1-flash-lite", "gemini-3.5-flash"];
const CLAUDE_PRESETS = ["claude-haiku-4-5", "claude-sonnet-4-6"];
const CUSTOM = "__custom__";

/**
 * Endpoints OpenAI-compatible conhecidos, para o provider de fallback. O utilizador escolhe um
 * (ou escreve a sua Base URL) e leva os modelos certos: um preset de modelos so faz sentido
 * COLADO ao endpoint que os serve (mandar um id do OpenRouter para o Groq da 404).
 *
 * O Groq esta primeiro por um motivo medido, nao por gosto: o tier gratuito do OpenRouter, sem
 * creditos comprados, da ~50 pedidos POR DIA aos modelos `:free`, e um utilizador que use o
 * Ember a serio queima isso numa tarde (aconteceu-nos em testes). O free tier do Groq da 14 400
 * pedidos por dia, sem cartao de credito. Para um fallback que tem de estar la quando o primario
 * cai, 288x mais folga nao e um detalhe.
 */
const OPENAI_ENDPOINTS = [
  {
    id: "groq",
    label: "Groq (free, best limits)",
    baseUrl: "https://api.groq.com/openai/v1",
    models: ["llama-3.3-70b-versatile", "llama-3.1-8b-instant", "openai/gpt-oss-120b"],
    note: "Free, no credit card, and about 14,000 requests a day. The most dependable free fallback.",
  },
  {
    id: "openai",
    label: "OpenAI (paid, no practical limits)",
    baseUrl: "https://api.openai.com/v1",
    models: ["gpt-4o-mini", "gpt-4.1-mini", "gpt-5-nano"],
    note: "Paid, but these small models cost a fraction of a cent per refine and never queue.",
  },
  {
    id: "openrouter",
    label: "OpenRouter (free models, low daily cap)",
    baseUrl: "https://openrouter.ai/api/v1",
    models: [
      "meta-llama/llama-3.3-70b-instruct:free",
      "google/gemma-4-31b-it:free",
      "qwen/qwen3-next-80b-a3b-instruct:free",
    ],
    note: "One key, many models. The free models are capped at roughly 50 requests a day, and they are shared with everyone, so they get busy. Topping up $10 of credit raises the cap about twentyfold and is not spent on free models.",
  },
] as const;

type EndpointId = (typeof OPENAI_ENDPOINTS)[number]["id"];

/** Que endpoint conhecido corresponde a esta Base URL? `undefined` = custom (DeepSeek, Ollama...). */
function endpointFor(baseUrl: string | undefined) {
  if (!baseUrl) return undefined;
  let host: string;
  try {
    host = new URL(baseUrl).host;
  } catch {
    return undefined;
  }
  return OPENAI_ENDPOINTS.find((e) => new URL(e.baseUrl).host === host);
}

/** Aplica o tema no <html> via data-theme. O CSS (globals.css) faz o resto: dark e o default
 *  (sem atributo ou "dark"); "cream" liga o bloco :root[data-theme="cream"]. */
function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
}

function Section({
  title,
  titleId,
  hint,
  action,
  children,
}: {
  title: string;
  /** Id opcional no titulo, para controlos sem Label proprio se associarem via aria-labelledby. */
  titleId?: string;
  hint?: string;
  /** Controlo opcional no canto superior direito do card (ex.: "Get a key" nos providers). */
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-lg border border-[color:var(--border-subtle)] bg-surface-1 p-5">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <h3 id={titleId} className="text-sm font-semibold text-fg">{title}</h3>
          {hint && <p className="mt-1 text-xs text-fg-muted">{hint}</p>}
        </div>
        {action && <div className="shrink-0">{action}</div>}
      </div>
      <div className="mt-4 flex flex-col gap-4">{children}</div>
    </div>
  );
}

/**
 * Consolas de chave: nome legivel + marca. Os URLs vivem no Rust (`open_key_console`).
 *
 * O Groq e a OpenAI nao tem logo oficial disponivel (o `simple-icons`, de onde vem os outros,
 * nao os inclui: o da OpenAI foi retirado a pedido deles, por marca registada). Em vez de
 * desenhar uma imitacao imprecisa, levam uma marca neutra na cor da casa: um raio para o Groq
 * (inferencia rapida e a identidade deles) e um atomo para a OpenAI.
 */
const KEY_CONSOLES: Record<KeyConsole, { label: string; icon: React.ReactNode }> = {
  gemini: { label: "Google AI Studio", icon: <BrandIcon brand="gemini" size={14} /> },
  groq: {
    label: "Groq Console",
    icon: <Lightning size={14} weight="fill" color="#F55036" aria-hidden="true" />,
  },
  openai: {
    label: "OpenAI Platform",
    icon: <Atom size={14} weight="fill" color="#10A37F" aria-hidden="true" />,
  },
  openrouter: { label: "OpenRouter", icon: <BrandIcon brand="openrouter" size={14} /> },
  claude: { label: "Anthropic Console", icon: <BrandIcon brand="claude" size={14} /> },
};

/** Botao que abre, no browser, a consola onde se cria a chave. Poupa ao utilizador ter de
 *  descobrir onde e (a queixa mais comum de qualquer app BYOK). */
function GetKeyButton({ console: target }: { console: KeyConsole }) {
  const { label, icon } = KEY_CONSOLES[target];
  return (
    <Button
      variant="ghost"
      size="sm"
      className="gap-1.5 text-xs"
      onClick={() =>
        ipc.openKeyConsole(target).catch(() => toast.error("Couldn't open your browser."))
      }
      title={`Opens ${label} in your browser`}
      aria-label={`Get an API key on ${label} (opens in your browser)`}
    >
      {icon}
      Get a key
      <ArrowSquareOut size={12} className="text-fg-muted" aria-hidden="true" />
    </Button>
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

  // O `model` real so chega depois do getSettings assincrono; o estado local foi inicializado
  // com o default. Ressincroniza quando o modelo guardado aterra, senao a UI mostrava sempre
  // o modelo por defeito em vez do escolhido pelo utilizador.
  useEffect(() => {
    setPicked(presets.includes(model) ? model : CUSTOM);
    setCustom(model);
  }, [model, presets]);

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
          aria-label={`Custom ${kind} model id`}
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
  baseUrl,
  onCommitBaseUrl,
  onKeyChanged,
}: {
  kind: ProviderKind;
  title: string;
  subtitle: string;
  hasKey: boolean;
  model: string;
  presets: string[];
  /** So o provider OpenAI-compatible mostra base URL (OpenRouter/DeepSeek/Groq/Ollama...). */
  baseUrl?: string;
  onCommitBaseUrl?: (url: string) => Promise<void>;
  /** Chamado apos gravar/remover chave, para o parent refazer a saude (Bug C). */
  onKeyChanged?: () => void;
}) {
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(hasKey);
  const [urlDraft, setUrlDraft] = useState(baseUrl ?? "");

  useEffect(() => setUrlDraft(baseUrl ?? ""), [baseUrl]);

  // `hasKey` chega do getSettings assincrono, depois do mount; sem ressincronizar, o
  // indicador de "chave guardada" ficava sempre a false mesmo com uma chave no cofre.
  useEffect(() => setSaved(hasKey), [hasKey]);

  const saveKey = async () => {
    if (!key.trim()) return;
    setBusy(true);
    try {
      await ipc.setApiKey(kind, key.trim());
      const status = await ipc.validateKey(kind);
      setSaved(true);
      setKey("");
      // "invalid" e "sem rede agora" sao coisas diferentes: uma chave boa nao deve parecer
      // recusada so porque a maquina estava offline no momento da validacao.
      if (status === "valid") {
        toast.success(`${title} key is valid and saved.`);
      } else if (status === "invalid") {
        toast.error(`${title} key saved, but looks invalid. Double-check it.`);
      } else {
        toast.error(`${title} key saved. Couldn't verify it right now (no network).`);
      }
      onKeyChanged?.();
    } catch {
      toast.error("Couldn't save the key (app not running?).");
    } finally {
      setBusy(false);
    }
  };

  const removeKey = async () => {
    setBusy(true);
    try {
      await ipc.clearApiKey(kind);
      setSaved(false);
      setKey("");
      toast.success(`${title} key removed.`);
      onKeyChanged?.();
    } catch {
      toast.error("Couldn't remove the key.");
    } finally {
      setBusy(false);
    }
  };

  const commitModel = async (m: string) => {
    try {
      await ipc.setModel(kind, m);
      toast.success(`${title} model updated.`);
    } catch {
      toast.error("Couldn't update the model.");
    }
  };

  // Trocar de servico e uma so accao para o utilizador, mas duas para o sistema: a Base URL e o
  // MODELO tem de mudar juntos. Um id do OpenRouter mandado ao Groq da 404; deixar o modelo do
  // servico antigo era garantir um erro no proximo refine.
  const endpoint = endpointFor(baseUrl);
  const switchEndpoint = async (id: EndpointId) => {
    const next = OPENAI_ENDPOINTS.find((e) => e.id === id);
    if (!next || !onCommitBaseUrl) return;
    try {
      await onCommitBaseUrl(next.baseUrl);
      await ipc.setModel("openai", next.models[0]);
      toast.success(`Fallback set to ${next.label.split(" (")[0]}.`);
      onKeyChanged?.(); // refaz a saude: a chave guardada e de OUTRO servico
    } catch {
      toast.error("Couldn't switch the service.");
    }
  };

  // A consola de chave do fallback depende do SERVICO escolhido, nao do provider: uma chave do
  // OpenRouter nao serve o Groq. Num endpoint custom (DeepSeek, Ollama) nao ha botao: nao
  // sabemos onde e, e mandar o utilizador ao sitio errado e pior do que nao o mandar a lado nenhum.
  const keyConsole: KeyConsole | undefined =
    kind === "openai" ? endpoint?.id : (kind as KeyConsole);

  return (
    <Section
      title={title}
      hint={subtitle}
      action={keyConsole ? <GetKeyButton console={keyConsole} /> : undefined}
    >
      {kind === "openai" && onCommitBaseUrl && (
        <div className="flex flex-col gap-2">
          <Label htmlFor="openai-endpoint">Service</Label>
          <Select value={endpoint?.id ?? CUSTOM} onValueChange={(v) => switchEndpoint(v as EndpointId)}>
            <SelectTrigger id="openai-endpoint">
              <SelectValue placeholder="Custom endpoint" />
            </SelectTrigger>
            <SelectContent>
              {OPENAI_ENDPOINTS.map((e) => (
                <SelectItem key={e.id} value={e.id}>
                  {e.label}
                </SelectItem>
              ))}
              {!endpoint && <SelectItem value={CUSTOM}>Custom (set the Base URL below)</SelectItem>}
            </SelectContent>
          </Select>
          {endpoint && <p className="text-xs text-fg-muted">{endpoint.note}</p>}
        </div>
      )}
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
          {saved && (
            <Button variant="ghost" onClick={removeKey} disabled={busy}>
              Remove
            </Button>
          )}
        </div>
      </div>
      {baseUrl !== undefined && onCommitBaseUrl && (
        <div className="flex flex-col gap-2">
          <Label htmlFor={`${kind}-base-url`}>Base URL</Label>
          <Input
            id={`${kind}-base-url`}
            value={urlDraft}
            onChange={(e) => setUrlDraft(e.target.value)}
            onBlur={() =>
              urlDraft.trim() &&
              urlDraft.trim() !== baseUrl &&
              onCommitBaseUrl(urlDraft.trim()).catch(() =>
                toast.error("Couldn't update the base URL.")
              )
            }
            placeholder="https://openrouter.ai/api/v1"
          />
        </div>
      )}
      <ModelPicker kind={kind} presets={presets} model={model} onCommit={commitModel} />
    </Section>
  );
}

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

/** Aviso honesto quando nao ha fallback pre-validado (regra de resiliencia). So aparece no caso
 *  estavel e nao-transitorio: exatamente um provider configurado (sem 2a familia). Dispensavel.
 *  Controlado por props: o parent (Settings) refaz o `health` sempre que uma chave muda, para o
 *  aviso nao ficar stale (Bug C: antes so buscava no mount). */
function ProviderHealthNotice({
  health,
  dismissed,
  onDismiss,
}: {
  health: ProviderHealth | null;
  dismissed: boolean;
  onDismiss: () => void;
}) {
  if (dismissed || !health || health.configuredCount !== 1) return null;
  return (
    <div className="flex items-start justify-between gap-3 rounded-lg border border-[color:var(--border-accent)] bg-surface-1 p-4 text-xs text-fg">
      <span>
        Only one provider is configured, so there's no fallback if it has an outage or hits a
        limit. Add a second key (a different family) for resilience.
      </span>
      <button className="shrink-0 text-fg-muted hover:text-fg" onClick={onDismiss}>
        Dismiss
      </button>
    </div>
  );
}

/** Diagnostico e modo debug: toggle, leitor de logs recentes, abrir a pasta, copiar report. */
function DiagnosticsSection({ debugMode }: { debugMode: boolean }) {
  const [on, setOn] = useState(debugMode);
  const [logs, setLogs] = useState("");
  const [loadingLogs, setLoadingLogs] = useState(false);

  // debugMode chega do getSettings assincrono; ressincroniza como os outros toggles.
  useEffect(() => setOn(debugMode), [debugMode]);

  const toggle = (v: boolean) => {
    setOn(v);
    ipc.setDebugMode(v).catch(() => {
      setOn(!v);
      toast.error("Couldn't change debug mode.");
    });
  };

  const refreshLogs = async () => {
    setLoadingLogs(true);
    try {
      setLogs(await ipc.readRecentLogs(200));
    } catch {
      toast.error("Couldn't read the logs.");
    } finally {
      setLoadingLogs(false);
    }
  };

  const copyDiagnostics = async () => {
    try {
      await navigator.clipboard.writeText(await ipc.getDiagnostics());
      toast.success("Diagnostics copied.");
    } catch {
      toast.error("Couldn't copy diagnostics.");
    }
  };

  return (
    <Section
      title="Diagnostics"
      hint="Debug mode opens the devtools and captures verbose logs. Logs live in a rotating file on your machine and never leave it."
    >
      <div className="flex items-center justify-between">
        <Label htmlFor="debug-mode">Debug mode</Label>
        <Switch id="debug-mode" checked={on} onCheckedChange={toggle} />
      </div>
      <div className="flex flex-wrap gap-2">
        <Button variant="ghost" size="sm" onClick={refreshLogs} disabled={loadingLogs}>
          {loadingLogs ? "Loading…" : "Load recent logs"}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() =>
            ipc.revealLogDir().catch(() => toast.error("Couldn't open the log folder."))
          }
        >
          Open log folder
        </Button>
        <Button variant="ghost" size="sm" onClick={copyDiagnostics}>
          Copy diagnostics
        </Button>
      </div>
      {logs && (
        <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-[color:var(--border-subtle)] bg-surface-1 p-3 font-mono text-[11px] leading-relaxed text-fg-muted">
          {logs}
        </pre>
      )}
    </Section>
  );
}

export function Settings() {
  // A janela e pintada escura pelo Rust (backgroundColor) e mostrada quando o componente monta,
  // por isso o fade-in de entrada corre no mount e ja se ve. As reaberturas re-animam via
  // `openKey` (remount do conteudo). O fecho esconde a janela no lado nativo (ver useEffect),
  // sem fade-out (fragil numa janela nativa), por isso nao ha estado de "invisivel" no JS.
  const [openKey, setOpenKey] = useState(0);
  const [s, setS] = useState<EmberSettings>(DEFAULT_SETTINGS);
  const [profileText, setProfileText] = useState("");
  const [hotkey, setHotkey] = useState(DEFAULT_SETTINGS.hotkey);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [polls, setPolls] = useState(DEFAULT_SETTINGS.capturePolls);
  const [stepMs, setStepMs] = useState(DEFAULT_SETTINGS.captureStepMs);
  const [settleMs, setSettleMs] = useState(DEFAULT_SETTINGS.pasteSettleMs);
  // Saude dos providers, ao nivel do Settings, para refazer quando uma chave muda (Bug C) e
  // passar ja resolvida ao aviso (que deixa de ter useEffect proprio).
  const [health, setHealth] = useState<ProviderHealth | null>(null);
  const [healthDismissed, setHealthDismissed] = useState(false);
  // Ate o getSettings assincrono voltar, `s` sao os defaults. Mostrar os tabs ja com defaults
  // pisca um estado falso (ex.: "sem chave" antes da chave real aterrar). Segura o conteudo ate
  // hidratar. Fica true tambem no catch (fora do Tauri: renderiza com defaults, sem ficar preso).
  const [hydrated, setHydrated] = useState(false);
  const refreshHealth = () =>
    ipc.getProviderHealth().then(setHealth).catch(() => {
      /* cofre ilegivel / fora do Tauri: o banner de key-store trata o caso grave */
    });

  useEffect(() => {
    // O fecho (X / Alt+F4) e tratado NATIVAMENTE no Rust (get_or_create_window): esconde a
    // janela, a app fica na tray. Nao ha handler de fecho no JS de proposito, o do webview era
    // fragil e deixava a janela presa a preto quando falhava.
    //
    // Reaberturas: a janela ja existe (so escondida), o Rust re-emite settings-opened. Incrementa
    // a openKey: a key nova remonta o conteudo, por isso o fade-in de entrada volta a correr do
    // zero a cada reabertura.
    const unlistenOpen = listen("settings-opened", () => {
      setOpenKey((k) => k + 1);
    });

    return () => {
      unlistenOpen.then((f) => f());
    };
  }, []);

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
        applyTheme(res.theme);
      })
      .catch(() => {
        /* outside Tauri: use defaults */
      })
      .finally(() => setHydrated(true));
    refreshHealth();
  }, []);

  const sourceLabel: Record<EmberSettings["profileSource"], string> = {
    claude_md: "auto-detected from your agent profile",
    user_edited: "edited by you",
    default: "built-in quality profile",
  };

  /** Escolhe um ficheiro de perfil e traz o texto para a textarea (nao grava sozinho: o
   *  utilizador revê e carrega em Save, que e o unico sitio onde o perfil muda de verdade). */
  const loadProfileFromFile = async () => {
    try {
      const picked = await open({
        multiple: false,
        directory: false,
        title: "Pick a profile file",
        filters: [{ name: "Markdown or text", extensions: ["md", "markdown", "txt"] }],
      });
      if (typeof picked !== "string") return; // cancelou
      const text = await ipc.readProfileFile(picked);
      if (!text.trim()) {
        toast.error("That file is empty.");
        return;
      }
      setProfileText(text);
      toast.success("Loaded. Review it, then hit Save.");
    } catch (e) {
      toast.error(typeof e === "string" ? e : "Couldn't read that file.");
    }
  };

  const setMode = (mode: RefineMode) => {
    const prev = s.mode;
    setS({ ...s, mode });
    ipc
      .setMode(mode)
      .then(() => toast.success(`Refine mode: ${MODE_COPY[mode].title}.`))
      .catch(() => {
        setS((cur) => ({ ...cur, mode: prev })); // reverte o otimismo se o backend falhou
        toast.error("Couldn't update the mode.");
      });
  };

  const setTheme = (theme: Theme) => {
    const prev = s.theme;
    setS({ ...s, theme });
    applyTheme(theme); // aplica ja (otimista); o data-theme troca as cores na hora
    ipc.setTheme(theme).catch(() => {
      setS((cur) => ({ ...cur, theme: prev }));
      applyTheme(prev);
      toast.error("Couldn't change the theme.");
    });
  };

  const setThinking = (enabled: boolean, level: ThinkingLevel) => {
    const prev = { enabled: s.thinkingEnabled, level: s.thinkingLevel };
    setS({ ...s, thinkingEnabled: enabled, thinkingLevel: level });
    ipc.setThinking(enabled, level).catch(() => {
      setS((cur) => ({ ...cur, thinkingEnabled: prev.enabled, thinkingLevel: prev.level }));
      toast.error("Couldn't update extended thinking.");
    });
  };

  const saveTiming = () => {
    ipc
      .setCaptureTiming(polls, stepMs, settleMs)
      .then((res) => {
        // O backend clampa os valores; reflete o que ficou mesmo gravado (ex: 500 -> 100),
        // senao a UI mostrava um numero fora da gama diferente do que esta em disco.
        setS(res);
        setPolls(res.capturePolls);
        setStepMs(res.captureStepMs);
        setSettleMs(res.pasteSettleMs);
        toast.success("Capture timing saved.");
      })
      .catch(() => toast.error("Couldn't save the timing."));
  };

  return (
    <MotionConfig reducedMotion="user">
          {/* Sem AnimatePresence/exit de proposito: a `key` (openKey) troca o conteudo num so
              commit (o antigo desmonta, o novo monta) e o novo corre initial->animate = fade-in
              limpo. Um exit-then-enter fazia o conteudo antigo SAIR primeiro (desaparecer) antes
              de o novo entrar, o "mostra, some, mostra" da reabertura. O fecho ja e nativo (Rust). */}
          <motion.main
            key={openKey}
            className="min-h-screen bg-panel text-fg"
            initial={{ opacity: 0, scale: 0.97 }}
            animate={{ opacity: 1, scale: 1 }}
            transition={{ duration: 0.6, ease: [0.22, 1, 0.36, 1] }}
            style={{ transformOrigin: "center" }}
          >
        <TitleBar />
        <motion.div
          className="mx-auto max-w-3xl px-8 pb-12 pt-10"
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.65, ease: [0.22, 1, 0.36, 1], delay: 0.12 }}
        >
          <header className="mb-10 flex items-center gap-3">
            <Logo size={34} />
            <div>
              <h1 className="text-2xl font-semibold tracking-tight">Ember</h1>
              <p className="text-sm text-fg-muted">
                Refine any text in the moment, in any app.
              </p>
            </div>
          </header>

          {!hydrated ? (
            // Esqueleto enquanto o getSettings nao voltou: evita piscar um estado falso (ex.:
            // "sem chave" antes da chave real aterrar). So opacidade anima (compositor-only).
            <div className="flex flex-col gap-4" aria-busy="true" aria-live="polite">
              <span className="sr-only">Loading settings</span>
              <div className="h-10 w-full animate-pulse rounded-lg bg-surface-1" />
              <div className="h-32 w-full animate-pulse rounded-lg bg-surface-1" />
              <div className="h-32 w-full animate-pulse rounded-lg bg-surface-1" />
            </div>
          ) : (
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
                {s.keyStoreError && (
                  <div className="rounded-lg border border-[color:var(--border-accent)] bg-surface-1 p-4 text-xs text-fg">
                    Ember couldn't read your saved keys (the credential vault may be locked).
                    Reopen the app or unlock the vault, then re-enter your keys.
                  </div>
                )}
                <ProviderHealthNotice
                  health={health}
                  dismissed={healthDismissed}
                  onDismiss={() => setHealthDismissed(true)}
                />
                <p className="text-xs text-fg-muted">
                  Bring your own keys. <strong className="text-fg">One key is enough to start</strong>{" "}
                  (Gemini is the fastest to set up and free). Ember tries them top to bottom, so a
                  second key is a backup for when the first is down or rate-limited. Keys live in
                  your OS credential vault, never in plain text.
                </p>
                <p className="text-xs text-fg-muted">
                  <strong className="text-fg">Hitting rate limits?</strong> Free tiers have daily
                  caps, and free models are shared with everyone, so they get busy at peak times.
                  That is normal, not a broken key. Either wait, or add a paid key (Claude Haiku
                  costs cents) as a third family that never queues.
                </p>
                <ProviderConfig
                  kind="gemini"
                  title="Gemini (primary)"
                  subtitle="Start here: free, fast, and the key takes a minute to create. The free tier has a daily cap that resets each day."
                  hasKey={s.hasGeminiKey}
                  model={s.geminiModel}
                  presets={GEMINI_PRESETS}
                  onKeyChanged={refreshHealth}
                />
                <ProviderConfig
                  kind="openai"
                  title="Fallback (Groq, OpenAI, OpenRouter…)"
                  subtitle="Used whenever Gemini fails or runs out of quota. Pick a service below and Ember sets the right models for it. Any OpenAI-compatible endpoint works, so you can also point it at DeepSeek or a local Ollama."
                  hasKey={s.hasOpenAiKey}
                  model={s.openaiModel}
                  // Os modelos vivem COLADOS ao servico: um id do OpenRouter no Groq da 404.
                  presets={[...(endpointFor(s.openaiBaseUrl)?.models ?? [])]}
                  baseUrl={s.openaiBaseUrl}
                  onKeyChanged={refreshHealth}
                  onCommitBaseUrl={async (url) => {
                    await ipc.setOpenAiBaseUrl(url);
                    // O backend sanitiza; rebusca para refletir o que ficou gravado e revalida a saude.
                    const res = await ipc.getSettings();
                    setS(res);
                    refreshHealth();
                    toast.success("Base URL updated.");
                  }}
                />
                <ProviderConfig
                  kind="claude"
                  title="Claude (optional third family)"
                  subtitle="Paid, but Haiku costs cents and never waits in a free-tier queue. Add this if you refine a lot and keep hitting limits. Tried last, only when the two above fail."
                  hasKey={s.hasClaudeKey}
                  model={s.claudeModel}
                  presets={CLAUDE_PRESETS}
                  onKeyChanged={refreshHealth}
                />
              </div>
            </TabsContent>
  
            <TabsContent value="refining">
              <div className="flex flex-col gap-4">
                <Section title="Refine mode" titleId="refine-mode-heading" hint={MODE_COPY[s.mode].hint}>
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
                </Section>
  
                <Section
                  title="Extended thinking"
                  hint="Gemini reasons longer before answering. Higher quality, a bit slower."
                >
                  <div className="flex items-center justify-between">
                    <Label htmlFor="thinking-enabled">Enable extended thinking</Label>
                    <Switch
                      id="thinking-enabled"
                      checked={s.thinkingEnabled}
                      onCheckedChange={(v) => setThinking(v, s.thinkingLevel)}
                    />
                  </div>
                  {s.thinkingEnabled && (
                    <div className="flex flex-col gap-2">
                      <Label htmlFor="thinking-level">Thinking level</Label>
                      <Select
                        value={s.thinkingLevel}
                        onValueChange={(v) => setThinking(true, v as ThinkingLevel)}
                      >
                        <SelectTrigger id="thinking-level">
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
                    <Label htmlFor="terminal-handling">Detect terminals automatically</Label>
                    <Switch
                      id="terminal-handling"
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
                  title="Project context"
                  hint="Detects the CLAUDE.md, AGENTS.md or GEMINI.md of the project in your focused window and merges it with your global profile. Off by default: turn it on only where you're OK sending a project's conventions to the LLM. Ember reads only those known files, redacts secret-shaped lines, and falls back to your global profile when no project is detected."
                >
                  <div className="flex items-center justify-between">
                    <Label htmlFor="project-context">Use the focused project's CLAUDE.md</Label>
                    <Switch
                      id="project-context"
                      checked={s.projectContext}
                      onCheckedChange={(v) => {
                        setS({ ...s, projectContext: v });
                        ipc
                          .setProjectContext(v)
                          .catch(() => setS((prev) => ({ ...prev, projectContext: !v })));
                      }}
                    />
                  </div>
                </Section>

                <Section
                  title="Preview before paste"
                  hint="After refining, show a small confirmation by your cursor and paste only when you press Enter. Press Esc (or your hotkey) to keep your original. Windows only."
                >
                  <div className="flex items-center justify-between">
                    <Label htmlFor="preview-before-paste">Confirm before pasting</Label>
                    <Switch
                      id="preview-before-paste"
                      checked={s.previewBeforePaste}
                      onCheckedChange={(v) => {
                        setS({ ...s, previewBeforePaste: v });
                        ipc
                          .setPreviewBeforePaste(v)
                          .catch(() => setS((prev) => ({ ...prev, previewBeforePaste: !v })));
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
                  {/* Reveal via grid-template-rows 0fr->1fr (sem reflow de irmaos, mais suave
                      que animar height:auto pelo JS). O interior faz min-h-0 + overflow-hidden. */}
                  <div
                    className="grid transition-[grid-template-rows] duration-[400ms] ease-[cubic-bezier(0.22,1,0.36,1)]"
                    style={{ gridTemplateRows: showAdvanced ? "1fr" : "0fr" }}
                  >
                    <div
                      className={`min-h-0 overflow-hidden transition-opacity duration-[400ms] ease-[cubic-bezier(0.22,1,0.36,1)] ${
                        showAdvanced ? "opacity-100" : "opacity-0"
                      }`}
                    >
                      <div className="grid grid-cols-3 gap-3 pt-1">
                        <NumberField
                          id="capture-polls"
                          label="Capture polls"
                          value={polls}
                          onChange={setPolls}
                          min={5}
                          max={200}
                        />
                        <NumberField
                          id="capture-step-ms"
                          label="Poll interval (ms)"
                          value={stepMs}
                          onChange={setStepMs}
                          min={1}
                          max={100}
                        />
                        <NumberField
                          id="paste-settle-ms"
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
                    </div>
                  </div>
                </Section>
              </div>
            </TabsContent>
  
            <TabsContent value="hotkey">
              <Section
                title="Global shortcut"
                titleId="hotkey-heading"
                hint="Click Set shortcut, then press the combo you want (e.g. Shift+Space). It's saved the moment you press it."
              >
                <HotkeyCapture
                  value={hotkey}
                  onCommit={async (accel) => {
                    try {
                      await ipc.setHotkey(accel);
                      setHotkey(accel);
                      toast.success(`Shortcut set to ${accel}.`);
                    } catch {
                      toast.error("Couldn't apply that shortcut (in use or invalid).");
                    }
                  }}
                />
              </Section>
              <div className="mt-4">
                <Section title="Startup" hint="Launch Ember automatically with Windows.">
                  <div className="flex items-center justify-between">
                    <Label htmlFor="autostart">Start with Windows</Label>
                    <Switch
                      id="autostart"
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
                titleId="profile-heading"
                hint={`Current source: ${sourceLabel[s.profileSource]}.`}
              >
                <p className="text-xs text-fg-muted">
                  This is how Ember learns to write like <em>you</em>: your tone, your rules, the
                  words you never use. It is added to every refine. Ember picks up the global
                  profile you already keep for your coding agent (
                  <code className="font-mono">CLAUDE.md</code>,{" "}
                  <code className="font-mono">AGENTS.md</code>, or{" "}
                  <code className="font-mono">GEMINI.md</code>), or you can load any markdown file
                  and edit it here.
                </p>
                {s.profilePath && <p className="font-mono text-xs text-fg-muted">{s.profilePath}</p>}
                <Textarea
                  aria-labelledby="profile-heading"
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
                        // Refetch para o hint "Current source" refletir que passou a
                        // "edited by you" em vez de continuar a mostrar a origem antiga.
                        .then(() => ipc.getSettings())
                        .then((res) => {
                          setS(res);
                          setProfileText(res.profileText);
                          toast.success("Profile saved.");
                        })
                        .catch(() => toast.error("Couldn't save."))
                    }
                  >
                    Save
                  </Button>
                  <Button variant="ghost" onClick={loadProfileFromFile}>
                    Load from file…
                  </Button>
                  <Button
                    variant="ghost"
                    onClick={() =>
                      ipc
                        .reloadProfileFromClaudeMd()
                        .then((res) => {
                          setS(res);
                          setProfileText(res.profileText);
                          toast.success("Reloaded the detected profile.");
                        })
                        .catch(() => toast.error("Couldn't reload."))
                    }
                  >
                    Re-detect
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
                title="Theme"
                titleId="theme-heading"
                hint="Applies to this Settings window. The cursor overlay keeps its glass look on any background. Respects the system's reduced-motion setting."
              >
                <Select value={s.theme} onValueChange={(v) => setTheme(v as Theme)}>
                  <SelectTrigger aria-labelledby="theme-heading">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="dark">Dark (glassy, orange accent)</SelectItem>
                    <SelectItem value="cream">Cream (warm light)</SelectItem>
                  </SelectContent>
                </Select>
              </Section>
            </TabsContent>
  
            <TabsContent value="about">
              <div className="flex flex-col gap-4">
                <Section title="Ember">
                  <p className="text-sm text-fg-muted">
                    In-the-moment text refiner for any app, prompts, emails, messages, docs and
                    more. Gemini primary, with an OpenAI-compatible fallback (OpenRouter by
                    default) and Claude as an optional third family, guided by your profile. Built
                    with Tauri.
                  </p>
                  <button
                    onClick={() =>
                      ipc.openRepo().catch(() => toast.error("Couldn't open the repository."))
                    }
                    className="inline-flex w-fit items-center gap-1.5 text-xs text-fg-muted transition-colors hover:text-fg"
                    aria-label="Open the Ember source repository on GitHub"
                  >
                    <GithubLogo size={15} weight="fill" />
                    Source on GitHub
                  </button>
                </Section>
                <Section title="Updates" hint="Checks against the latest GitHub release, signed and verified.">
                  <UpdateChecker />
                </Section>
                <DiagnosticsSection debugMode={s.debugMode} />
              </div>
            </TabsContent>
          </Tabs>
          )}
        </motion.div>
          </motion.main>
    </MotionConfig>
  );
}
