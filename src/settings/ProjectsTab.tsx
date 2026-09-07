import { SourcePath } from "../components/SourcePath";
import { Feedback } from "../components/Feedback";
import { ContextInspector } from "./ContextInspector";
import { InfoPopover } from "./Section";
import { ICON_BY_NAME } from "../components/projectIcons";
import { useEffect, useRef, useState } from "react";
import { motion, useReducedMotion } from "motion/react";
import { toast } from "sonner";
import { Sparkle, CaretLeft, CaretRight, X, type Icon } from "@phosphor-icons/react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  Dialog,
  DialogBody,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { open } from "@tauri-apps/plugin-dialog";
import { ipc, type AccentPreview, type EmberSettings, type Project, type ProjectScan } from "@/lib/ipc";
import { cn } from "@/lib/utils";

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
        if (e.button !== 0 || !e.isPrimary) return;
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
        if (!dragging.current || !e.currentTarget.hasPointerCapture(e.pointerId)) return;
        dragging.current = false;
        e.currentTarget.releasePointerCapture(e.pointerId);
        const p = at(e) ?? pos;
        setPos(p);
        onCommit(p.chroma, p.hue);
      }}
      onPointerCancel={() => {
        dragging.current = false;
        setPos({ chroma, hue });
        onPreview(chroma, hue);
      }}
      onLostPointerCapture={() => {
        if (!dragging.current) return;
        dragging.current = false;
        setPos({ chroma, hue });
        onPreview(chroma, hue);
      }}
      className="relative h-44 w-44 shrink-0 cursor-crosshair rounded-full shadow-[0_1px_2px_rgba(0,0,0,0.18),0_8px_24px_-8px_rgba(0,0,0,0.35)]"
      style={{ background: wheelBackground(wheel), touchAction: "none" }}
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

/** Grelha de escolha única (cores, ícones). */
function ChoiceGrid({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label>{label}</Label>
      <div role="radiogroup" aria-label={label} className="flex flex-wrap gap-1.5">
        {children}
      </div>
    </div>
  );
}

const emptySource = (path: string) => ({ path, text: "", fingerprint: "", excludedLines: 0 });

/** Applications and authorised files of a project, in a dialog. The editor shows one summary
 *  line; the lists can be long and used to sit inside a native disclosure that grew the card. */
function AutomaticContextDialog({
  draft,
  busy,
  onChange,
}: {
  draft: Project;
  busy: boolean;
  onChange: React.Dispatch<React.SetStateAction<Project | null>>;
}) {
  const applications = draft.context?.applications ?? [];
  const sources = draft.context?.sources ?? [];
  const setContext = (next: { applications?: string[]; sources?: Project["context"] extends infer C ? (C extends { sources: infer S } ? S : never) : never }) =>
    onChange((current) => current && current.id === draft.id
      ? { ...current, context: { version: 1, applications: next.applications ?? current.context?.applications ?? [], sources: next.sources ?? current.context?.sources ?? [] } }
      : current);
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm" aria-label="Manage automatic context">Manage…</Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Automatic context</DialogTitle>
          <DialogDescription>A project path is used first. Application associations apply only when one project matches.</DialogDescription>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-4 text-xs">
          <div>
            <div className="flex items-center justify-between gap-2">
              <p className="font-medium text-fg">Applications</p>
              <Button variant="ghost" size="sm" disabled={busy} onClick={async () => {
                const selected = await open({ multiple: false, directory: false, title: "Choose application" });
                if (typeof selected === "string") setContext({ applications: [...new Set([...applications, selected])] });
              }}>Associate application</Button>
            </div>
            {applications.length === 0
              ? <p className="mt-2 text-fg-muted">No application associations. Project paths and manual selection still work.</p>
              : <ul className="mt-2 flex flex-col gap-2">{applications.map(path => (
                  <li key={path} className="flex items-center justify-between gap-2">
                    <SourcePath path={path} />
                    <Button size="sm" variant="outline" disabled={busy} onClick={() => setContext({ applications: applications.filter(p => p !== path) })}>Remove</Button>
                  </li>
                ))}</ul>}
          </div>
          <div>
            <div className="flex items-center justify-between gap-2">
              <div>
                <p className="font-medium text-fg">Authorized sources</p>
                <p className="mt-0.5 text-fg-muted">Changes to these files update locally. New files and imports require your choice.</p>
              </div>
              <Button variant="ghost" size="sm" disabled={busy || !draft.folder} onClick={async () => {
                const selected = await open({ multiple: true, directory: false, title: "Authorize context files inside this project", filters: [{ name: "Context", extensions: ["md", "markdown", "txt"] }] });
                if (!selected) return;
                const picked = (Array.isArray(selected) ? selected : [selected]).filter(path => !sources.some(s => s.path === path));
                setContext({ sources: [...sources, ...picked.map(emptySource)] });
              }}>Add sources</Button>
            </div>
            {sources.length === 0
              ? <p className="mt-2 text-fg-muted">No authorized files. Your saved brief is still used.</p>
              : <ul className="mt-2 flex flex-col gap-2">{sources.map(source => (
                  <li key={source.path} className="flex items-center justify-between gap-2">
                    <SourcePath path={source.path} />
                    <Button size="sm" variant="outline" disabled={busy} onClick={() => setContext({ sources: sources.filter(s => s.path !== source.path) })}>Remove</Button>
                  </li>
                ))}</ul>}
          </div>
        </DialogBody>
        <DialogFooter>
          <DialogClose asChild><Button variant="ghost" size="sm">Done</Button></DialogClose>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ProjectEditor({
  draft,
  isNew,
  accents,
  wheel,
  icons,
  scan,
  savedBrief,
  onChange,
  onSave,
  onDelete,
  onBack,
  onDiscard,
  onPickFolder,
  onUseSubfolder,
  onDistil,
  onRescan,
  busy,
  saving,
  distilling,
}: {
  draft: Project;
  isNew: boolean;
  accents: EmberSettings["accents"];
  wheel: EmberSettings["accentWheel"];
  icons: string[];
  scan: (ProjectScan & { folder: string }) | null;
  /** The brief on disk, when it differs from the draft after a generated one. */
  savedBrief: string | null;
  onChange: React.Dispatch<React.SetStateAction<Project | null>>;
  onSave: () => void;
  onDelete: () => void;
  onBack: () => void;
  onDiscard: () => void;
  onPickFolder: () => void;
  onUseSubfolder: (path: string) => void;
  onDistil: () => void;
  onRescan: () => void;
  busy: boolean;
  saving: boolean;
  distilling: boolean;
}) {
  // Apagar em dois passos, sem modal: o botão vira "Really delete?" e volta atrás sozinho.
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
  const askStops = (chroma: number, hue: number, commit: boolean) => {
    const mine = ++askSeq.current;
    ipc
      .accentFromWheel(chroma, hue)
      .then((p) => {
        if (mine !== askSeq.current) return;
        setPreview(p);
        // Gravar no projeto SO ao largar. A escrever a cada movimento, cada uma disparava o efeito
        // que rebusca o preview do hex, e os dois pedidos passavam a competir um com o outro.
        if (commit) onChange(current => current && current.id === draft.id && current.accentCustom === draft.accentCustom
          ? { ...current, accentCustom: p.mid } : current);
      })
      .catch(() => {});
  };

  useEffect(() => {
    const mine = ++askSeq.current;
    if (!custom) setPreview(null);
    else ipc.previewAccent(custom)
      .then(value => { if (mine === askSeq.current) setPreview(value); })
      .catch(() => { if (mine === askSeq.current) setPreview(null); });
    // Hex lookups and wheel responses share one revision, including editor retirement.
    return () => { askSeq.current++; };
  }, [custom]);

  const id = draft.id || "novo";
  const applications = draft.context?.applications.length ?? 0;
  const sourceCount = draft.context?.sources.length ?? 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 p-[var(--card-pad,1.25rem)]">
      <div className="flex items-center gap-2">
        <Button variant="ghost" size="sm" className="@3xl/settings:hidden" onClick={onBack} aria-label="Back to projects" disabled={busy || distilling}>
          <CaretLeft size={14} weight="bold" aria-hidden="true" />
        </Button>
        <Label htmlFor={`name-${id}`} className="sr-only">Name</Label>
        <Input
          id={`name-${id}`}
          className="min-w-0 flex-1"
          value={draft.name}
          onChange={(e) => onChange({ ...draft, name: e.target.value })}
          placeholder={isNew ? "Project name, e.g. Sintra" : "Name"}
        />
        {isNew && (
          <Button variant="ghost" onClick={onPickFolder} disabled={busy || distilling}>
            Pick folder…
          </Button>
        )}
        <Button variant="primary" onClick={onSave} loading={saving} disabled={busy || !draft.name.trim()}>
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
        {isNew && (
          // Sair sem gravar. Sem isto, abrir "Add project" por engano era um beco.
          <Button variant="ghost" size="icon" onClick={onDiscard} disabled={busy || distilling} aria-label="Discard this project" title="Discard">
            <X size={14} weight="bold" aria-hidden="true" />
          </Button>
        )}
      </div>

      {isNew && scan && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-[color:var(--border-subtle)] bg-surface-2 px-3 py-2 text-xs">
          <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-fg-muted" title={scan.folder}>{scan.folder}</span>
          {scan.fileName ? (
            <>
              <span className="text-fg" title={scan.candidates.length > 1 ? scan.candidates.map((c) => `${c.fileName} ${c.score}${c.chosen ? " (chosen)" : ""}`).join(", ") : undefined}>
                Reads <span className="font-mono">{scan.fileName}</span> ({scan.lines} lines) once, to write the brief.
              </span>
              <Button variant="primary" size="sm" onClick={onDistil} disabled={distilling} className="disabled:opacity-100" aria-busy={distilling}>
                <span className="inline-flex w-4 justify-center" aria-hidden>{distilling && <Spinner size={14} />}</span>
                Read and write the brief
              </Button>
            </>
          ) : scan.subfolders.length > 0 ? (
            // Apontar à pasta-mãe em vez do repo é um erro natural e acontece. Oferecer as que
            // têm conventions resolve-o num clique, sem obrigar a reabrir o seletor.
            <Dialog>
              <span className="text-fg">Nothing here, but {scan.subfolders.length} folders inside it have conventions.</span>
              <DialogTrigger asChild><Button variant="ghost" size="sm">Choose one…</Button></DialogTrigger>
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>Folders with conventions</DialogTitle>
                  <DialogDescription>Inside {scan.folder}. Picking one scans it as if you had chosen it.</DialogDescription>
                </DialogHeader>
                <DialogBody className="flex flex-col gap-1">
                  {scan.subfolders.map((sf) => (
                    <DialogClose asChild key={sf.path}>
                      <button
                        type="button"
                        onClick={() => onUseSubfolder(sf.path)}
                        disabled={busy || distilling}
                        className="flex items-baseline gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-surface-3"
                      >
                        <span className="text-xs text-fg">{sf.name}</span>
                        <span className="font-mono text-[11px] text-fg-muted">{sf.fileName}</span>
                      </button>
                    </DialogClose>
                  ))}
                </DialogBody>
              </DialogContent>
            </Dialog>
          ) : (
            <span className="text-fg-muted">No conventions file here (no AGENTS.md, CLAUDE.md or similar). Write the brief yourself.</span>
          )}
        </div>
      )}

      {/* Cor e icone lado a lado: a fila de cores tem nove elementos e deixava meia linha vazia
          a seguir. Em contentores estreitos voltam a empilhar. */}
      <div className="grid gap-3 @xl/settings:grid-cols-[auto_1fr]">
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
          {/* The wheel lives in a popover (portalled) so the editor pane can clip its own
              content without clipping the disc. Non-modal: choosing a colour does not deserve
              to interrupt the page, it only needs to get out of the way when done. */}
          <Popover open={picking} onOpenChange={setPicking}>
            <PopoverTrigger asChild>
              <button
                type="button"
                aria-label="Custom colour"
                title="A colour of your own"
                onClick={() => {
                  if (!custom) {
                    onChange({
                      ...draft,
                      accentCustom: accents[draft.accent]?.mid ?? "#4a90d9",
                    });
                  }
                }}
                className={`grid h-7 w-7 place-items-center rounded-full border-2 transition-transform hover:scale-110 ${
                  custom ? "border-[color:var(--border-accent)] scale-110" : "border-transparent"
                }`}
              >
                {/* O gradiente vive num elemento PROPRIO, e nao no fundo do botao: o fundo de um
                    elemento pinta-se por baixo da sua borda, e com uma borda de 2px o recorte
                    redondo do fundo deixa de coincidir com o circulo que se ve. */}
                <span
                  aria-hidden="true"
                  className="block h-full w-full rounded-full"
                  style={{ background: custom ? preview?.mid ?? custom : RAINBOW }}
                />
              </button>
            </PopoverTrigger>
            <PopoverContent aria-label="Pick a colour" align="start" className="flex w-auto flex-col items-center gap-3 rounded-xl bg-surface-1 p-4">
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
            </PopoverContent>
          </Popover>
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
                className={`flex h-7 w-7 items-center justify-center rounded-md border transition-colors ${
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

      <div className="flex min-h-[7.5rem] flex-1 flex-col gap-1.5">
        <div className="flex items-baseline justify-between gap-2">
          <div className="flex items-center gap-1.5">
            <Label htmlFor={`brief-${id}`}>Brief</Label>
            <InfoPopover title="Brief">
              <p>
                Writing preferences and technical facts: language, terminology, architecture and
                constraints. Exclude instructions to run commands, edit files or manage agents.
                This text rides along with every refine while the project is active.
              </p>
            </InfoPopover>
          </div>
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
          id={`brief-${id}`}
          data-scroll-pane=""
          value={draft.brief}
          onChange={(e) => onChange({ ...draft, brief: e.target.value })}
          className="min-h-[72px] flex-1 font-mono text-xs"
          placeholder={
            "What changes how text about this project should be written. For example:\n" +
            "Write in European Portuguese, informal.\n" +
            "Never translate or 'fix': Sintra, e2o, deleg8lab.\n" +
            "Avoid em dashes."
          }
        />
      </div>

      <div className="flex shrink-0 flex-wrap items-center gap-2 text-xs">
        <span className="text-fg-muted">
          Automatic context · {applications} {applications === 1 ? "app" : "apps"} · {sourceCount} {sourceCount === 1 ? "source" : "sources"}
        </span>
        <AutomaticContextDialog draft={draft} busy={busy} onChange={onChange} />
        <div className="ml-auto flex flex-wrap items-center gap-2">
          {draft.folder && !isNew && (
            <Button variant="ghost" size="sm" disabled={busy || distilling} onClick={onRescan}>Check project sources</Button>
          )}
          {scan && !isNew && (
            <Button variant="ghost" size="sm" loading={distilling} disabled={busy || !scan.sourcePaths.length} onClick={onDistil}>Generate a reviewed draft</Button>
          )}
          {scan && !isNew && savedBrief !== null && (
            <Dialog>
              <DialogTrigger asChild><Button variant="ghost" size="sm">Saved brief…</Button></DialogTrigger>
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>Previously saved brief</DialogTitle>
                  <DialogDescription>What is on disk for this project right now.</DialogDescription>
                </DialogHeader>
                <DialogBody>
                  <pre className="whitespace-pre-wrap break-words rounded-md border border-[color:var(--border-subtle)] bg-surface-2 p-3 font-mono text-[11px] leading-relaxed text-fg-muted">{savedBrief || "Empty."}</pre>
                </DialogBody>
                <DialogFooter>
                  <DialogClose asChild><Button variant="ghost" size="sm">Close</Button></DialogClose>
                  <DialogClose asChild>
                    <Button variant="primary" size="sm" onClick={() => onChange({ ...draft, brief: savedBrief })}>Restore saved brief</Button>
                  </DialogClose>
                </DialogFooter>
              </DialogContent>
            </Dialog>
          )}
          {scan && (
            <Button variant="ghost" size="sm" disabled={busy || !scan.sourcePaths.length} onClick={() => onChange(current => current ? { ...current, context: { version: 1, applications: current.context?.applications ?? [], sources: scan.sourcePaths.map(emptySource) } } : current)}>Use scanned sources automatically</Button>
          )}
          {scan && (
            <Popover>
              <PopoverTrigger asChild><Button variant="ghost" size="sm">Scanned sources</Button></PopoverTrigger>
              <PopoverContent className="w-96" role="status">
                <p className="break-all">Sources: {scan.sourcePaths.join(", ") || "None"}</p>
                {scan.warnings.map((warning) => <p key={warning} className="mt-1">{warning}</p>)}
              </PopoverContent>
            </Popover>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * Projects, master-detail: the list on the left scrolls inside its own pane, the editor on the
 * right takes the remaining height and gives it to the brief. In narrow containers the editor
 * replaces the list, with a Back button. It used to be a stack of cards where each project
 * expanded in place to a 569px editor, which is why the tab scrolled.
 */
export function ProjectsTab({
  s,
  setS,
}: {
  s: EmberSettings;
  setS: (next: EmberSettings) => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const still = useReducedMotion();
  const [openId, setOpenId] = useState<string | null>(null);
  const [draft, setDraftState] = useState<Project | null>(null);
  const draftEpoch = useRef(0);
  useEffect(() => () => { draftEpoch.current++; }, []);
  const setDraft: React.Dispatch<React.SetStateAction<Project | null>> = (next) => {
    draftEpoch.current += 1;
    setDraftState(next);
  };
  const [saving, setSaving] = useState(false);
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

  /** Fecha o editor de projeto novo e deita fora o rascunho. Nada foi gravado ate aqui. */
  const discardNew = () => {
    setOpenId(null);
    setDraft(null);
    setScan(null);
  };

  const startNew = () => {
    setError(null);
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
    setError(null);
    setBusy(true);
    try {
      const r = await ipc.scanProjectFolder(chosen);
      if (epoch !== draftEpoch.current) return;
      setScan({ ...r, folder: chosen });
      // Nome sugerido a partir da pasta: quase sempre é o certo, e continua editável.
      const base = chosen.split(/[\\/]/).filter(Boolean).pop() ?? "";
      setDraft((d) => (d ? { ...d, name: d.name || base, folder: chosen } : d));
    } catch (e) {
      if (epoch === draftEpoch.current) setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  /** Escolher uma subpasta é o mesmo que a ter escolhido no seletor: volta a fazer o scan. */
  const useSubfolder = async (path: string) => {
    const epoch = draftEpoch.current;
    setError(null);
    setBusy(true);
    try {
      const r = await ipc.scanProjectFolder(path);
      if (epoch !== draftEpoch.current) return;
      setScan({ ...r, folder: path });
      const base = path.split(/[\\/]/).filter(Boolean).pop() ?? "";
      setDraft((d) => (d ? { ...d, name: base, folder: path } : d));
    } catch (e) {
      if (epoch === draftEpoch.current) setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const distil = async () => {
    if (!scan?.folder) return;
    const epoch = draftEpoch.current;
    setError(null);
    setDistilling(true);
    try {
      const brief = await ipc.distillProject(scan.folder, scan.sourceFingerprint);
      if (epoch !== draftEpoch.current) {
        toast.info("The draft changed. Generate the brief again to use the current project.");
        return;
      }
      setDraft((d) => (d ? { ...d, brief, sourcePath: scan.sourcePath, sourceFingerprint: scan.sourceFingerprint } : d));
      toast.success("Read it. Check the brief before saving.");
    } catch (e) {
      // A mensagem vem do Rust já a dizer o que falhou de verdade (sem ficheiro, sem rede,
      // nada de útil no ficheiro, resposta rejeitada). O projeto continua a poder ser gravado
      // com um brief escrito à mão.
      if (epoch === draftEpoch.current) setError(String(e));
    } finally {
      setDistilling(false);
    }
  };

  const rescanDraft = async () => {
    if (!draft?.folder || busy || distilling) return;
    const epoch = draftEpoch.current;
    const folder = draft.folder;
    setError(null);
    setBusy(true);
    try {
      const result = await ipc.scanProjectFolder(folder);
      if (epoch === draftEpoch.current) setScan({ ...result, folder });
    } catch { if (epoch === draftEpoch.current) setError("Project sources could not be read."); }
    finally { setBusy(false); }
  };

  const openEditor = (p: Project) => {
    setError(null);
    if (openId === p.id) return;
    setScan(null);
    setDraft({ ...p });
    setOpenId(p.id);
  };

  const closeEditor = () => {
    setOpenId(null);
    setScan(null);
  };

  const save = async () => {
    if (!draft || busy || distilling) return;
    const epoch = draftEpoch.current;
    setError(null);
    setBusy(true);
    setSaving(true);
    try {
      setS(await ipc.saveProject(draft));
      if (epoch === draftEpoch.current) closeEditor();
      else toast.info("Your newer edits remain in the draft. Save them when ready.");
      toast.success("Project saved.");
    } catch (e) {
      if (epoch === draftEpoch.current) setError(String(e));
    } finally {
      setBusy(false);
      setSaving(false);
    }
  };

  const remove = async (id: string) => {
    setError(null);
    setBusy(true);
    try {
      setS(await ipc.deleteProject(id));
      setOpenId(null);
      setDraft(null);
      toast.success("Project deleted.");
    } catch {
      setError("Couldn't delete the project.");
    } finally {
      setBusy(false);
    }
  };

  const setActive = async (id: string | null) => {
    setError(null);
    setBusy(true);
    try {
      setS(await ipc.setActiveProject(id));
      toast.success(id ? "Project is now active." : "No project active.");
    } catch {
      setError("Couldn't change the active project.");
    } finally {
      setBusy(false);
    }
  };

  const editing = openId !== null && draft !== null;
  const isNew = openId === "__novo__";

  return (
    <div data-tab-body="" className="flex min-h-0 flex-1 flex-col gap-[var(--card-gap,1rem)]">
      <ContextInspector />
      {error && <Feedback tone="error">{error}</Feedback>}
      {/* No editor pane exists until a project is selected: an empty container waiting for
          content is the thing this layout work is removing. The list spans the tab until then,
          and the grid splits only when there is something to split for. */}
      <div
        className={cn(
          "grid min-h-0 flex-1 gap-[var(--card-gap,1rem)]",
          editing ? "grid-cols-1 @3xl/settings:grid-cols-[minmax(240px,2fr)_3fr]" : "grid-cols-1",
        )}
      >
        <section
          aria-label="Projects"
          className={cn(
            "flex min-h-0 flex-col rounded-lg border border-[color:var(--border-subtle)] bg-surface-1",
            editing && "hidden @3xl/settings:flex",
          )}
        >
          <div className="flex items-center justify-between gap-3 px-[var(--card-pad,1.25rem)] pb-2 pt-[var(--card-pad,1.25rem)]">
            <div className="flex items-center gap-1.5">
              <h3 className="text-sm font-semibold text-fg">Projects</h3>
              <InfoPopover title="Projects">
                <p>
                  A project's brief rides along with every refine while it's active, so names and
                  wording specific to that work survive. Nothing here leaves your machine on its own.
                </p>
                {s.activeProject && (
                  <p className="mt-2">While a project is active it replaces the focused-window detection from Refining.</p>
                )}
              </InfoPopover>
            </div>
            <Button variant="ghost" size="sm" onClick={startNew} disabled={busy}>
              Add project
            </Button>
          </div>
          <ul data-scroll-pane="" className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
            {s.projects.length === 0 && !isNew && (
              <li className="m-1 rounded-md border border-dashed border-[color:var(--border-subtle)] p-4 text-center text-xs text-fg-muted">
                No projects yet. Add one and write a couple of lines about how text for it should read.
              </li>
            )}
            {isNew && (
              <li className="m-1 rounded-md border border-[color:var(--border-accent)] px-3 py-2 text-xs text-fg">
                New project
              </li>
            )}
            {s.projects.map((p) => {
              const isActive = s.activeProject === p.id;
              const isOpen = openId === p.id;
              // Com o editor aberto, a fila mostra o RASCUNHO e nao o que esta gravado: escolher
              // uma cor ou um icone e nao ver nada mudar ate carregar em Save nao diz se a
              // escolha pegou. Continua a ser so pre-visualizacao; quem grava e o Save.
              const shown = isOpen && draft?.id === p.id ? draft : p;
              const I = iconOf(shown.icon);
              // A cor a medida GANHA ao indice, como no Rust (`resolve_accent`).
              const dot = shown.accentCustom?.trim() || (s.accents[shown.accent] ?? s.accents[0])?.mid;
              return (
                <li key={p.id}>
                  <div
                    className={cn(
                      "flex items-center gap-3 rounded-md px-3 py-2 transition-colors",
                      isOpen ? "bg-surface-2" : "hover:bg-surface-2/60",
                    )}
                  >
                    <span
                      className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg"
                      style={{ background: dot ?? "var(--color-accent)", color: "#1a0e03" }}
                    >
                      <I size={16} weight="bold" />
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
                      size="sm"
                      onClick={() => setActive(isActive ? null : p.id)}
                      disabled={busy}
                      className="shrink-0"
                    >
                      {isActive ? "Deactivate" : "Set active"}
                    </Button>
                    <button
                      type="button"
                      onClick={() => openEditor(p)}
                      disabled={busy}
                      aria-label="Edit"
                      aria-pressed={isOpen}
                      className="flex h-8 w-8 shrink-0 items-center justify-center rounded-sm text-fg-muted transition-colors hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[color:var(--border-accent)]"
                    >
                      <CaretRight size={14} weight="bold" />
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        </section>

        {editing && (
          <section
            aria-label="Project editor"
            data-scroll-pane=""
            className={cn(
              "flex min-h-0 flex-col overflow-y-auto rounded-lg border bg-surface-1",
              isNew ? "border-[color:var(--border-accent)]" : "border-[color:var(--border-subtle)]",
            )}
          >
            {/* Switching projects crossfades the editor; opacity only, so nothing else moves. */}
            <motion.div
              key={openId}
              className="flex min-h-0 flex-1 flex-col"
              initial={still ? false : { opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ duration: still ? 0 : 0.18 }}
            >
              <ProjectEditor
                draft={draft}
                isNew={isNew}
                accents={s.accents}
                wheel={s.accentWheel}
                icons={s.icons}
                scan={scan}
                savedBrief={draft.id ? (s.projects.find((p) => p.id === draft.id)?.brief ?? null) : null}
                onChange={setDraft}
                onSave={save}
                onDelete={() => draft.id && remove(draft.id)}
                onBack={isNew ? discardNew : closeEditor}
                onDiscard={discardNew}
                onPickFolder={pickFolder}
                onUseSubfolder={useSubfolder}
                onDistil={distil}
                onRescan={rescanDraft}
                busy={busy}
                saving={saving}
                distilling={distilling}
              />
            </motion.div>
          </section>
        )}
      </div>
    </div>
  );
}
