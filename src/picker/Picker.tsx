import { useEffect, useState } from "react";
import { AnimatePresence, LazyMotion, domAnimation, m } from "motion/react";
import { listen } from "@tauri-apps/api/event";
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
  Prohibit,
  type Icon,
} from "@phosphor-icons/react";

/**
 * O picker de projetos: uma lista pequena ao lado do cursor, controlada 100% pelo Rust.
 *
 * Este componente é um RENDER PURO do evento `ember://picker`. Não tem estado de seleção
 * próprio, não ouve teclado, não tem handlers de clique: a janela nunca recebe foco (a regra
 * sagrada do overlay: o paste tem de aterrar na app do utilizador) e por isso nunca haveria
 * keydown no DOM. Quem decide é o hook de teclado no Rust; a UI só desenha o que lhe disserem.
 * Se alguém um dia acrescentar `useState` para o índice, as duas fontes vão divergir.
 */

const PICKER_EVENT = "ember://picker";

interface Row {
  id: string | null;
  name: string;
  color: string;
  icon: string;
}

interface PickerState {
  rows: Row[];
  index: number;
  open: boolean;
}

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

/** Espelha `PICKER_ITEM_H`/`PICKER_PAD` do Rust: a janela é dimensionada lá com estes números. */
const ITEM_H = 34;
const PAD = 8;
/** Espelha `PICKER_MAX_VISIBLE`: acima disto a janela não cresce e a lista desliza por índice. */
const MAX_VISIBLE = 9;
/** Espelha `PICKER_HINT_H`. A lista obedece ao rato e ao teclado, e tem de o dizer: a primeira
 *  utilização real acabou com ela aberta oito segundos sem nada acontecer. */
const HINT_H = 20;

export function Picker() {
  const [s, setS] = useState<PickerState>({ rows: [], index: 0, open: false });

  useEffect(() => {
    const un = listen<PickerState>(PICKER_EVENT, (e) => setS(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, []);

  // Janela de índices visível: com mais linhas do que cabem, desliza para conter a seleção.
  // Sem scrollbar: numa lista percorrida só por setas, a scrollbar era ruído.
  const total = s.rows.length;
  const first = Math.min(
    Math.max(0, s.index - (MAX_VISIBLE - 1)),
    Math.max(0, total - MAX_VISIBLE),
  );
  const visible = s.rows.slice(first, first + MAX_VISIBLE);
  const selected = s.rows[s.index];

  return (
    <LazyMotion features={domAnimation} strict>
      <AnimatePresence>
        {s.open && s.rows.length > 0 && (
          <m.div
            key="picker"
            className="ember-bubble flex flex-col"
            style={{ borderRadius: 12, padding: PAD, willChange: "opacity, transform" }}
            initial={{ opacity: 0, scale: 0.96, y: 4 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.97, transition: { duration: 0.1 } }}
            transition={{ type: "spring", stiffness: 600, damping: 34 }}
          >
            {visible.map((row, vi) => {
              const i = first + vi;
              const isSel = i === s.index;
              const I = row.id === null ? Prohibit : (ICON_BY_NAME[row.icon] ?? Sparkle);
              return (
                <div
                  key={row.id ?? "__none__"}
                  className="relative flex items-center gap-2 px-2"
                  style={{ height: ITEM_H }}
                >
                  {/* A pilula deslizante: um so elemento partilhado por `layoutId`, que o motion
                      desliza com spring entre linhas em vez de a fazer saltar. E o detalhe que
                      faz o menu parecer nativo, e custa um div. */}
                  {isSel && (
                    <m.div
                      layoutId="picker-sel"
                      className="absolute inset-x-0 inset-y-[3px] rounded-md"
                      style={{
                        background: `color-mix(in srgb, ${selected?.color ?? "#fd8c3c"} 26%, transparent)`,
                        border: `1px solid color-mix(in srgb, ${selected?.color ?? "#fd8c3c"} 55%, transparent)`,
                      }}
                      transition={{ type: "spring", stiffness: 640, damping: 42 }}
                    />
                  )}
                  <span
                    className="relative z-10 flex h-5 w-5 shrink-0 items-center justify-center rounded-full"
                    style={
                      row.id === null
                        ? { color: "var(--color-fg-muted)" }
                        : { background: row.color, color: "#1a0e03" }
                    }
                  >
                    <I size={12} weight="bold" />
                  </span>
                  <span
                    className={`relative z-10 truncate text-xs ${
                      row.id === null ? "text-fg-muted" : "text-fg"
                    } ${isSel ? "font-semibold" : ""}`}
                  >
                    {row.name}
                  </span>
                </div>
              );
            })}
            <div
              className="flex items-center justify-center gap-1 text-[10px] text-fg-muted"
              style={{ height: HINT_H }}
            >
              <span>click or</span>
              <kbd className="rounded border border-white/15 px-1">↑</kbd>
              <kbd className="rounded border border-white/15 px-1">↓</kbd>
              <span>then</span>
              <kbd className="rounded border border-white/15 px-1">↵</kbd>
            </div>
          </m.div>
        )}
      </AnimatePresence>
    </LazyMotion>
  );
}
