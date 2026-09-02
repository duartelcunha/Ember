import { useEffect, useState, useCallback } from "react";
import { motion, MotionConfig } from "motion/react";
import { toast } from "sonner";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowSquareOut,
  ArrowUp,
  Atom,
  Cube,
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
import { TitleBar } from "@/components/TitleBar";
import { HotkeyCapture } from "./HotkeyCapture";
import { ProjectsTab } from "./ProjectsTab";
import { UpdateChecker } from "./UpdateChecker";
import {
  DEFAULT_SETTINGS,
  ipc,
  type EmberSettings,
  type ProviderHealth,
  type KeyConsole,
  type OpenAiAuth,
  type ProviderKind,
  type RefineMode,
  type Theme,
  type ThinkingLevel,
  type ModelCatalog,
  type HotkeySlot,
} from "@/lib/ipc";

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

/** O aviso do macOS so faz sentido no macOS; no Windows seria ruido sobre um problema que la
 *  nao existe (o RegisterHotKey do Windows recusa mesmo os conflitos). */
const IS_MAC = /Mac|iPhone|iPad/.test(navigator.userAgent);

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
  detail,
  action,
  children,
}: {
  title: string;
  /** Id opcional no titulo, para controlos sem Label proprio se associarem via aria-labelledby. */
  titleId?: string;
  /** UMA linha. O que a pessoa precisa de ler para decidir o toggle. */
  hint?: string;
  /** O porque, as excecoes, os limites. Fica atras do (i) em vez de ocupar o ecra: quem esta a
   *  mexer numa definicao quer decidir depressa, e quem quer o detalhe sabe onde o encontrar. */
  detail?: React.ReactNode;
  /** Controlo opcional no canto superior direito do card (ex.: "Get a key" nos providers). */
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  const [showDetail, setShowDetail] = useState(false);
  return (
    <div className="rounded-lg border border-[color:var(--border-subtle)] bg-surface-1 p-5">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <h3 id={titleId} className="text-sm font-semibold text-fg">{title}</h3>
            {detail && (
              <button
                type="button"
                aria-label={`More about ${title}`}
                aria-expanded={showDetail}
                onClick={() => setShowDetail((v) => !v)}
                className={`flex h-4 w-4 shrink-0 items-center justify-center rounded-full border text-[10px] leading-none transition-colors ${
                  showDetail
                    ? "border-[color:var(--border-accent)] text-fg"
                    : "border-[color:var(--border-subtle)] text-fg-muted hover:text-fg"
                }`}
              >
                i
              </button>
            )}
          </div>
          {hint && <p className="mt-1 text-xs text-fg-muted">{hint}</p>}
        </div>
        {action && <div className="shrink-0">{action}</div>}
      </div>
      {detail && showDetail && (
        <div className="mt-3 border-l-2 border-[color:var(--border-subtle)] pl-3 text-xs text-fg-muted">
          {detail}
        </div>
      )}
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

/** Uma linha de um cartao de provider: etiqueta A ESQUERDA, controlo a ocupar o resto.
 *
 *  Estes cartoes tinham a etiqueta POR CIMA de cada campo, como o resto das settings. Aqui isso
 *  custava caro: sao quatro a cinco campos no mesmo cartao, e ha DOIS cartoes um a seguir ao
 *  outro, por isso o separador nao cabia na janela sem scroll. Lado a lado, cada campo passa de
 *  duas linhas para uma. So neste separador: onde ha um ou dois campos, a etiqueta por cima
 *  continua a ler-se melhor.
 *
 *  A coluna da etiqueta e fixa (92px) para os campos ficarem todos alinhados uns por baixo dos
 *  outros; a `hint` por baixo leva o mesmo avanco, senao lia-se como legenda da etiqueta em vez
 *  de legenda do campo. */
function ProviderRow({
  label,
  htmlFor,
  hint,
  children,
}: {
  label: string;
  htmlFor?: string;
  hint?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-3">
        <Label htmlFor={htmlFor} className="w-[92px] shrink-0">
          {label}
        </Label>
        <div className="flex min-w-0 flex-1 items-center gap-2">{children}</div>
      </div>
      {hint && <div className="pl-[104px] text-xs text-fg-muted">{hint}</div>}
    </div>
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
        <div className="flex h-9 flex-1 items-center rounded-sm border border-[color:var(--border-subtle)] bg-surface-2 px-3 font-mono text-sm text-fg">
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
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(hasKey);
  const [urlDraft, setUrlDraft] = useState(baseUrl ?? "");
  const subscription = auth === "chat_gpt";

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

  const signIn = async () => {
    setBusy(true);
    try {
      // Só resolve depois do browser: o toast de sucesso é sobre a sessão gravada, e não sobre
      // ter aberto uma página. Prometer antes de saber seria mentir-lhe.
      onSettings?.(await ipc.chatgptLogin());
      toast.success("Signed in with your ChatGPT account.");
      onKeyChanged?.();
    } catch (e) {
      // A mensagem vem do Rust já legível (login cancelado, portas ocupadas, OpenAI recusou).
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const signOut = async () => {
    setBusy(true);
    try {
      onSettings?.(await ipc.chatgptLogout());
      toast.success("Signed out. The fallback is back to using an API key.");
      onKeyChanged?.();
    } catch {
      toast.error("Couldn't sign out.");
    } finally {
      setBusy(false);
    }
  };

  const switchEndpoint = async (id: EndpointId | typeof CHATGPT) => {
    if (id === CHATGPT) {
      setBusy(true);
      try {
        onSettings?.(await ipc.setOpenAiAuth("chat_gpt"));
        onKeyChanged?.();
      } catch {
        toast.error("Couldn't switch to the subscription.");
      } finally {
        setBusy(false);
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
      toast.error("Couldn't switch the service.");
    }
  };

  // A consola de chave do fallback depende do SERVICO escolhido, nao do provider: uma chave do
  // OpenRouter nao serve o Groq. Num endpoint custom (DeepSeek, Ollama) nao ha botao: nao
  // sabemos onde e, e mandar o utilizador ao sitio errado e pior do que nao o mandar a lado nenhum.
  const keyConsole: KeyConsole | undefined =
    kind === "openai" ? (subscription ? undefined : endpoint?.id) : (kind as KeyConsole);

  return (
    <div className="relative">
    <Section
      title={title}
      hint={subtitle}
      action={keyConsole ? <GetKeyButton console={keyConsole} /> : undefined}
    >
      {kind === "openai" && onCommitBaseUrl && (
        <ProviderRow
          label="Service"
          htmlFor="openai-endpoint"
          hint={
            subscription ? (
              // A ressalva vem ANTES de ele depender disto, e não depois de deixar de funcionar.
              <>
                Refines come out of the ChatGPT plan you already pay for. Unofficial: it uses the
                same route as the Codex CLI and OpenAI can turn it off without notice. If that
                happens, pick any service above; those keep working.
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
                <p className="flex-1 text-sm text-fg-muted">
                  Signed in{account ? ` (account ${account})` : ""}.
                </p>
                <Button variant="ghost" onClick={signOut} disabled={busy}>
                  Sign out
                </Button>
              </>
            ) : (
              <>
                <p className="flex-1 text-sm text-fg-muted">
                  Opens your browser to sign in. No key to paste.
                </p>
                <Button variant="primary" onClick={signIn} disabled={busy}>
                  {busy ? "Waiting for the browser…" : "Sign in with ChatGPT"}
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
            <Button variant="primary" onClick={saveKey} disabled={busy || !key.trim()}>
              Save
            </Button>
            {saved && (
              <Button variant="ghost" onClick={removeKey} disabled={busy}>
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
                toast.error("Couldn't update the base URL.")
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
    </Section>
      {onMakePrimary && !isPrimary && (
        // AO LADO do cartao, e ABSOLUTA de proposito. Uma coluna normal a direita ja esteve aqui
        // e foi removida porque tirava 47px de largura AOS CARTOES DESTE SEPARADOR, que ficavam
        // mais estreitos do que os de todos os outros. Fora do fluxo nao rouba largura nenhuma:
        // os cartoes ficam com a largura toda E a seta fica na margem.
        //
        // O espaco dela vem do padding lateral da pagina (64px), e nao da sobra do centramento:
        // e a diferenca entre caber sempre e caber por sorte. Por isso a janela abre a 1000px e
        // nao a 920: sem esses 80px, alargar o padding tirava largura aos cartoes.
        //
        // Sem legenda por escolha do utilizador. O `title` e o `aria-label` carregam o significado
        // para quem passa o rato e para quem usa leitor de ecra.
        <button
          type="button"
          onClick={onMakePrimary}
          aria-label="Try this one first"
          title={
            kind === "gemini"
              ? "Try Gemini first for every refine; the other service becomes the fallback"
              : "Try this service first for every refine; Gemini becomes the fallback"
          }
          className="absolute -right-12 top-1/2 flex h-9 w-9 -translate-y-1/2 shrink-0 items-center justify-center rounded-md border border-[color:var(--border-subtle)] bg-surface-1 text-fg-muted transition-colors hover:border-[color:var(--border-accent)] hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[color:var(--border-accent)]"
        >
          <ArrowUp size={15} weight="bold" />
        </button>
      )}
    </div>
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

// Os nomes visiveis sao VERBOS, nao adjetivos. "Adaptive", "Polish" e "Turbo" descreviam o
// comportamento interno e obrigavam a ler tres frases para perceber a diferenca; "Fix", "Improve"
// e "Rebuild" dizem o que sai do outro lado. Os ids continuam adaptive/polish/turbo: sao contrato
// com o Rust e com a config em disco, e renomear isso partia as definicoes de quem ja tem a app.
//
// A ordem e por intensidade crescente, que e como as pessoas escolhem: mexe pouco, mexe o
// necessario, mexe tudo.
const MODE_COPY: Record<RefineMode, { title: string; hint: string }> = {
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
function DiagnosticsSection({
  debugMode,
  savePrompts,
  keepResults,
}: {
  debugMode: boolean;
  savePrompts: boolean;
  keepResults: boolean;
}) {
  const [on, setOn] = useState(debugMode);
  const [saving, setSaving] = useState(savePrompts);
  const [keeping, setKeeping] = useState(keepResults);
  const [logs, setLogs] = useState("");
  const [loadingLogs, setLoadingLogs] = useState(false);

  // debugMode chega do getSettings assincrono; ressincroniza como os outros toggles.
  useEffect(() => setOn(debugMode), [debugMode]);
  useEffect(() => setSaving(savePrompts), [savePrompts]);
  useEffect(() => setKeeping(keepResults), [keepResults]);

  const toggle = (v: boolean) => {
    setOn(v);
    ipc.setDebugMode(v).catch(() => {
      setOn(!v);
      toast.error("Couldn't change debug mode.");
    });
  };

  const togglePrompts = (v: boolean) => {
    setSaving(v);
    ipc.setSavePrompts(v).catch(() => {
      setSaving(!v);
      toast.error("Couldn't change prompt saving.");
    });
  };

  const toggleKeep = (v: boolean) => {
    setKeeping(v);
    ipc.setKeepResults(v).catch(() => {
      setKeeping(!v);
      toast.error("Couldn't change refine memory.");
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
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <Label htmlFor="save-prompts">Save prompts to a file</Label>
          <p className="mt-1 text-xs text-fg-muted">
            Writes what was sent to the model and what came back to{" "}
            <span className="font-mono">prompts.jsonl</span>, next to the logs. Off by default:
            unlike the log, this file contains the text you refined. Open the log folder below to
            read or delete it.
          </p>
        </div>
        <Switch
          id="save-prompts"
          checked={saving}
          onCheckedChange={togglePrompts}
          className="mt-1 shrink-0"
        />
      </div>
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <Label htmlFor="keep-results">Remember refines</Label>
          <p className="mt-1 text-xs text-fg-muted">
            Keeps recent refined results on this machine, so an interrupted refine is never lost
            and the same text is never paid for twice. Reapply the last one from the tray menu.
            Turning this off deletes what is stored.
          </p>
        </div>
        <Switch
          id="keep-results"
          checked={keeping}
          onCheckedChange={toggleKeep}
          className="mt-1 shrink-0"
        />
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

export function Settings({ initialTab = "providers" }: { initialTab?: string } = {}) {
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
  // Listagens de modelos por provider, descobertas em runtime. Nao bloqueiam nada: ate
  // chegarem, os selects mostram a lista embutida e dizem que e essa.
  const [catalogs, setCatalogs] = useState<Partial<Record<ProviderKind, ModelCatalog>>>({});
  const [healthDismissed, setHealthDismissed] = useState(false);
  // Ate o getSettings assincrono voltar, `s` sao os defaults. Mostrar os tabs ja com defaults
  // pisca um estado falso (ex.: "sem chave" antes da chave real aterrar). Segura o conteudo ate
  // hidratar. Fica true tambem no catch (fora do Tauri: renderiza com defaults, sem ficar preso).
  const [hydrated, setHydrated] = useState(false);
  const refreshHealth = () =>
    ipc.getProviderHealth().then(setHealth).catch(() => {
      /* cofre ilegivel / fora do Tauri: o banner de key-store trata o caso grave */
    });
  /** Rebusca as listagens. Best-effort: uma falha deixa o select como estava, sem toast, porque
   *  nao ha nada que o utilizador possa fazer e a lista embutida continua a servir. */
  const refreshCatalogs = () => {
    (["gemini", "openai"] as ProviderKind[]).forEach((kind) => {
      ipc
        .listModels(kind)
        .then((c) => setCatalogs((prev) => ({ ...prev, [kind]: c })))
        .catch(() => {});
    });
  };
  /** Grava um dos tres atalhos. Em caso de recusa mostra a mensagem do SO (que diz se a
   *  combinacao e invalida ou se ja esta ocupada por outra app), em vez de um erro generico
   *  que deixava o utilizador sem saber o que tentar a seguir. O Rust ja restaurou o conjunto
   *  anterior, por isso a app nunca fica sem atalho por causa de uma tentativa falhada. */
  const commitHotkey = async (
    which: HotkeySlot,
    accel: string,
  ): Promise<string | null> => {
    try {
      await ipc.setHotkey(which, accel);
      const res = await ipc.getSettings();
      setS(res);
      setHotkey(res.hotkey);
      toast.success(accel ? `Shortcut set to ${accel}.` : "Shortcut cleared.");
      return null;
    } catch (e) {
      // A mensagem volta para o alerta inline da HotkeyCapture (um canal de erro so); o toast
      // fica reservado ao sucesso. Um erro passageiro num canto nao ensina o que tentar a seguir.
      return `Couldn't apply that shortcut. ${String(e)}`;
    }
  };
  /** Ids a oferecer no select: a listagem viva quando existe, senao a lista embutida. Junta
   *  sempre o modelo GRAVADO, mesmo que nao esteja na listagem, para uma escolha antiga nao
   *  aparecer como "Custom..." so porque o provider parou de a anunciar. */
  /** Põe este provider à frente na ordem de tentativa. Só muda a ordem: chaves, sessão e modelos
   *  ficam onde estavam, e é por isso que voltar atrás é um clique no outro cartão. */
  const makePrimary = (kind: ProviderKind) => {
    ipc
      .setPrimaryProvider(kind)
      .then((next) => {
        setS(next);
        refreshHealth();
        toast.success(
          kind === "gemini"
            ? "Gemini is now tried first."
            : "The fallback service is now tried first."
        );
      })
      .catch(() => toast.error("Couldn't change which one goes first."));
  };

  const presetsFor = (kind: ProviderKind, builtIn: string[]): string[] => {
    const c = catalogs[kind];
    const base = c?.live && c.models.length ? c.models.map((m) => m.id) : builtIn;
    const saved = kind === "gemini" ? s.geminiModel : s.openaiModel;
    return saved && !base.includes(saved) ? [saved, ...base] : base;
  };

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
      loadSettings();
    });

    return () => {
      unlistenOpen.then((f) => f());
    };
  }, []);

  /** Traz o estado do Rust para o ecra. Corre na montagem E a cada reabertura da janela: como
   *  a janela e criada no ARRANQUE e depois so escondida/mostrada, sem isto ficava a mostrar o
   *  que era verdade quando a app abriu (o projeto ativo mudado pelo picker, um atalho limpo na
   *  sanitizacao da config, uma chave gravada noutro sitio). */
  const loadSettings = useCallback(() => {
    ipc
      .getSettings()
      .then((res) => {
        setS(res);
        setProfileText(res.profileText);
        setHotkey(res.hotkey);
        refreshCatalogs();
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

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
            // 280ms, e nao os 600 de antes: com a janela pre-aquecida no arranque, abrir e so
            // mostrar, e uma entrada longa passa a ser a UNICA coisa que se sente lenta. A
            // escala parte de 0.985 (era 0.97) para o texto nao chegar visivelmente a arrastar.
            initial={{ opacity: 0, scale: 0.985 }}
            animate={{ opacity: 1, scale: 1 }}
            transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }}
            style={{ transformOrigin: "center" }}
          >
        <TitleBar />
        <motion.div
          // pt-14 e nao pt-10: a TitleBar e `fixed` e tem 36px de altura, por isso com 40px de
          // topo os cartoes passavam a 4px dos botoes de minimizar e fechar e liam-se colados.
          //
          // px-16 (64px) e nao px-8: e deste padding que sai o espaco da seta de "tentar este
          // primeiro", que vive FORA dos cartoes. Antes ela usava a sobra do `mx-auto`, que numa
          // janela de 920px dava 4px de cada lado e numa janela estreita dava zero. O padding e
          // igual em qualquer largura; a sobra do centramento nao. O `max-w` sobe junto para os
          // cartoes nao ficarem mais estreitos do que ja eram.
          className="mx-auto max-w-5xl px-16 pb-12 pt-14"
          // Segue a de fora de perto (delay curto) em vez de somar mais meio segundo por cima:
          // as duas encadeadas davam ~800ms ate a janela assentar.
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.32, ease: [0.22, 1, 0.36, 1], delay: 0.06 }}
        >
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
          <Tabs defaultValue={initialTab}>
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
              <TabsTrigger value="projects">
                <Cube size={16} /> Projects
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
                {(() => {
                  // Os dois cartões existem sempre; só a ORDEM muda. Cada um vai dentro de um
                  // `motion.div` com `layout`, e é isso que faz o cartão promovido subir de facto
                  // em vez de a lista trocar de conteúdo num piscar de olhos: a animação mostra o
                  // que aconteceu, que é exatamente a informação que o utilizador precisa.
                  const gemini = (
                    <motion.div
                      key="gemini"
                      layout
                      transition={SWAP_SPRING}
                    >
                <ProviderConfig
                  kind="gemini"
                  isPrimary={s.primaryProvider === "gemini"}
                  onMakePrimary={() => makePrimary("gemini")}
                  title={s.primaryProvider === "gemini" ? "Gemini (primary)" : "Gemini (fallback)"}
                  subtitle={
                    s.primaryProvider === "gemini"
                      ? "Free, fast, and the key takes a minute. Ember picks the model for you."
                      : "Used whenever the primary fails or runs out of quota. Free, and Ember picks the model."
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
                    </motion.div>
                  );
                  const openai = (
                    <motion.div
                      key="openai"
                      layout
                      transition={SWAP_SPRING}
                    >
                <ProviderConfig
                  kind="openai"
                  isPrimary={s.primaryProvider === "openai"}
                  onMakePrimary={() => makePrimary("openai")}
                  title={s.primaryProvider === "openai" ? "Primary" : "Fallback"}
                  subtitle={
                    s.primaryProvider === "openai"
                      ? "Tried first for every refine. Pick a service below."
                      : "Used whenever the primary fails or runs out of quota. Pick a service below."
                  }
                  hasKey={s.hasOpenAiKey}
                  model={s.openaiModel}
                  // Os modelos vivem COLADOS ao servico: um id do OpenRouter no Groq da 404. Na
                  // subscricao nao ha Base URL nenhuma e os modelos sao outros (os `gpt-5.x` do
                  // backend do ChatGPT), por isso a lista de arranque vem do catalogo, que ali
                  // nunca e viva: aquele backend nao publica listagem de modelos.
                  presets={presetsFor(
                    "openai",
                    s.openaiAuth === "chat_gpt"
                      ? (catalogs.openai?.models.map((m) => m.id) ?? [])
                      : [...(endpointFor(s.openaiBaseUrl)?.models ?? [])]
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
                    </motion.div>
                  );
                  // Ordem visual = ordem real de tentativa. Mostrar o fallback por cima do
                  // primário seria contar a história ao contrário no sítio onde ela se decide.
                  return s.primaryProvider === "gemini" ? [gemini, openai] : [openai, gemini];
                })()}
              </div>
            </TabsContent>
  
            <TabsContent value="refining">
              <div className="flex flex-col gap-4">
                <Section
                  title="Refine mode"
                  titleId="refine-mode-heading"
                  hint={MODE_COPY[s.mode].hint}
                  detail={
                    <p>
                      This is what your main shortcut does. The example below is written by hand
                      to show the difference between the three, not a live refine. Bind a
                      shortcut to Fix or Rebuild under Shortcut to switch as you press.
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
                  <ModeExample mode={s.mode} />
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
                  hint="Use Ctrl+Shift+C/V in terminals, where Ctrl+C interrupts instead of copying."
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
                  title="Nothing selected"
                  hint="Fire the hotkey with nothing selected and Ember refines the whole field."
                  detail={
                    <p>
                      This is what makes it work in a chat composer, where you typed a prompt but
                      never highlighted it. If what it grabs looks like a whole page instead of a
                      field, Ember stops and pastes nothing, and a refine that came in this way
                      always asks you to confirm before replacing. Windows only for now.
                    </p>
                  }
                >
                  <div className="flex items-center justify-between">
                    <Label htmlFor="select-all-fallback">Refine the whole field</Label>
                    <Switch
                      id="select-all-fallback"
                      checked={s.selectAllFallback}
                      onCheckedChange={(v) => {
                        setS({ ...s, selectAllFallback: v });
                        ipc
                          .setSelectAllFallback(v)
                          .catch(() => setS((prev) => ({ ...prev, selectAllFallback: !v })));
                      }}
                    />
                  </div>
                </Section>

                <Section
                  title="Project context"
                  hint="Merges the focused project's CLAUDE.md into the refine."
                  detail={
                    <p>
                      Reads the CLAUDE.md, AGENTS.md or GEMINI.md of the project in your focused
                      window. Off by default: turn it on only where you are fine sending a
                      project's conventions to the LLM. Ember reads only those known files,
                      redacts secret-shaped lines, and falls back to your global profile when no
                      project is detected.
                    </p>
                  }
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
                  hint="Confirm by your cursor before anything is pasted."
                  detail={
                    <p>
                      After refining, a small prompt appears by your cursor and Ember pastes only
                      when you press Enter. Esc, or your shortcut, keeps your original. Windows
                      only.
                    </p>
                  }
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
                        On macOS some system shortcuts win over any app without reporting a
                        conflict. Ember knows the common ones and refuses them, but if a shortcut
                        saves and then never fires, that is what happened: pick another.
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
              <div className="mt-4">
                <Section
                  title="Shortcuts per mode"
                  hint="Optional. Fire one mode directly, without opening settings first."
                  detail={
                    <p>
                      Off until you set them. The main shortcut above keeps using the mode picked
                      in Refining; these two ignore it and always run their own. Leave one empty
                      and Ember does not claim that combo at all.
                    </p>
                  }
                >
                  <div className="flex flex-col gap-4">
                    <div className="flex flex-col gap-2">
                      <Label>Fix</Label>
                      <HotkeyCapture
                        value={s.hotkeyPolish}
                        slot="polish"
                        clearable
                        ariaLabel="Fix shortcut"
                        onCommit={(accel) => commitHotkey("polish", accel)}
                      />
                    </div>
                    <div className="flex flex-col gap-2">
                      <Label>Rebuild</Label>
                      <HotkeyCapture
                        value={s.hotkeyTurbo}
                        slot="turbo"
                        clearable
                        ariaLabel="Rebuild shortcut"
                        onCommit={(accel) => commitHotkey("turbo", accel)}
                      />
                    </div>
                    <div className="flex flex-col gap-2">
                      <Label>Project picker</Label>
                      <HotkeyCapture
                        value={s.hotkeyPicker}
                        slot="picker"
                        clearable
                        ariaLabel="Project picker shortcut"
                        onCommit={(accel) => commitHotkey("picker", accel)}
                      />
                    </div>
                  </div>
                </Section>
              </div>
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
  
            <TabsContent value="projects">
              <ProjectsTab s={s} setS={setS} />
            </TabsContent>

            <TabsContent value="profile">
              <Section
                title="Personalization profile"
                titleId="profile-heading"
                hint={`How Ember writes like you. Current source: ${sourceLabel[s.profileSource]}.`}
                detail={
                  <p>
                    Your tone, your rules, the words you never use. It is added to every refine.
                    Ember picks up the global profile you already keep for your coding agent
                    (<code className="font-mono">CLAUDE.md</code>,{" "}
                    <code className="font-mono">AGENTS.md</code>, or{" "}
                    <code className="font-mono">GEMINI.md</code>), or you can load any markdown
                    file and edit it here.
                  </p>
                }
              >
                {s.profilePath && (
                  <p className="truncate font-mono text-xs text-fg-muted" title={s.profilePath}>
                    {s.profilePath}
                  </p>
                )}
                {/* Altura FIXA em vez de `rows`: o perfil de qualquer pessoa que use um CLAUDE.md
                    a serio tem centenas de linhas, e com uma textarea a crescer com o conteudo os
                    botoes de Save e Re-detect eram empurrados para fora do ecra. Aqui a caixa
                    ocupa o espaco que sobra e faz o seu proprio scroll; os botoes ficam sempre a
                    vista. O `min-h-0` e o que permite ao flex encolher a caixa (sem ele, o
                    conteudo impoe a altura minima e o overflow volta a sair para a pagina). */}
                <Textarea
                  aria-labelledby="profile-heading"
                  className="h-[clamp(140px,38vh,420px)] min-h-0 resize-none overflow-y-auto"
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
                    In-the-moment text refiner for any app: prompts, emails, messages, docs.
                    Gemini as the free primary, with one OpenAI-compatible fallback of your
                    choosing, guided by your profile. Built with Tauri.
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
                <DiagnosticsSection
                  debugMode={s.debugMode}
                  savePrompts={s.savePrompts}
                  keepResults={s.keepResults}
                />
              </div>
            </TabsContent>
          </Tabs>
          )}
        </motion.div>
          </motion.main>
    </MotionConfig>
  );
}
