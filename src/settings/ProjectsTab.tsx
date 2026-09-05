import { ContextInspector } from "./ContextInspector";
import { ICON_BY_NAME } from "../components/projectIcons";
import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { toast } from "sonner";
import {
  Sparkle,
  CaretDown,
  X,
  type Icon,
} from "@phosphor-icons/react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { open } from "@tauri-apps/plugin-dialog";
import { ipc, type AccentPreview, type EmberSettings, type Project, type ProjectScan } from "@/lib/ipc";

/**
 * Os nomes dos ícones vêm do Rust (`ember_core::projects::ICONS`); aqui só se mapeia cada nome ao
 * componente. Um nome que o Rust passe a mandar e que não esteja aqui cai no primeiro, em vez de
 * rebentar a lista toda.
 */


function iconOf(name: string): Icon {
  return ICON_BY_NAME[name] ?? Sparkle;
}

/**
 * O anel de tons do DISCO, pelo angulo. A primeira e a ultima paragem sao o mesmo tom (0 e 360
 * graus), o que fecha o circulo sem costura.
 *
 * A lista de recurso so serve o primeiro render, antes de as settings chegarem do Rust.
 */
function wheelRing(wheel: EmberSettings["accentWheel"]) {
  const ring = wheel.ring.length
    ? [...wheel.ring].reverse().join(", ")
    : "#ef4444, #eab308, #4ade80, #06b6d4, #6366f1, #d946ef, #ef4444";
  return `conic-gradient(from 90deg, ${ring})`;
}

/**
 * A bolinha que abre o disco: gradiente LINEAR, e nao conico.
 *
 * Levou tres tentativas e a culpa nunca foi das cores. Um `conic-gradient` de 28px e um problema
 * de renderizacao, nao de paleta: o Chromium desenha-o com banda visivel nesse tamanho e deixa uma
 * costura de um pixel onde o circulo fecha, mesmo com a primeira e a ultima paragem iguais. Um
 * gradiente linear nao tem onde fechar, portanto nao tem costura, e a 28px le-se limpo.
 *
 * Quatro paragens vivas e nao os tons do disco: isto e um ICONE cujo trabalho e dizer "cor" de
 * relance, nao uma pre-visualizacao fiel do que a roda serve.
 */
const RAINBOW =
  "linear-gradient(135deg, #ff3b30 0%, #ff9500 20%, #ffcc00 38%, #34c759 56%, #0a84ff 74%, #bf5af2 100%)";

/**
 * O disco completo: o anel de tons, com o neutro a apagar a saturacao para o centro.
 *
 * O fim do gradiente central e a MESMA cor com alpha zero, e nao `transparent`. Sao coisas
 * diferentes: `transparent` e preto transparente, e o CSS interpola por ele, o que mete uma banda
 * suja no meio da transicao. Era o "corte" que se via.
 */
function wheelBackground(wheel: EmberSettings["accentWheel"]) {
  const c = wheel.centre;
  return `radial-gradient(circle at 50% 50%, ${c} 0%, ${c}00 72%), ${wheelRing(wheel)}`;
}

/**
 * Roda de cores: o angulo e o hue, o raio e o chroma, e a luminosidade e fixa.
 *
 * Fixa de proposito, e nao por preguica de nao ter um terceiro eixo: a cor escolhida e a paragem
 * do MEIO de um gradiente de tres, e a derivacao conta com ela numa faixa de luminosidade (a media
 * das oito fixas). Deixar escolher uma cor quase preta dava um orb sem gradiente. O que a roda
 * mostra e exatamente o que a app consegue pintar, e quem tem um codigo de marca fora desta faixa
 * cola-o no campo ao lado.
 *
 * As cores do anel vem do Rust, ja convertidas. Escrever `oklch()` no CSS dependia do WebView e um
 * disco sem cor era um falhanco silencioso.
 *
 * O marcador vive em estado LOCAL e segue o rato sem esperar por ninguem. A primeira versao usava
 * a posicao que voltava do Rust, e isso dava exactamente o defeito que se via a arrastar: um
 * pedido por movimento, respostas a chegar fora de ordem, e o marcador a saltar para onde o rato
 * ja nao estava. O Rust continua a decidir a COR; a posicao e do rato.
 */
function ColourWheel({
  wheel,
  chroma,
  hue,
  onPreview,
  onCommit,
}: {
  wheel: EmberSettings["accentWheel"];
  chroma: number;
  hue: number;
  /** Enquanto arrasta: so para as tres paragens acompanharem. Nao grava nada. */
  onPreview: (chroma: number, hue: number) => void;
  /** Ao largar: e aqui que a cor entra no projeto. */
  onCommit: (chroma: number, hue: number) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  const [pos, setPos] = useState({ chroma, hue });

  // Reacerta com o que vem de fora (ao abrir, e depois de gravar), nunca a meio de um arraste:
  // ali a verdade e o rato, e deixar a resposta do Rust reposicionar o marcador era o salto.
  useEffect(() => {
    if (!dragging.current) setPos({ chroma, hue });
  }, [chroma, hue]);

  // Um unico handler para o clique e para o arraste: `setPointerCapture` mantem os eventos nesta
  // div mesmo quando o ponteiro sai do disco, que e o que faz o arraste continuar a funcionar ao
  // passar por fora da borda em vez de ficar preso no ultimo valor.
  const at = (e: React.PointerEvent<HTMLDivElement>) => {
    const box = ref.current?.getBoundingClientRect();
    if (!box) return null;
    const r = box.width / 2;
    const dx = e.clientX - (box.left + r);
    const dy = e.clientY - (box.top + r);
    const dist = Math.min(Math.hypot(dx, dy) / r, 1);
    // `atan2` cresce no sentido dos ponteiros do relogio no ecra (y para baixo) e o hue do OKLCH
    // cresce ao contrario: o sinal negativo e o que alinha a cor debaixo do cursor com a cor que
    // sai. Sem ele, a roda pinta o oposto do que devolve.
    const deg = (-Math.atan2(dy, dx) * 180) / Math.PI;
    return { chroma: dist * wheel.maxChroma, hue: (deg + 360) % 360 };
  };

  const r = pos.chroma / (wheel.maxChroma || 1);
  const rad = (pos.hue * Math.PI) / 180;
  const cursor = {
    left: `${50 + Math.cos(rad) * r * 50}%`,
    top: `${50 - Math.sin(rad) * r * 50}%`,
  };

  return (
    <div
      ref={ref}
      role="application"
      aria-label="Colour wheel"
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        dragging.current = true;
        const p = at(e);
        if (p) {
          setPos(p);
          onPreview(p.chroma, p.hue);
        }
      }}
      onPointerMove={(e) => {
        if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
        const p = at(e);
        if (!p) return;
        setPos(p);
        onPreview(p.chroma, p.hue);
      }}
      onPointerUp={(e) => {
        dragging.current = false;
        const p = at(e) ?? pos;
        setPos(p);
        onCommit(p.chroma, p.hue);
      }}
      onPointerCancel={() => {
        dragging.current = false;
      }}
      className="relative h-44 w-44 shrink-0 cursor-crosshair rounded-full shadow-[0_1px_2px_rgba(0,0,0,0.18),0_8px_24px_-8px_rgba(0,0,0,0.35)]"
      style={{ background: wheelBackground(wheel) }}
    >
      {/* O marcador nao leva a cor escolhida por dentro: durante o arraste ela so chega do Rust um
          instante depois, e um marcador que pisca a cor errada le-se pior do que um anel vazio. */}
      <span
        aria-hidden="true"
        className="pointer-events-none absolute h-5 w-5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,0.35)]"
        style={cursor}
      />
    </div>
  );
}

/** Teto do brief, espelhado do Rust (`MAX_BRIEF_CHARS`) só para o contador. Quem corta é o Rust. */
const MAX_BRIEF = 1200;

/**
 * Grelha de escolha única (cores, ícones). É o único primitivo novo que esta funcionalidade
 * precisa: a app não tem Dialog, Popover nem color picker, e nada disto os pede.
 */
function ChoiceGrid({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2">
      <Label>{label}</Label>
      <div role="radiogroup" aria-label={label} className="flex flex-wrap gap-1.5">
        {children}
      </div>
    </div>
  );
}

function ProjectEditor({
  draft,
  accents,
  wheel,
  icons,
  onChange,
  onSave,
  onDelete,
  busy,
}: {
  draft: Project;
  accents: EmberSettings["accents"];
  wheel: EmberSettings["accentWheel"];
  icons: string[];
  onChange: (p: Project) => void;
  onSave: () => void;
  onDelete: () => void;
  busy: boolean;
}) {
  // Apagar em dois passos, sem modal: a app não tem Dialog e não vale a pena construir um para
  // uma confirmação. O botão vira "Really delete?" e volta atrás sozinho.
  const [confirming, setConfirming] = useState(false);

  // Uma cor a medida vale mais do que o indice: com ela preenchida, nenhuma das fixas esta ativa.
  const custom = draft.accentCustom?.trim() ? draft.accentCustom : null;
  // Os tres tons vem do Rust (`preview_accent`), e nao de uma segunda copia da conversao OKLCH
  // aqui: duas implementacoes da mesma rampa divergiam e a pre-visualizacao passaria a mostrar
  // uma cor que o orb nunca pinta.
  const [preview, setPreview] = useState<AccentPreview | null>(null);
  const [picking, setPicking] = useState(false);
  // Contador de pedidos. Sao chamadas locais e rapidas, mas nada garante que voltem na ordem em
  // que sairam, e uma resposta atrasada a sobrescrever uma recente era metade do defeito que se
  // via a arrastar na roda. Quem nao e o ultimo pedido nao escreve.
  const askSeq = useRef(0);
  const panelRef = useRef<HTMLDivElement>(null);
  const swatchRef = useRef<HTMLDivElement>(null);
  const askStops = (chroma: number, hue: number, commit: boolean) => {
    const mine = ++askSeq.current;
    ipc
      .accentFromWheel(chroma, hue)
      .then((p) => {
        if (mine !== askSeq.current) return;
        setPreview(p);
        // Gravar no projeto SO ao largar. A escrever a cada movimento, cada uma disparava o efeito
        // que rebusca o preview do hex, e os dois pedidos passavam a competir um com o outro.
        if (commit) onChange({ ...draft, accentCustom: p.mid });
      })
      .catch(() => {});
  };
  useEffect(() => {
    // O painel cresce para baixo e a janela e pequena: aberto, metade dele ficava por baixo do
    // que se ve. `nearest` desloca o minimo para o mostrar, em vez de saltar a pagina inteira.
    panelRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }, []);

  useEffect(() => {
    if (!picking) return;
    // O disco abre para baixo e para a direita da bolinha; junto ao fundo da janela abria fora do
    // que se ve e parecia que nao tinha acontecido nada. Centra-se ao abrir.
    swatchRef.current?.scrollIntoView({ behavior: "smooth", block: "center" });
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setPicking(false);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [picking]);

  useEffect(() => {
    if (!custom) {
      setPreview(null);
      return;
    }
    let alive = true;
    ipc
      .previewAccent(custom)
      .then((p) => alive && setPreview(p))
      .catch(() => alive && setPreview(null));
    return () => {
      alive = false;
    };
  }, [custom]);

  return (
    <div
      ref={panelRef}
      className="flex flex-col gap-5 border-t border-[color:var(--border-subtle)] px-5 py-5"
    >
      <div className="flex flex-col gap-2">
        <Label htmlFor={`name-${draft.id || "novo"}`}>Name</Label>
        <Input
          id={`name-${draft.id || "novo"}`}
          value={draft.name}
          onChange={(e) => onChange({ ...draft, name: e.target.value })}
          placeholder="e.g. Sintra"
        />
      </div>

      {/* Cor e icone lado a lado: a fila de cores tem nove elementos e deixava meia linha vazia
          a seguir, com o painel a crescer sem necessidade. Abaixo de `sm` voltam a empilhar, que
          e onde nao ha largura para os dois. */}
      <div className="grid gap-5 sm:grid-cols-[auto_1fr]">
      <ChoiceGrid label="Colour">
        {accents.map((a, i) => (
          <button
            key={a.label}
            type="button"
            role="radio"
            aria-checked={!custom && draft.accent === i}
            aria-label={a.label}
            title={a.label}
            // Escolher uma fixa apaga a cor a medida. O indice fica gravado por baixo enquanto a
            // custom esta ligada, para desligar voltar a esta sem ter de a escolher outra vez.
            onClick={() => onChange({ ...draft, accent: i, accentCustom: null })}
            className={`h-7 w-7 rounded-full border-2 transition-transform hover:scale-110 ${
              !custom && draft.accent === i
                ? "border-[color:var(--border-accent)] scale-110"
                : "border-transparent"
            }`}
            style={{ background: a.mid }}
          />
        ))}
        <div className="relative" ref={swatchRef}>
          <button
            type="button"
            aria-haspopup="dialog"
            aria-expanded={picking}
            aria-label="Custom colour"
            title="A colour of your own"
            onClick={() => {
              if (!custom) {
                onChange({
                  ...draft,
                  accentCustom: accents[draft.accent]?.mid ?? "#4a90d9",
                });
              }
              setPicking((v) => !v);
            }}
            className={`grid h-7 w-7 place-items-center rounded-full border-2 transition-transform hover:scale-110 ${
              custom ? "border-[color:var(--border-accent)] scale-110" : "border-transparent"
            }`}
          >
            {/* O gradiente vive num elemento PROPRIO, e nao no fundo do botao.
                Era essa a origem dos cantos estranhos: o fundo de um elemento pinta-se por baixo
                da sua borda, e com uma borda de 2px o recorte redondo do fundo deixa de coincidir
                com o circulo que se ve. Numa cor solida ninguem nota; num gradiente aparece nos
                cantos. Aqui o gradiente tem o seu proprio `rounded-full` e nada por baixo. */}
            <span
              aria-hidden="true"
              className="block h-full w-full rounded-full"
              style={{ background: custom ? preview?.mid ?? custom : RAINBOW }}
            />
          </button>

          <AnimatePresence>
            {picking && (
              <>
                {/* Fechar ao clicar fora. Nao e um modal: escolher uma cor nao merece interromper
                    a pagina nem proteger o foco, so precisa de sair do caminho quando se acaba. */}
                <div
                  className="fixed inset-0 z-40"
                  onClick={() => setPicking(false)}
                  aria-hidden="true"
                />
                <motion.div
                  role="dialog"
                  aria-label="Pick a colour"
                  // O momento: o painel ABRE DA PROPRIA BOLINHA, por um circulo de clip que
                  // cresce a partir dela. E a mesma forma do que se vai escolher, e diz de onde
                  // veio sem precisar de uma seta desenhada.
                  initial={{ clipPath: "circle(14px at 14px 14px)", opacity: 0, scale: 0.96 }}
                  animate={{ clipPath: "circle(150% at 14px 14px)", opacity: 1, scale: 1 }}
                  exit={{ clipPath: "circle(14px at 14px 14px)", opacity: 0, scale: 0.96 }}
                  transition={{ duration: 0.34, ease: [0.22, 1, 0.36, 1] }}
                  style={{ transformOrigin: "14px 14px" }}
                  className="absolute left-0 top-0 z-50 flex flex-col items-center gap-3 rounded-xl border border-[color:var(--border-subtle)] bg-surface-1 p-4 shadow-[0_2px_4px_rgba(0,0,0,0.14),0_18px_48px_-16px_rgba(0,0,0,0.45)]"
                >
                  <ColourWheel
                    wheel={wheel}
                    chroma={preview?.chroma ?? 0}
                    hue={preview?.hue ?? 0}
                    onPreview={(chroma, hue) => askStops(chroma, hue, false)}
                    onCommit={(chroma, hue) => askStops(chroma, hue, true)}
                  />
                  <div className="flex w-44 items-center gap-2">
                    <div
                      className="flex h-7 flex-1 overflow-hidden rounded-sm border border-[color:var(--border-subtle)]"
                      aria-hidden="true"
                    >
                      <span className="flex-1" style={{ background: preview?.raw }} />
                      <span className="flex-1" style={{ background: preview?.mid }} />
                      <span className="flex-1" style={{ background: preview?.glow }} />
                    </div>
                    <Button variant="ghost" size="sm" onClick={() => setPicking(false)}>
                      Done
                    </Button>
                  </div>
                </motion.div>
              </>
            )}
          </AnimatePresence>
        </div>
      </ChoiceGrid>

      <ChoiceGrid label="Icon">
        {icons.map((name) => {
          const I = iconOf(name);
          const on = draft.icon === name;
          return (
            <button
              key={name}
              type="button"
              role="radio"
              aria-checked={on}
              aria-label={name}
              title={name}
              onClick={() => onChange({ ...draft, icon: name })}
              className={`flex h-8 w-8 items-center justify-center rounded-md border transition-colors ${
                on
                  ? "border-[color:var(--border-accent)] text-fg"
                  : "border-[color:var(--border-subtle)] text-fg-muted hover:text-fg"
              }`}
            >
              <I size={15} />
            </button>
          );
        })}
      </ChoiceGrid>
      </div>

      <div className="flex flex-col gap-2">
        <div className="flex items-baseline justify-between gap-2">
          <Label htmlFor={`brief-${draft.id || "novo"}`}>Brief</Label>
          {/* O contador não é decoração: este texto vai no prompt em TODOS os refines, e é o
              único sítio onde esse custo é visível enquanto se escreve. */}
          <span
            className={`font-mono text-[11px] ${
              draft.brief.length > MAX_BRIEF ? "text-[color:var(--color-error)]" : "text-fg-muted"
            }`}
          >
            {draft.brief.length}/{MAX_BRIEF}
          </span>
        </div>
        <Textarea
          id={`brief-${draft.id || "novo"}`}
          value={draft.brief}
          onChange={(e) => onChange({ ...draft, brief: e.target.value })}
          className="h-40 resize-none font-mono text-xs"
          placeholder={
            "What changes how text about this project should be written. For example:\n" +
            "Write in European Portuguese, informal.\n" +
            "Never translate or 'fix': Sintra, e2o, deleg8lab.\n" +
            "Avoid em dashes."
          }
        />
        <p className="text-xs text-fg-muted">
          Writing preferences and technical facts: language, terminology, architecture and constraints.
          Exclude instructions to run commands, edit files or manage agents.
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-2 pt-1">
        <Button variant="primary" onClick={onSave} disabled={busy || !draft.name.trim()}>
          Save
        </Button>
        {draft.id && (
          <Button
            variant="ghost"
            // Red ONLY on the second step. The first click deletes nothing yet and does not
            // deserve alarm; the second one deletes for good, and the colour has to say so before
            // the finger lands.
            className={
              confirming
                ? "border-transparent bg-[color:var(--color-error)] text-white hover:bg-[color:var(--color-error)] hover:brightness-110"
                : undefined
            }
            onClick={() => {
              if (!confirming) {
                setConfirming(true);
                setTimeout(() => setConfirming(false), 4000);
                return;
              }
              onDelete();
            }}
            disabled={busy}
          >
            {confirming ? "Really delete?" : "Delete"}
          </Button>
        )}
      </div>
    </div>
  );
}

export function ProjectsTab({
  s,
  setS,
}: {
  s: EmberSettings;
  setS: (next: EmberSettings) => void;
}) {
  const [openId, setOpenId] = useState<string | null>(null);
  const [draft, setDraftState] = useState<Project | null>(null);
  const draftEpoch = useRef(0);
  const setDraft: React.Dispatch<React.SetStateAction<Project | null>> = (next) => {
    draftEpoch.current += 1;
    setDraftState(next);
  };
  const [busy, setBusy] = useState(false);
  const [distilling, setDistilling] = useState(false);
  const [scan, setScan] = useState<(ProjectScan & { folder: string }) | null>(null);

  const blank = (): Project => ({
    id: "",
    name: "",
    accent: s.projects.length % Math.max(s.accents.length, 1),
    icon: s.icons[0] ?? "sparkle",
    brief: "",
    folder: null,
    sourcePath: null,
  });

  /** Fecha o cartao de projeto novo e deita fora o rascunho. Nada foi gravado ate aqui. */
  const discardNew = () => {
    setOpenId(null);
    setDraft(null);
    setScan(null);
  };

  const startNew = () => {
    setScan(null);
    setDraft(blank());
    setOpenId("__novo__");
  };

  /**
   * Escolher pasta NÃO envia nada. Só lê o que lá está e mostra qual dos ficheiros ganharia e
   * porquê. O envio fica atrás de um segundo clique explícito: um repo de cliente não pode sair
   * da máquina por causa de um clique numa pasta.
   */
  const pickFolder = async () => {
    const epoch = draftEpoch.current;
    const chosen = await open({ directory: true, multiple: false });
    if (typeof chosen !== "string" || epoch !== draftEpoch.current) return;
    setBusy(true);
    try {
      const r = await ipc.scanProjectFolder(chosen);
      if (epoch !== draftEpoch.current) return;
      setScan({ ...r, folder: chosen });
      // Nome sugerido a partir da pasta: quase sempre é o certo, e continua editável.
      const base = chosen.split(/[\\/]/).filter(Boolean).pop() ?? "";
      setDraft((d) => (d ? { ...d, name: d.name || base, folder: chosen } : d));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  /** Escolher uma subpasta é o mesmo que a ter escolhido no seletor: volta a fazer o scan. */
  const useSubfolder = async (path: string) => {
    const epoch = draftEpoch.current;
    setBusy(true);
    try {
      const r = await ipc.scanProjectFolder(path);
      if (epoch !== draftEpoch.current) return;
      setScan({ ...r, folder: path });
      const base = path.split(/[\\/]/).filter(Boolean).pop() ?? "";
      setDraft((d) => (d ? { ...d, name: base, folder: path } : d));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const distil = async () => {
    if (!scan?.folder) return;
    const epoch = draftEpoch.current;
    setDistilling(true);
    try {
      const brief = await ipc.distillProject(scan.folder, scan.sourceFingerprint);
      if (epoch !== draftEpoch.current) {
        toast.info("The draft changed. Generate the brief again to use the current project.");
        return;
      }
      setDraft((d) => (d ? { ...d, brief, sourcePath: scan.sourcePath, sourceFingerprint: scan.sourceFingerprint } : d));
      toast.success("Read it. Check the brief below before saving.");
    } catch (e) {
      // A mensagem vem do Rust já a dizer o que falhou de verdade (sem ficheiro, sem rede,
      // nada de útil no ficheiro, resposta rejeitada). O projeto continua a poder ser gravado
      // com um brief escrito à mão.
      toast.error(String(e));
    } finally {
      setDistilling(false);
    }
  };

  const rescanDraft = async () => {
    if (!draft?.folder || busy || distilling) return;
    const epoch = draftEpoch.current;
    const folder = draft.folder;
    setBusy(true);
    try {
      const result = await ipc.scanProjectFolder(folder);
      if (epoch === draftEpoch.current) setScan({ ...result, folder });
    } catch { toast.error("Project sources could not be read."); }
    finally { setBusy(false); }
  };

  const toggleEditor = (p: Project) => {
    if (openId === p.id) {
      // Fecha SEM limpar o rascunho: se o limpasse aqui, o conteúdo desmontava no mesmo frame e
      // a caixa ficava a encolher vazia. O rascunho só é trocado quando se abre outro projeto.
      setOpenId(null);
      return;
    }
    setScan(null);
    setDraft({ ...p });
    setOpenId(p.id);
  };

  const save = async () => {
    if (!draft || busy || distilling) return;
    const epoch = draftEpoch.current;
    setBusy(true);
    try {
      setS(await ipc.saveProject(draft));
      if (epoch === draftEpoch.current) setOpenId(null);
      else toast.info("Your newer edits remain in the draft. Save them when ready.");
      toast.success("Project saved.");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: string) => {
    setBusy(true);
    try {
      setS(await ipc.deleteProject(id));
      setOpenId(null);
      setDraft(null);
      toast.success("Project deleted.");
    } catch {
      toast.error("Couldn't delete the project.");
    } finally {
      setBusy(false);
    }
  };

  const setActive = async (id: string | null) => {
    setBusy(true);
    try {
      setS(await ipc.setActiveProject(id));
      toast.success(id ? "Project is now active." : "No project active.");
    } catch {
      toast.error("Couldn't change the active project.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <ContextInspector />
      {openId && draft?.folder && <Button variant="ghost" disabled={busy || distilling} onClick={() => void rescanDraft()}>Check project sources</Button>}
      {scan && openId !== "__novo__" && <div className="space-y-2">
        <Button variant="ghost" disabled={busy || distilling || !scan.sourcePaths.length} onClick={() => void distil()}>Generate a reviewed draft</Button>
        {draft?.id && <details className="text-xs"><summary>Previously saved brief</summary><pre className="whitespace-pre-wrap p-3">{s.projects.find(p => p.id === draft.id)?.brief}</pre></details>}
      </div>}
      {scan && <div className="text-xs text-fg-muted" role="status"><p>Sources: {scan.sourcePaths.join(", ") || "None"}</p>{scan.warnings.map((warning) => <p key={warning}>{warning}</p>)}</div>}
      <div className="rounded-lg border border-[color:var(--border-subtle)] bg-surface-1 p-5">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <h3 className="text-sm font-semibold text-fg">Projects</h3>
            <p className="mt-1 text-xs text-fg-muted">
              A project's brief rides along with every refine while it's active, so names and
              wording specific to that work survive. Nothing here leaves your machine on its own.
            </p>
          </div>
          <Button variant="ghost" onClick={startNew} disabled={busy} className="shrink-0">
            Add project
          </Button>
        </div>

        {s.activeProject && (
          <p className="mt-3 text-xs text-fg-muted">
            While a project is active it replaces the focused-window detection in the Providers
            tab.
          </p>
        )}
      </div>

      {openId === "__novo__" && draft && (
        <div className="overflow-hidden rounded-lg border border-[color:var(--border-accent)] bg-surface-1">
          {/* `pb-5` para igualar o `py-5` do editor logo abaixo: a linha divisória fica com o
              mesmo ar dos dois lados. Sem ele, este bloco só tinha padding em cima e o botão
              "Pick folder" ficava encostado ao traço. */}
          <div className="flex flex-col gap-3 px-5 pb-5 pt-5">
            <div className="flex items-center justify-between gap-3">
              <h4 className="text-sm font-semibold text-fg">New project</h4>
              <div className="flex items-center gap-2">
                <Button variant="ghost" onClick={pickFolder} disabled={busy || distilling}>
                  Pick folder…
                </Button>
                {/* Sair sem gravar. Sem isto, abrir "Add project" por engano era um beco: só se
                    saía a gravar um projeto que não se queria, ou a trocar de separador. */}
                <button
                  type="button"
                  onClick={discardNew}
                  disabled={busy || distilling}
                  aria-label="Discard this project"
                  title="Discard"
                  className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-[color:var(--border-subtle)] text-fg-muted transition-colors hover:border-[color:var(--border-accent)] hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[color:var(--border-accent)]"
                >
                  <X size={14} weight="bold" />
                </button>
              </div>
            </div>

            {scan && (
              <div className="rounded-md border border-[color:var(--border-subtle)] bg-surface-2 p-3">
                <p className="truncate font-mono text-[11px] text-fg-muted">{scan.folder}</p>
                {scan.fileName ? (
                  <>
                    <p className="mt-2 text-xs text-fg">
                      Will read <span className="font-mono">{scan.fileName}</span> ({scan.lines}{" "}
                      lines) and send it to your model once, to write the brief.
                    </p>
                    {scan.candidates.length > 1 && (
                      // Mostrar os pesos torna a escolha explicável: dá para ver que um
                      // CLAUDE.md de uma linha perdeu para o AGENTS.md, em vez de parecer magia.
                      <p className="mt-1 font-mono text-[11px] text-fg-muted">
                        {scan.candidates
                          .map((c) => `${c.fileName} ${c.score}${c.chosen ? " ←" : ""}`)
                          .join("   ")}
                      </p>
                    )}
                    <Button
                      variant="primary"
                      onClick={distil}
                      disabled={distilling}
                      className="mt-3"
                    >
                      {distilling ? (
                        <span className="flex items-center gap-1.5">
                          <Spinner variant="embers" size={14} /> Reading…
                        </span>
                      ) : (
                        "Read and write the brief"
                      )}
                    </Button>
                  </>
                ) : scan.subfolders.length > 0 ? (
                  // Apontar à pasta-mãe em vez do repo é um erro natural e acontece. Dizer só
                  // "não há nada aqui" é verdade e não ajuda; oferecer as que têm resolve-o num
                  // clique, sem obrigar a reabrir o seletor.
                  <>
                    <p className="mt-2 text-xs text-fg">
                      Nothing here, but these folders inside it have conventions:
                    </p>
                    <div className="mt-2 flex flex-col gap-1">
                      {scan.subfolders.map((sf) => (
                        <button
                          key={sf.path}
                          type="button"
                          onClick={() => useSubfolder(sf.path)}
                          disabled={busy || distilling}
                          className="flex items-baseline gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-surface-3"
                        >
                          <span className="text-xs text-fg">{sf.name}</span>
                          <span className="font-mono text-[11px] text-fg-muted">
                            {sf.fileName}
                          </span>
                        </button>
                      ))}
                    </div>
                  </>
                ) : (
                  <p className="mt-2 text-xs text-fg-muted">
                    No conventions file here (no AGENTS.md, CLAUDE.md or similar with anything in
                    it). Write the brief yourself below.
                  </p>
                )}
              </div>
            )}
          </div>
          <ProjectEditor
            draft={draft}
            accents={s.accents}
            wheel={s.accentWheel}
            icons={s.icons}
            onChange={setDraft}
            onSave={save}
            onDelete={() => {}}
            busy={busy}
          />
        </div>
      )}

      {s.projects.length === 0 && openId !== "__novo__" && (
        <div className="rounded-lg border border-dashed border-[color:var(--border-subtle)] p-6 text-center text-xs text-fg-muted">
          No projects yet. Add one and write a couple of lines about how text for it should read.
        </div>
      )}

      {s.projects.map((p) => {
        const isActive = s.activeProject === p.id;
        const isOpen = openId === p.id;
        // Com o painel aberto, o cabecalho mostra o RASCUNHO e nao o que esta gravado: escolher
        // uma cor ou um icone e nao ver nada mudar ate carregar em Save nao diz se a escolha
        // pegou. Continua a ser so pre-visualizacao; quem grava e o Save.
        const shown = isOpen && draft?.id === p.id ? draft : p;
        const I = iconOf(shown.icon);
        // A cor a medida GANHA ao indice, como no Rust (`resolve_accent`). Sem esta linha,
        // escolher uma cor, gravar, e ver o cartao com a cor antiga parecia que a gravacao nao
        // tinha funcionado, quando o que estava errado era so o que se mostrava.
        const dot = shown.accentCustom?.trim() || (s.accents[shown.accent] ?? s.accents[0])?.mid;
        return (
          <div
            key={p.id}
            className="overflow-hidden rounded-lg border border-[color:var(--border-subtle)] bg-surface-1"
          >
            <div className="flex items-center gap-3 px-5 py-4">
              <span
                className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg"
                style={{ background: dot ?? "var(--color-accent)", color: "#1a0e03" }}
              >
                <I size={17} weight="bold" />
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="truncate text-sm font-semibold text-fg">{p.name}</span>
                  {isActive && (
                    // `leading-none` + padding simetrico: sem isso o `uppercase tracking-wide`
                    // empurrava a etiqueta para fora da caixa e ela ficava colada ao nome.
                    <span
                      className="shrink-0 whitespace-nowrap rounded-full px-2 py-1 text-[10px] font-semibold uppercase leading-none tracking-wider"
                      style={{
                        color: dot ?? "var(--color-accent)",
                        border: `1px solid ${dot ?? "var(--color-accent)"}`,
                      }}
                    >
                      Active
                    </span>
                  )}
                </div>
                <p className="mt-0.5 truncate text-xs text-fg-muted">
                  {p.brief.trim()
                    ? p.brief.trim().split("\n")[0]
                    : "No brief yet: this project changes nothing until you write one."}
                </p>
              </div>
              <Button
                variant="ghost"
                onClick={() => setActive(isActive ? null : p.id)}
                disabled={busy}
                className="shrink-0"
              >
                {isActive ? "Deactivate" : "Set active"}
              </Button>
              <button
                type="button"
                onClick={() => toggleEditor(p)}
                disabled={busy}
                aria-label={isOpen ? "Close" : "Edit"}
                aria-expanded={isOpen}
                className="flex h-8 w-8 shrink-0 items-center justify-center rounded-sm text-fg-muted transition-colors hover:text-fg"
              >
                <CaretDown
                  size={14}
                  weight="bold"
                  className={isOpen ? "rotate-180 transition-transform" : "transition-transform"}
                />
              </button>
            </div>
            {/* `height: auto` pelo motion, e nao uma transicao CSS de `grid-template-rows`.
                O caminho do CSS foi tentado e MEDIDO: a linha ficava presa em 0px para sempre e a
                caixa nunca chegava a abrir (sem a transicao resolvia para 569px, com ela ficava a
                zero). O motion mede a altura real e anima ate ela, que e precisamente o problema
                que ele existe para resolver.

                O `AnimatePresence` trata do outro lado: mantem o conteudo montado durante o fecho.
                Sem ele, o editor desaparecia no instante do clique e via-se uma caixa vazia a
                encolher, que era a outra metade do que parecia mal. */}
            <AnimatePresence initial={false}>
              {isOpen && draft && draft.id === p.id && (
                <motion.div
                  key="editor"
                  initial={{ height: 0, opacity: 0 }}
                  animate={{ height: "auto", opacity: 1 }}
                  exit={{ height: 0, opacity: 0 }}
                  transition={{ duration: 0.32, ease: [0.22, 1, 0.36, 1] }}
                  style={{ overflow: "hidden" }}
                >
                  <ProjectEditor
                    draft={draft}
                    accents={s.accents}
                    wheel={s.accentWheel}
                    icons={s.icons}
                    onChange={setDraft}
                    onSave={save}
                    onDelete={() => remove(p.id)}
                    busy={busy}
                  />
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        );
      })}
    </div>
  );
}
