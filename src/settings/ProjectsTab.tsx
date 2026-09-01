import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { toast } from "sonner";
import {
  Sparkle,
  Lightning,
  Atom,
  Code,
  Briefcase,
  Flask,
  Rocket,
  Compass,
  Cube,
  Target,
  Book,
  GearSix,
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
import { ipc, type EmberSettings, type Project, type ProjectScan } from "@/lib/ipc";

/**
 * Os nomes dos ícones vêm do Rust (`ember_core::projects::ICONS`); aqui só se mapeia cada nome ao
 * componente. Um nome que o Rust passe a mandar e que não esteja aqui cai no primeiro, em vez de
 * rebentar a lista toda.
 */
const ICON_BY_NAME: Record<string, Icon> = {
  sparkle: Sparkle,
  lightning: Lightning,
  atom: Atom,
  code: Code,
  briefcase: Briefcase,
  flask: Flask,
  rocket: Rocket,
  compass: Compass,
  cube: Cube,
  target: Target,
  book: Book,
  gear: GearSix,
};

function iconOf(name: string): Icon {
  return ICON_BY_NAME[name] ?? Sparkle;
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
  icons,
  onChange,
  onSave,
  onDelete,
  busy,
}: {
  draft: Project;
  accents: EmberSettings["accents"];
  icons: string[];
  onChange: (p: Project) => void;
  onSave: () => void;
  onDelete: () => void;
  busy: boolean;
}) {
  // Apagar em dois passos, sem modal: a app não tem Dialog e não vale a pena construir um para
  // uma confirmação. O botão vira "Really delete?" e volta atrás sozinho.
  const [confirming, setConfirming] = useState(false);

  return (
    <div className="flex flex-col gap-5 border-t border-[color:var(--border-subtle)] px-5 py-5">
      <div className="flex flex-col gap-2">
        <Label htmlFor={`name-${draft.id || "novo"}`}>Name</Label>
        <Input
          id={`name-${draft.id || "novo"}`}
          value={draft.name}
          onChange={(e) => onChange({ ...draft, name: e.target.value })}
          placeholder="e.g. Sintra"
        />
      </div>

      <ChoiceGrid label="Colour">
        {accents.map((a, i) => (
          <button
            key={a.label}
            type="button"
            role="radio"
            aria-checked={draft.accent === i}
            aria-label={a.label}
            title={a.label}
            onClick={() => onChange({ ...draft, accent: i })}
            className={`h-7 w-7 rounded-full border-2 transition-transform hover:scale-110 ${
              draft.accent === i
                ? "border-[color:var(--border-accent)] scale-110"
                : "border-transparent"
            }`}
            style={{ background: a.mid }}
          />
        ))}
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
          Only what changes a rewrite: language and register, names that must stay untouched,
          domain words and their spelling, and a couple of things to avoid. Not architecture.
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-2 pt-1">
        <Button variant="primary" onClick={onSave} disabled={busy || !draft.name.trim()}>
          Save
        </Button>
        {draft.id && (
          <Button
            variant="ghost"
            // Vermelho SO no segundo passo. O primeiro clique ainda nao apaga nada e nao merece
            // alarme; o segundo apaga de vez, e a cor tem de dizer isso antes de o dedo cair.
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
  const [draft, setDraft] = useState<Project | null>(null);
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
    const chosen = await open({ directory: true, multiple: false });
    if (typeof chosen !== "string") return;
    setBusy(true);
    try {
      const r = await ipc.scanProjectFolder(chosen);
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
    setBusy(true);
    try {
      const r = await ipc.scanProjectFolder(path);
      setScan({ ...r, folder: path });
      const base = path.split(/[\/]/).filter(Boolean).pop() ?? "";
      setDraft((d) => (d ? { ...d, name: base, folder: path } : d));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const distil = async () => {
    if (!scan?.folder) return;
    setDistilling(true);
    try {
      const brief = await ipc.distillProject(scan.folder);
      setDraft((d) => (d ? { ...d, brief, sourcePath: scan.sourcePath } : d));
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

  const toggleEditor = (p: Project) => {
    if (openId === p.id) {
      // Fecha SEM limpar o rascunho: se o limpasse aqui, o conteúdo desmontava no mesmo frame e
      // a caixa ficava a encolher vazia. O rascunho só é trocado quando se abre outro projeto.
      setOpenId(null);
      return;
    }
    setDraft({ ...p });
    setOpenId(p.id);
  };

  const save = async () => {
    if (!draft) return;
    setBusy(true);
    try {
      setS(await ipc.saveProject(draft));
      setOpenId(null);
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
        const I = iconOf(p.icon);
        const a = s.accents[p.accent] ?? s.accents[0];
        const isActive = s.activeProject === p.id;
        const isOpen = openId === p.id;
        return (
          <div
            key={p.id}
            className="overflow-hidden rounded-lg border border-[color:var(--border-subtle)] bg-surface-1"
          >
            <div className="flex items-center gap-3 px-5 py-4">
              <span
                className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg"
                style={{ background: a?.mid ?? "var(--color-accent)", color: "#1a0e03" }}
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
                        color: a?.mid ?? "var(--color-accent)",
                        border: `1px solid ${a?.mid ?? "var(--color-accent)"}`,
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
