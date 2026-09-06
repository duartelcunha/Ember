import { useEffect, useState } from "react";
import { motion, useReducedMotion } from "motion/react";
import { toast } from "sonner";
import { ArrowSquareOut, ArrowUp, Atom, CaretDown, Lightning } from "@phosphor-icons/react";
import { Feedback } from "@/components/Feedback";
import { BrandIcon } from "@/components/BrandIcon";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { ProviderRow, Section } from "../Section";
import {
  ipc,
  type EmberSettings,
  type KeyConsole,
  type ModelCatalog,
  type OpenAiAuth,
  type ProviderHealth,
  type ProviderKind,
} from "@/lib/ipc";
import { cn } from "@/lib/utils";

// ESTAS LISTAS SAO SO O ARRANQUE A FRIO. Assim que houver uma chave validada, os modelos vem da
// listagem que o proprio provider publica (`ipc.listModels`, alimentada pelo mesmo `GET /models`
// que valida a chave), por isso um modelo descontinuado desaparece daqui sozinho e nao ha nada
// para vir apagar a mao. E o que se ve enquanto nao ha chave nenhuma, e mais nada.
//
// A quota gratuita do Gemini e POR MODELO (`GenerateRequestsPerDayPerProjectPerModel-FreeTier`),
// por isso trocar de modelo aqui da uma quota diaria nova. E a saida gratuita quando um deles
// esgota.
const GEMINI_PRESETS = ["gemini-2.5-flash", "gemini-3.1-flash-lite", "gemini-3.5-flash"];
const CUSTOM = "__custom__";

/** Movimento da troca de primário: rápido e sem baloiço, porque a animação está a explicar uma
 *  reordenação e não a chamar a atenção para si própria. */
const SWAP_SPRING = { type: "spring" as const, stiffness: 460, damping: 38 };

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
    label: "OpenRouter (free models, low cap)",
    baseUrl: "https://openrouter.ai/api/v1",
    models: [
      "meta-llama/llama-3.3-70b-instruct:free",
      "google/gemma-4-31b-it:free",
      "qwen/qwen3-next-80b-a3b-instruct:free",
    ],
    note: "One key, many models. Free models are capped near 50 requests a day and shared with everyone.",
  },
  {
    // A Anthropic entrou aqui quando o Claude deixou de ser um provider proprio. Fala o
    // protocolo OpenAI na mesma Base URL, por isso nao precisa de codigo nenhum a parte.
    id: "anthropic",
    label: "Anthropic (paid, Claude models)",
    baseUrl: "https://api.anthropic.com/v1",
    models: ["claude-haiku-4-5", "claude-sonnet-4-6"],
    note: "Cents per refine and never queues. Goes through Anthropic's OpenAI-compatible endpoint.",
  },
] as const;

type EndpointId = (typeof OPENAI_ENDPOINTS)[number]["id"];

/**
 * Valor da dropdown de serviço para o modo subscrição. Não está em `OPENAI_ENDPOINTS` porque não
 * tem Base URL nenhuma: fala com outro backend, e escolhê-lo não muda um endereço, muda a forma
 * de autenticar.
 */
const CHATGPT = "chatgpt-subscription";

/** Que endpoint conhecido corresponde a esta Base URL? `undefined` = custom (DeepSeek, Ollama...). */
export function endpointFor(baseUrl: string | undefined) {
  if (!baseUrl) return undefined;
  let host: string;
  try {
    host = new URL(baseUrl).host;
  } catch {
    return undefined;
  }
  return OPENAI_ENDPOINTS.find((e) => new URL(e.baseUrl).host === host);
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
  anthropic: { label: "Anthropic Console", icon: <BrandIcon brand="claude" size={14} /> },
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

/** Puts this provider first in the try order. Replaces the arrow that used to float beside the
 *  card: that arrow needed 64px of gutter outside the cards, which a two-column layout does not
 *  have. A labelled button also reads on its own; the arrow only explained itself on hover. */
function TryFirstButton({ kind, onClick }: { kind: ProviderKind; onClick: () => void }) {
  return (
    <Button
      variant="ghost"
      size="sm"
      className="gap-1.5 text-xs"
      onClick={onClick}
      title={
        kind === "gemini"
          ? "Try Gemini first for every refine; the other service becomes the fallback"
          : "Try this service first for every refine; Gemini becomes the fallback"
      }
    >
      <ArrowUp size={12} weight="bold" aria-hidden="true" />
      Try first
    </Button>
  );
}

function PrimaryPill() {
  return (
    <span className="shrink-0 whitespace-nowrap rounded-full border border-[color:var(--border-accent)] px-2 py-1 text-[10px] font-semibold uppercase leading-none tracking-wider text-accent">
      Primary
    </span>
  );
}

function ModelPicker({
  kind,
  presets,
  catalog,
  model,
  auto,
  onSetAuto,
  onCommit,
}: {
  kind: ProviderKind;
  presets: string[];
  /** Listagem viva do provider. `null` = ainda nao houve descoberta. */
  catalog?: ModelCatalog | null;
  model: string;
  /** O Ember e que escolhe este modelo? Quando `true`, nao ha dropdown nenhum: mostra-se o que
   *  ficou escolhido e um botao para quem quiser mesmo mexer. Ninguem devia ter de perceber de
   *  ids de modelos para a app funcionar bem, e a escolha certa muda a cada geracao nova. */
  auto?: boolean;
  onSetAuto?: (enabled: boolean) => void;
  onCommit: (model: string) => Promise<void>;
}) {
  const [picked, setPicked] = useState(presets.includes(model) ? model : CUSTOM);
  const [custom, setCustom] = useState(model);
  const live = catalog?.live ? catalog : null;

  // O `model` real so chega depois do getSettings assincrono; o estado local foi inicializado
  // com o default. Ressincroniza quando o modelo guardado aterra, senao a UI mostrava sempre
  // o modelo por defeito em vez do escolhido pelo utilizador.
  useEffect(() => {
    setPicked(presets.includes(model) ? model : CUSTOM);
    setCustom(model);
  }, [model, presets]);

  if (auto) {
    return (
      <ProviderRow
        label="Model"
        hint="Chosen for you: the best free model this provider serves. It follows new generations on its own."
      >
        <div className="flex h-9 min-w-0 flex-1 items-center truncate rounded-sm border border-[color:var(--border-subtle)] bg-surface-2 px-3 font-mono text-sm text-fg">
          {model}
        </div>
        <Button variant="ghost" onClick={() => onSetAuto?.(false)}>
          Change
        </Button>
      </ProviderRow>
    );
  }

  return (
    <div className="flex flex-col gap-1.5">
      <ProviderRow
        label="Model"
        htmlFor={`${kind}-model`}
        hint={
          /* Diz de onde vem a lista. Servir a lista embutida sem o dizer faria uma lista velha
             passar por atual, que e exatamente o problema que a descoberta veio resolver. */
          live
            ? `Live list from the provider${
                live.fetchedAtMs
                  ? `, read at ${new Date(live.fetchedAtMs).toLocaleTimeString()}`
                  : ""
              }. Discontinued models disappear on their own.`
            : "Built-in list. Add and validate a key to load the models this provider serves today."
        }
      >
        <Select
          value={picked}
          onValueChange={(v) => {
            setPicked(v);
            if (v !== CUSTOM) onCommit(v);
          }}
        >
          <SelectTrigger id={`${kind}-model`} className="flex-1">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {presets.map((p) => {
              const info = live?.models.find((m) => m.id === p);
              return (
                <SelectItem key={p} value={p}>
                  {p}
                  {info?.freeTier && " · free"}
                  {info?.preview && " · preview"}
                </SelectItem>
              );
            })}
            <SelectItem value={CUSTOM}>Custom…</SelectItem>
          </SelectContent>
        </Select>
      </ProviderRow>
      {picked === CUSTOM && (
        <Input
          aria-label={`Custom ${kind} model id`}
          className="ml-[104px] w-[calc(100%-104px)]"
          value={custom}
          onChange={(e) => setCustom(e.target.value)}
          onBlur={() => custom.trim() && onCommit(custom.trim())}
          placeholder="exact model id"
        />
      )}
      {onSetAuto && (
        <button
          type="button"
          onClick={() => onSetAuto(true)}
          className="self-start text-xs text-fg-muted underline underline-offset-2 hover:text-fg"
        >
          Let Ember choose again
        </button>
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
  catalog,
  auto,
  onSetAuto,
  baseUrl,
  onCommitBaseUrl,
  onKeyChanged,
  auth,
  signedIn,
  account,
  onSettings,
  isPrimary,
  onMakePrimary,
}: {
  kind: ProviderKind;
  title: string;
  subtitle: string;
  hasKey: boolean;
  model: string;
  presets: string[];
  catalog?: ModelCatalog | null;
  auto?: boolean;
  onSetAuto?: (enabled: boolean) => void;
  /** So o provider OpenAI-compatible mostra base URL (OpenRouter/DeepSeek/Groq/Ollama...). */
  baseUrl?: string;
  onCommitBaseUrl?: (url: string) => Promise<void>;
  /** Chamado apos gravar/remover chave, para o parent refazer a saude (Bug C). */
  onKeyChanged?: () => void;
  /** Só o slot de fallback: como se autentica hoje. */
  auth?: OpenAiAuth;
  signedIn?: boolean;
  account?: string | null;
  /** Recebe as settings devolvidas por um comando que as altera, para o parent as adotar. */
  onSettings?: (s: EmberSettings) => void;
  /** Este é o provider tentado primeiro? O outro é o fallback. */
  isPrimary?: boolean;
  onMakePrimary?: () => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const [key, setKey] = useState("");
  const [busyAction, setBusyAction] = useState<"save" | "remove" | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(hasKey);
  const [urlDraft, setUrlDraft] = useState(baseUrl ?? "");
  const subscription = auth === "chat_gpt";

  useEffect(() => setUrlDraft(baseUrl ?? ""), [baseUrl]);

  // `hasKey` chega do getSettings assincrono, depois do mount; sem ressincronizar, o
  // indicador de "chave guardada" ficava sempre a false mesmo com uma chave no cofre.
  useEffect(() => setSaved(hasKey), [hasKey]);

  const saveKey = async () => {
    setBusyAction("save");
    if (!key.trim()) return;
    setError(null);
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
        setError(`${title} key saved, but looks invalid. Double-check it.`);
      } else {
        setError(`${title} key saved. Couldn't verify it right now (no network).`);
      }
      onKeyChanged?.();
    } catch {
      setError("Couldn't save the key (app not running?).");
    } finally {
      setBusy(false);
      setBusyAction(null);
    }
  };

  const removeKey = async () => {
    setBusyAction("remove");
    setError(null);
    setBusy(true);
    try {
      await ipc.clearApiKey(kind);
      setSaved(false);
      setKey("");
      toast.success(`${title} key removed.`);
      onKeyChanged?.();
    } catch {
      setError("Couldn't remove the key.");
    } finally {
      setBusy(false);
      setBusyAction(null);
    }
  };

  const commitModel = async (m: string) => {
    try {
      await ipc.setModel(kind, m);
      toast.success(`${title} model updated.`);
    } catch {
      setError("Couldn't update the model.");
    }
  };

  // Trocar de servico e uma so accao para o utilizador, mas duas para o sistema: a Base URL e o
  // MODELO tem de mudar juntos. Um id do OpenRouter mandado ao Groq da 404; deixar o modelo do
  // servico antigo era garantir um erro no proximo refine.
  const endpoint = endpointFor(baseUrl);

  const signIn = async () => {
    setError(null);
    setBusy(true);
    try {
      // Só resolve depois do browser: o toast de sucesso é sobre a sessão gravada, e não sobre
      // ter aberto uma página. Prometer antes de saber seria mentir-lhe.
      onSettings?.(await ipc.chatgptLogin());
      toast.success("Signed in with your ChatGPT account.");
      onKeyChanged?.();
    } catch (e) {
      // A mensagem vem do Rust já legível (login cancelado, portas ocupadas, OpenAI recusou).
      setError(String(e));
    } finally {
      setBusy(false);
      setBusyAction(null);
    }
  };

  const signOut = async () => {
    setError(null);
    setBusy(true);
    try {
      onSettings?.(await ipc.chatgptLogout());
      toast.success("Signed out. The fallback is back to using an API key.");
      onKeyChanged?.();
    } catch {
      setError("Couldn't sign out.");
    } finally {
      setBusy(false);
      setBusyAction(null);
    }
  };

  const switchEndpoint = async (id: EndpointId | typeof CHATGPT) => {
    if (id === CHATGPT) {
      setError(null);
      setBusy(true);
      try {
        onSettings?.(await ipc.setOpenAiAuth("chat_gpt"));
        onKeyChanged?.();
      } catch {
        setError("Couldn't switch to the subscription.");
      } finally {
        setBusy(false);
        setBusyAction(null);
      }
      return;
    }
    const next = OPENAI_ENDPOINTS.find((e) => e.id === id);
    if (!next || !onCommitBaseUrl) return;
    try {
      // Sair do modo subscrição antes de mexer no endpoint: os `gpt-5.x` não existem em endpoint
      // nenhum destes, e mudar só a Base URL deixava o slot a falar com o backend errado.
      if (subscription) onSettings?.(await ipc.setOpenAiAuth("api_key"));
      await onCommitBaseUrl(next.baseUrl);
      await ipc.setModel("openai", next.models[0]);
      toast.success(`Fallback set to ${next.label.split(" (")[0]}.`);
      // Refaz o estado DEPOIS do setModel. O `onCommitBaseUrl` ja refez, mas nessa altura o
      // modelo ainda era o do servico anterior, por isso a UI continuava a mostrar um modelo
      // que ja nao estava em disco: Service = OpenAI com um modelo do Groq por baixo.
      onKeyChanged?.();
    } catch {
      setError("Couldn't switch the service.");
    }
  };

  // A consola de chave do fallback depende do SERVICO escolhido, nao do provider: uma chave do
  // OpenRouter nao serve o Groq. Num endpoint custom (DeepSeek, Ollama) nao ha botao: nao
  // sabemos onde e, e mandar o utilizador ao sitio errado e pior do que nao o mandar a lado nenhum.
  const keyConsole: KeyConsole | undefined =
    kind === "openai" ? (subscription ? undefined : endpoint?.id) : (kind as KeyConsole);

  const actions =
    keyConsole || (onMakePrimary && !isPrimary) ? (
      <div className="flex flex-wrap items-center justify-end gap-1.5">
        {onMakePrimary && !isPrimary && <TryFirstButton kind={kind} onClick={onMakePrimary} />}
        {keyConsole && <GetKeyButton console={keyConsole} />}
      </div>
    ) : undefined;

  return (
    <Section title={title} badge={isPrimary ? <PrimaryPill /> : undefined} hint={subtitle} action={actions}>
      {kind === "openai" && onCommitBaseUrl && (
        <ProviderRow
          label="Service"
          htmlFor="openai-endpoint"
          hint={
            subscription ? (
              // A ressalva vem ANTES de ele depender disto, e não depois de deixar de funcionar.
              <>
                Runs on the ChatGPT plan you already pay for, through the same unofficial route as
                the Codex CLI. OpenAI can cut it off without notice; if that happens, pick any
                service above.
              </>
            ) : (
              endpoint?.note
            )
          }
        >
          <Select
            value={subscription ? CHATGPT : (endpoint?.id ?? CUSTOM)}
            onValueChange={(v) => switchEndpoint(v as EndpointId | typeof CHATGPT)}
          >
            <SelectTrigger id="openai-endpoint" className="flex-1">
              <SelectValue placeholder="Custom endpoint" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={CHATGPT}>ChatGPT subscription (no API key)</SelectItem>
              {OPENAI_ENDPOINTS.map((e) => (
                <SelectItem key={e.id} value={e.id}>
                  {e.label}
                </SelectItem>
              ))}
              {!endpoint && !subscription && (
                <SelectItem value={CUSTOM}>Custom (set the Base URL below)</SelectItem>
              )}
            </SelectContent>
          </Select>
        </ProviderRow>
      )}
      {subscription ? (
        <ProviderRow label="Account">
          <>
            {signedIn ? (
              <>
                <p className="min-w-0 flex-1 truncate text-sm text-fg-muted">
                  {account ? `Signed in as ${account}.` : "Signed in."}
                </p>
                <Button variant="ghost" onClick={signOut} loading={busy}>
                  Sign out
                </Button>
              </>
            ) : (
              <>
                <p className="min-w-0 flex-1 truncate text-sm text-fg-muted">
                  Opens your browser to sign in. No key to paste.
                </p>
                <Button variant="primary" onClick={signIn} loading={busy}>
                  Sign in with ChatGPT
                </Button>
              </>
            )}
          </>
        </ProviderRow>
      ) : (
        <ProviderRow label="API key" htmlFor={`${kind}-key`}>
          <>
            <Input
              id={`${kind}-key`}
              type="password"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              placeholder={saved ? "•••••••• (saved)" : "paste your key"}
            />
            <Button variant="primary" onClick={saveKey} loading={busy && busyAction === "save"} disabled={busy || !key.trim()}>
              Save
            </Button>
            {saved && (
              <Button variant="ghost" onClick={removeKey} loading={busy && busyAction === "remove"} disabled={busy}>
                Remove
              </Button>
            )}
          </>
        </ProviderRow>
      )}
      {!subscription && baseUrl !== undefined && onCommitBaseUrl && (
        <ProviderRow label="Base URL" htmlFor={`${kind}-base-url`}>
          <Input
            id={`${kind}-base-url`}
            value={urlDraft}
            onChange={(e) => setUrlDraft(e.target.value)}
            onBlur={() =>
              urlDraft.trim() &&
              urlDraft.trim() !== baseUrl &&
              onCommitBaseUrl(urlDraft.trim()).catch(() =>
                setError("Couldn't update the base URL.")
              )
            }
            placeholder="https://openrouter.ai/api/v1"
          />
        </ProviderRow>
      )}
      <ModelPicker
        kind={kind}
        presets={presets}
        catalog={catalog}
        model={model}
        auto={auto}
        onSetAuto={onSetAuto}
        onCommit={commitModel}
      />
      {error && <Feedback tone="error">{error}</Feedback>}
    </Section>
  );
}

/** The collapsed form of a provider card, shown only when the container is too narrow for two
 *  cards side by side: one line with the essentials and a way to expand. Both cards stacked and
 *  expanded do not fit a 520px-tall window, and the settings never scroll. */
function ProviderSummary({
  title,
  status,
  model,
  isPrimary,
  kind,
  onExpand,
  onMakePrimary,
  className,
}: {
  title: string;
  status: string;
  model: string;
  isPrimary: boolean;
  kind: ProviderKind;
  onExpand: () => void;
  onMakePrimary?: () => void;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex h-12 items-center gap-3 rounded-lg border border-[color:var(--border-subtle)] bg-surface-1 pl-[var(--card-pad,1.25rem)] pr-3",
        className,
      )}
    >
      <button
        type="button"
        onClick={onExpand}
        aria-expanded={false}
        className="flex min-w-0 flex-1 items-center gap-2 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[color:var(--border-accent)]"
      >
        <span className="truncate text-sm font-semibold text-fg">{title}</span>
        {isPrimary && <PrimaryPill />}
        <span className="min-w-0 truncate text-xs text-fg-muted" title={`${status} · ${model}`}>
          {status} · <span className="font-mono">{model}</span>
        </span>
      </button>
      {!isPrimary && onMakePrimary && <TryFirstButton kind={kind} onClick={onMakePrimary} />}
      <Button variant="ghost" size="sm" onClick={onExpand} aria-label={`Expand ${title}`}>
        <CaretDown size={14} aria-hidden="true" />
      </Button>
    </div>
  );
}

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
    <NoticeRow
      text="Only one provider is configured, so there is no fallback if it has an outage or hits a limit. Add a second key from a different family."
      onDismiss={onDismiss}
    />
  );
}

/** One line, truncated, with the full text on hover. The old four-line boxes pushed the cards
 *  down; a notice earns one row, no more. */
function NoticeRow({ text, onDismiss }: { text: string; onDismiss?: () => void }) {
  return (
    <div className="flex h-8 shrink-0 items-center gap-3 rounded-md border border-[color:var(--border-accent)] bg-surface-1 px-3 text-xs text-fg">
      <span className="min-w-0 flex-1 truncate" title={text}>
        {text}
      </span>
      {onDismiss && (
        <button type="button" className="shrink-0 text-fg-muted hover:text-fg" onClick={onDismiss}>
          Dismiss
        </button>
      )}
    </div>
  );
}

export function ProvidersTab({
  s,
  setS,
  health,
  healthDismissed,
  onDismissHealth,
  catalogs,
  refreshHealth,
  refreshCatalogs,
  makePrimary,
}: {
  s: EmberSettings;
  setS: React.Dispatch<React.SetStateAction<EmberSettings>>;
  health: ProviderHealth | null;
  healthDismissed: boolean;
  onDismissHealth: () => void;
  catalogs: Partial<Record<ProviderKind, ModelCatalog>>;
  refreshHealth: () => void;
  refreshCatalogs: () => void;
  makePrimary: (kind: ProviderKind) => void;
}) {
  const still = useReducedMotion();
  const primary = s.primaryProvider;
  // Narrow containers show one card expanded and the other as a summary row. The primary is
  // expanded by default; promoting a card resets the override so the new primary opens.
  const [override, setOverride] = useState<ProviderKind | null>(null);
  useEffect(() => setOverride(null), [primary]);
  const expanded = override ?? primary;

  /** Ids a oferecer no select: a listagem viva quando existe, senao a lista embutida. Junta
   *  sempre o modelo GRAVADO, mesmo que nao esteja na listagem, para uma escolha antiga nao
   *  aparecer como "Custom..." so porque o provider parou de a anunciar. */
  const presetsFor = (kind: ProviderKind, builtIn: string[]): string[] => {
    const c = catalogs[kind];
    const base = c?.live && c.models.length ? c.models.map((m) => m.id) : builtIn;
    const saved = kind === "gemini" ? s.geminiModel : s.openaiModel;
    return saved && !base.includes(saved) ? [saved, ...base] : base;
  };

  const subscription = s.openaiAuth === "chat_gpt";
  const endpoint = endpointFor(s.openaiBaseUrl);
  const openaiTitle = subscription
    ? "ChatGPT subscription"
    : endpoint
      ? endpoint.label.split(" (")[0]
      : "Custom endpoint";

  const cards: Record<ProviderKind, { title: string; status: string; model: string; config: React.ReactNode }> = {
    gemini: {
      title: "Gemini",
      status: s.hasGeminiKey ? "Key saved" : "No key yet",
      model: s.geminiModel,
      config: (
        <ProviderConfig
          kind="gemini"
          isPrimary={primary === "gemini"}
          onMakePrimary={() => makePrimary("gemini")}
          title="Gemini"
          subtitle={
            primary === "gemini"
              ? "Free, fast, and the key takes a minute. Ember picks the model for you."
              : "Used when the primary fails or hits its quota. Free; Ember picks the model."
          }
          hasKey={s.hasGeminiKey}
          model={s.geminiModel}
          presets={presetsFor("gemini", GEMINI_PRESETS)}
          catalog={catalogs.gemini}
          auto={s.geminiModelAuto}
          onSetAuto={(enabled) => {
            // Otimista, e o backend devolve o estado ja resolvido (ligar o automatico
            // muda tambem o modelo, a partir da listagem em cache).
            setS({ ...s, geminiModelAuto: enabled });
            ipc
              .setGeminiModelAuto(enabled)
              .then(setS)
              .catch(() => setS((prev) => ({ ...prev, geminiModelAuto: !enabled })));
          }}
          onKeyChanged={() => {
            refreshHealth();
            refreshCatalogs();
          }}
        />
      ),
    },
    openai: {
      title: openaiTitle,
      status: subscription ? (s.chatgptSignedIn ? "Signed in" : "Not signed in") : s.hasOpenAiKey ? "Key saved" : "No key yet",
      model: s.openaiModel,
      config: (
        <ProviderConfig
          kind="openai"
          isPrimary={primary === "openai"}
          onMakePrimary={() => makePrimary("openai")}
          title={openaiTitle}
          subtitle={
            primary === "openai"
              ? "Tried first for every refine. Pick a service below."
              : "Used when the primary fails or hits its quota. Pick a service below."
          }
          hasKey={s.hasOpenAiKey}
          model={s.openaiModel}
          // Os modelos vivem COLADOS ao servico: um id do OpenRouter no Groq da 404. Na
          // subscricao nao ha Base URL nenhuma e os modelos sao outros (os `gpt-5.x` do
          // backend do ChatGPT), por isso a lista de arranque vem do catalogo, que ali
          // nunca e viva: aquele backend nao publica listagem de modelos.
          presets={presetsFor(
            "openai",
            subscription
              ? (catalogs.openai?.models.map((m) => m.id) ?? [])
              : [...(endpoint?.models ?? [])]
          )}
          catalog={catalogs.openai}
          baseUrl={s.openaiBaseUrl}
          auth={s.openaiAuth}
          signedIn={s.chatgptSignedIn}
          account={s.chatgptAccount}
          onSettings={setS}
          onKeyChanged={() => {
            ipc.getSettings().then(setS).catch(() => {});
            refreshHealth();
            refreshCatalogs();
          }}
          onCommitBaseUrl={async (url) => {
            await ipc.setOpenAiBaseUrl(url);
            // O backend sanitiza; rebusca para refletir o que ficou gravado e revalida a saude.
            const res = await ipc.getSettings();
            setS(res);
            refreshHealth();
            toast.success("Base URL updated.");
          }}
        />
      ),
    },
  };

  // Ordem visual = ordem real de tentativa. Mostrar o fallback por cima do primário seria
  // contar a história ao contrário no sítio onde ela se decide.
  const order: ProviderKind[] = primary === "gemini" ? ["gemini", "openai"] : ["openai", "gemini"];

  return (
    <div data-tab-body="" className="flex min-h-0 flex-1 flex-col gap-[var(--card-gap,1rem)]">
      {s.keyStoreError && (
        <NoticeRow text="Ember couldn't read your saved keys (the credential vault may be locked). Reopen the app or unlock the vault, then re-enter your keys." />
      )}
      <ProviderHealthNotice health={health} dismissed={healthDismissed} onDismiss={onDismissHealth} />
      {/* Os dois cartões existem sempre; só a ORDEM muda. Cada um vai dentro de um `motion.div`
          com `layout`, e é isso que faz o cartão promovido subir de facto em vez de a lista
          trocar de conteúdo num piscar de olhos: a animação mostra o que aconteceu, que é
          exatamente a informação que o utilizador precisa. `layoutDependency` limits the
          measurement to the swap; without it every window resize would animate the cards. */}
      <div className="grid grid-cols-1 items-start gap-[var(--card-gap,1rem)] @4xl/settings:grid-cols-2">
        {order.map((kind) => {
          const card = cards[kind];
          const isExpanded = expanded === kind;
          return (
            <motion.div
              key={kind}
              layout
              layoutDependency={primary}
              transition={still ? { duration: 0 } : SWAP_SPRING}
              className="min-w-0"
            >
              <div className={cn(!isExpanded && "hidden @4xl/settings:block")}>{card.config}</div>
              <ProviderSummary
                className={cn("@4xl/settings:hidden", isExpanded && "hidden")}
                title={card.title}
                status={card.status}
                model={card.model}
                isPrimary={primary === kind}
                kind={kind}
                onExpand={() => setOverride(kind)}
                onMakePrimary={() => makePrimary(kind)}
              />
            </motion.div>
          );
        })}
      </div>
    </div>
  );
}
