import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LazyMotion, MotionConfig, domAnimation, m, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { GearSix, Power } from "@phosphor-icons/react";
import { Logo } from "../components/Logo";

/**
 * The tray menu. A render of `ember://tray` (`{ open }`) from the Rust side, which owns when it
 * opens, where it goes and when the window hides; this only draws it and reports the choice
 * through `tray_action`.
 *
 * Unlike the picker, this window has focus, so it hears its own keyboard: arrows move the
 * selection, Enter and click choose, Esc closes. The `role=menu` node holds DOM focus and points
 * at the active item through `aria-activedescendant`, which is what a screen reader follows as
 * the arrows move; the Win32 menu this replaces announced its items, and so must this. The
 * selection pill is the same element the picker uses, on the same spring.
 *
 * It opens out of the icon: the surface carries `data-enter` with the bottom-edge variant of
 * `ember-surface-open` (see `.ember-tray` in globals.css) and folds back with `data-leave`
 * before Rust hides the window. Nothing here animates `transform` on the surface itself.
 */

const EVENT = "ember://tray";

/** Mirrors `POPUP` in src-tauri/src/tray.rs and the "tray" window in tauri.conf.json: header +
 *  rows + padding + the outer inset must add up to the window's 128px, or the surface is cut. */
const ITEM_H = 34;
const HEADER_H = 36;
const PAD = 8;

const ITEMS = [
  { id: "settings", label: "Settings", Icon: GearSix },
  { id: "quit", label: "Quit Ember", Icon: Power },
] as const;

type Phase = "closed" | "open" | "leaving";

export function TrayMenu() {
  const still = useReducedMotion();
  const [phase, setPhase] = useState<Phase>("closed");
  // Bumped on every open so the surface is a NEW node and its entrance runs again.
  const [generation, setGeneration] = useState(0);
  const [index, setIndex] = useState(0);
  // Which edge faces the icon (Rust decides: a taskbar at the top puts the menu below it).
  const [below, setBelow] = useState(false);
  // One choice per opening. A held Enter repeats; the repeats find this set and do nothing.
  const acted = useRef(false);
  const menu = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let disposed = false;
    const apply = (open: boolean, isBelow = false) => {
      if (disposed) return;
      if (open) {
        acted.current = false;
        setIndex(0);
        setBelow(isBelow);
        setGeneration((g) => g + 1);
        setPhase("open");
      } else {
        setPhase((p) => (p === "open" ? "leaving" : p));
      }
    };
    const un = listen<{ open: boolean; below?: boolean }>(EVENT, (e) => apply(e.payload.open, e.payload.below === true));
    // The first click can arrive before this listener exists; ask whether it did.
    void un
      .then(() => invoke<boolean>("tray_action", { action: "ready" }))
      .then((open) => { if (open) apply(true); })
      .catch(() => {});
    return () => {
      disposed = true;
      void un.then((stop) => stop()).catch(() => {});
    };
  }, []);

  const act = (action: string) => {
    if (phase !== "open" || acted.current) return;
    acted.current = true;
    void invoke("tray_action", { action }).catch(() => {});
  };

  // The menu node takes focus when it opens, so the keyboard and the screen reader have a
  // composite to follow. Under reduced motion the leave animation never ends (there is none), so
  // a timer stands in for `animationend`; Rust hides the window either way.
  useEffect(() => {
    if (phase === "open") menu.current?.focus({ preventScroll: true });
    if (phase !== "leaving") return;
    const timer = window.setTimeout(() => setPhase("closed"), 300);
    return () => window.clearTimeout(timer);
  }, [phase, generation]);

  useEffect(() => {
    if (phase !== "open") return;
    const onKey = (e: KeyboardEvent) => {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setIndex((i) => (i + 1) % ITEMS.length);
          break;
        case "ArrowUp":
          e.preventDefault();
          setIndex((i) => (i + ITEMS.length - 1) % ITEMS.length);
          break;
        case "Enter":
          e.preventDefault();
          act(ITEMS[index].id);
          break;
        case "Escape":
          e.preventDefault();
          act("close");
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [phase, index]);

  const slide = still ? { duration: 0 } : ({ type: "spring", stiffness: 640, damping: 42 } as const);

  return (
    <LazyMotion features={domAnimation} strict>
      <MotionConfig reducedMotion="user">
        <div className={`ember-tray fixed inset-0 flex p-1 ${below ? "items-start" : "items-end"}`} data-side={below ? "below" : "above"}>
          {phase !== "closed" && (
            <div
              key={generation}
              ref={menu}
              data-enter=""
              {...(phase === "leaving" ? { "data-leave": "" } : {})}
              onAnimationEnd={(e) => {
                if (e.animationName === "ember-surface-close") setPhase("closed");
              }}
              role="menu"
              aria-label="Ember"
              aria-activedescendant={`tray-item-${ITEMS[index].id}`}
              tabIndex={0}
              className="ember-bubble flex w-full flex-col outline-none"
              style={{ borderRadius: 12, padding: PAD }}
            >
              <div className="flex items-center gap-2 px-2 pb-1" style={{ height: HEADER_H }}>
                <Logo size={18} />
                <span className="text-sm font-semibold text-fg">Ember</span>
              </div>
              <div className="relative" style={{ height: ITEMS.length * ITEM_H }}>
                <m.div
                  aria-hidden
                  className="absolute inset-x-0 rounded-md"
                  style={{
                    height: ITEM_H - 6,
                    background: "color-mix(in srgb, var(--color-accent) 22%, transparent)",
                    border: "1px solid color-mix(in srgb, var(--color-accent) 50%, transparent)",
                  }}
                  initial={false}
                  animate={{ y: index * ITEM_H + 3 }}
                  transition={slide}
                />
                {ITEMS.map(({ id, label, Icon }, i) => (
                  <button
                    key={id}
                    id={`tray-item-${id}`}
                    type="button"
                    role="menuitem"
                    data-active={i === index ? "" : undefined}
                    tabIndex={-1}
                    className={`relative z-10 flex w-full items-center gap-2 rounded-md px-2 text-left text-xs ${
                      i === index ? "font-semibold text-fg" : "text-fg"
                    }`}
                    style={{ height: ITEM_H }}
                    onMouseEnter={() => setIndex(i)}
                    onClick={() => act(id)}
                  >
                    <Icon size={14} weight="bold" className="shrink-0 text-fg-muted" />
                    <span className="truncate">{label}</span>
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </MotionConfig>
    </LazyMotion>
  );
}
