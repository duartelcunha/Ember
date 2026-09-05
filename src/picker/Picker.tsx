import { invoke } from "@tauri-apps/api/core";
import { ICON_BY_NAME } from "../components/projectIcons";
import { useEffect, useState } from "react";
import { AnimatePresence, LazyMotion, MotionConfig, domAnimation, m, useReducedMotion } from "motion/react";
import { useFloatingPosition } from "../components/useFloatingPosition";
import { listen } from "@tauri-apps/api/event";
import {
  Sparkle,
  Prohibit,
} from "@phosphor-icons/react";

/**
 * O picker de projetos: uma lista pequena colada ao ponteiro, controlada 100% pelo Rust.
 *
 * Este componente é um RENDER PURO dos eventos `ember://picker` (o que mostrar) e
 * `ember://picker-at` (onde). Não tem estado de seleção próprio, não ouve teclado, não tem
 * handlers de clique: a janela nunca recebe foco (a regra sagrada do overlay: o paste tem de
 * aterrar na app do utilizador) e por isso nunca haveria keydown no DOM. Quem decide é o hook no
 * Rust; a UI só desenha o que lhe disserem. Se alguém um dia acrescentar `useState` para o
 * índice, as duas fontes vão divergir.
 *
 * A JANELA cobre o monitor e está quieta: o que anda é esta lista, por `transform`. Mover a
 * janela era o caminho óbvio e rangia, porque arrastar uma superfície com `backdrop-filter`
 * obriga o compositor a re-amostrar o desfoque a cada frame. Pela mesma razão, enquanto o
 * ponteiro anda o vidro é trocado por um fundo sólido, e só volta quando ele pára.
 */

const PICKER_EVENT = "ember://picker";
const PICKER_AT_EVENT = "ember://picker-at";

interface Row {
  id: string | null;
  automatic?: boolean;
  name: string;
  color: string;
  icon: string;
}

interface PickerState {
  sequence?: number;
  rows: Row[];
  index: number;
  open: boolean;
  /** Só no fecho por escolha: a linha que ficou escolhida, para o fecho o mostrar. */
  chosen: number | null;
}



/** Espelha `PICKER_ITEM_H`/`PICKER_PAD` do Rust: a janela é dimensionada lá com estes números. */
const ITEM_H = 34;
const PAD = 8;
/** Espelha `PICKER_MAX_VISIBLE`: acima disto a lista não cresce e desliza por índice. */
const MAX_VISIBLE = 9;
/** Espelha `PICKER_HINT_H`. A lista obedece à roda e ao teclado, e tem de o dizer: a primeira
 *  utilização real acabou com ela aberta oito segundos sem nada acontecer. O ponteiro não aponta
 *  linhas (a lista anda com ele), por isso a ajuda não pode prometer isso. */
const HINT_H = 20;
/** Quanto tempo depois do último movimento é que o vidro volta. Curto de mais e ele volta entre
 *  dois movimentos da mesma passagem; longo de mais e nota-se que a lista ficou opaca. */

export function Picker() {
  const still = useReducedMotion();
  const [s, setS] = useState<PickerState>({
    rows: [],
    index: 0,
    open: false,
    chosen: null,
  });
  const floating = useFloatingPosition(PICKER_AT_EVENT);

  useEffect(() => {
    let disposed = false;
    const accept = (next: PickerState | null) => {
      if (next && !disposed) setS((current) => (next.sequence ?? 0) > (current.sequence ?? -1) ? next : current);
    };
    const un = listen<PickerState>(PICKER_EVENT, (e) => accept(e.payload));
    void un.then(() => invoke<PickerState | null>("picker_snapshot")).then(accept).catch(() => {});
    return () => {
      disposed = true;
      un.then((f) => f());
    };
  }, []);

  // Janela de índices visível: com mais linhas do que cabem, desliza para conter a seleção.
  // Sem scrollbar: numa lista percorrida por setas e roda, a scrollbar era ruído.
  const [viewportHeight, setViewportHeight] = useState(window.innerHeight);
  useEffect(() => {
    const resize = () => setViewportHeight(window.innerHeight);
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);
  const maxVisible = Math.max(1, Math.min(MAX_VISIBLE, Math.floor((viewportHeight - 32 - PAD * 2 - HINT_H) / ITEM_H)));
  const total = s.rows.length;
  const first = Math.min(
    Math.max(0, s.index - (maxVisible - 1)),
    Math.max(0, total - maxVisible),
  );
  const visible = s.rows.slice(first, first + maxVisible);
  const selected = s.rows[s.index];
  // O fecho por escolha: a lista já está a fechar, mas tem de se ver O QUE foi escolhido.
  const chosen = !s.open && s.chosen !== null ? s.chosen : null;
  const accent = selected?.color ?? "#fd8c3c";

  return (
    <LazyMotion features={domAnimation} strict>
      <MotionConfig reducedMotion="user">
      <div
        ref={floating}
        className="ember-floating absolute left-0 top-0 max-w-[calc(100vw-16px)]"
      >
        <AnimatePresence>
          {s.open && s.rows.length > 0 && (
            <m.div
              key="picker"
              role="listbox"
              aria-label="Project context"
              aria-activedescendant={`project-option-${s.index}`}
              className="ember-bubble relative flex max-w-[calc(100vw-16px)] flex-col"
              style={{
                borderRadius: 12,
                padding: PAD,
                width: 240,
                willChange: "opacity",
                // O vidro sai de cena enquanto a lista anda: `backdrop-filter` re-amostra o
                // fundo a cada frame do movimento, e é isso que se vê como ranger.

              }}
              initial={still ? false : { opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={
                still ? { opacity: 0, transition: { duration: 0 } } : chosen !== null
                  ? // Confirm the choice with colour and a short fade, within the measured bounds.
                    {
                      opacity: [1, 1, 0],
                      transition: { duration: 0.34, times: [0, 0.45, 1] },
                    }
                  : { opacity: 0, transition: { duration: 0.1 } }
              }
              transition={still ? { duration: 0 } : { type: "spring", stiffness: 600, damping: 34 }}
            >
              {/* O flash da cor do projeto escolhido, por cima da lista inteira. Existe para a
                  escolha ter cor: é a mesma que passa a pintar o orb a seguir. */}
              {chosen !== null && !still && (
                <m.div
                  className="pointer-events-none absolute inset-0"
                  style={{ borderRadius: 12, background: s.rows[chosen]?.color ?? accent }}
                  initial={still ? false : { opacity: 0 }}
                  animate={{ opacity: [0, 0.32, 0] }}
                  transition={{ duration: 0.34, times: [0, 0.4, 1] }}
                />
              )}
              {visible.map((row, vi) => {
                const i = first + vi;
                const isSel = i === s.index;
                const isChosen = chosen === i;
                const I = row.id === null && !row.automatic ? Prohibit : (ICON_BY_NAME[row.icon] ?? Sparkle);
                return (
                  <m.div
                    key={row.automatic ? "__auto__" : row.id ?? "__none__"}
                    id={`project-option-${i}`}
                    role="option"
                    aria-selected={isSel}
                    className="relative flex items-center gap-2 px-2"
                    style={{ height: ITEM_H }}
                    animate={
                      chosen === null
                        ? { opacity: 1 }
                        : // As outras saem de cena para a escolhida ficar sozinha.
                          { opacity: isChosen ? 1 : 0 }
                    }
                    transition={{ duration: still ? 0 : 0.16 }}
                  >
                    {/* A pilula deslizante: um so elemento partilhado por `layoutId`, que o motion
                        desliza com spring entre linhas em vez de a fazer saltar. E o detalhe que
                        faz o menu parecer nativo, e custa um div. */}
                    {isSel && (
                      <m.div
                        layoutId="picker-sel"
                        className="absolute inset-x-0 inset-y-[3px] rounded-md"
                        style={{
                          background: `color-mix(in srgb, ${accent} 26%, transparent)`,
                          border: `1px solid color-mix(in srgb, ${accent} 55%, transparent)`,
                        }}
                        transition={still ? { duration: 0 } : { type: "spring", stiffness: 640, damping: 42 }}
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
                  </m.div>
                );
              })}
              <m.div
                className="flex items-center justify-center gap-1 text-[10px] text-fg-muted"
                style={{ height: HINT_H }}
                animate={{ opacity: chosen === null ? 1 : 0 }}
                transition={{ duration: still ? 0 : 0.12 }}
              >
                <span>scroll or</span>
                <kbd className="rounded border border-white/15 px-1">↑</kbd>
                <kbd className="rounded border border-white/15 px-1">↓</kbd>
                <span>then click or</span>
                <kbd className="rounded border border-white/15 px-1">↵</kbd>
              </m.div>
            </m.div>
          )}
        </AnimatePresence>
      </div>
      </MotionConfig>
    </LazyMotion>
  );
}
