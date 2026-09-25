import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LazyMotion, MotionConfig, domAnimation, m, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { Copy, GearSix, Power } from "@phosphor-icons/react";
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

/** Mirrors `POPUP` in src-tauri/src/tray.rs: the recoverable row adds exactly ITEM_H. */
const ITEM_H = 34;
const HEADER_H = 36;
const PAD = 8;

const ITEMS = [
  { id: "settings", label: "Settings", Icon: GearSix },
  { id: "quit", label: "Quit Ember", Icon: Power },
] as const;
const RECOVERY_ITEMS = [
  { id: "copy-result", label: "Copy blocked result", Icon: Copy },
  ...ITEMS,
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
  const [resultReady, setResultReady] = useState(false);
  const [copyError, setCopyError] = useState(false);
  const items = resultReady ? RECOVERY_ITEMS : ITEMS;
  // One choice per opening. A held Enter repeats; the repeats find this set and do nothing.
  const acted = useRef(false);
  const menu = useRef<HTMLDivElement>(null);
  // The phase as the event listener sees it. The listener is registered once, so it would close
  // over the phase of the first render; this is the value it has to read to tell an assertion of
  // the open state from a real opening.
  const phaseNow = useRef<Phase>("closed");
  phaseNow.current = phase;

  useEffect(() => {
    let disposed = false;
    const apply = (open: boolean, isBelow = false, hasResult = false) => {
      if (disposed) return;
      if (open) {
        setResultReady(hasResult);
        setCopyError(false);
        // An `open` for a menu already on screen is Rust asserting the state, not a new opening:
        // it sends one on every click of the icon so a surface that folded itself away while the
        // window stayed up can always be brought back. Restarting here would replay the entrance
        // and throw away the highlighted item under the pointer, so an open menu only keeps
        // drawing. Recovery is the branch below: from "closed" or "leaving" this reopens.
        if (phaseNow.current === "open") {
          setIndex((current) => Math.min(current, hasResult ? RECOVERY_ITEMS.length - 1 : ITEMS.length - 1));
          return;
        }
        acted.current = false;
        setIndex(0);
        setBelow(isBelow);
        setGeneration((g) => g + 1);
        setPhase("open");
      } else {
        setPhase((p) => (p === "open" ? "leaving" : p));
      }
    };
    const un = listen<{ open: boolean; below?: boolean; resultReady?: boolean }>(EVENT,
      (e) => apply(e.payload.open, e.payload.below === true, e.payload.resultReady === true));
    // The first click can arrive before this listener exists; ask whether it did.
    void un
      .then(() => Promise.all([
        invoke<boolean>("tray_action", { action: "ready" }),
        invoke<boolean>("tray_action", { action: "recovery-available" }),
      ]))
      .then(([open, hasResult]) => { if (open) apply(true, false, hasResult); })
      .catch(() => {});
    return () => {
      disposed = true;
      void un.then((stop) => stop()).catch(() => {});
    };
  }, []);

  const act = (action: string) => {
    if (phase !== "open" || acted.current) return;
    acted.current = true;
    if (action === "copy-result") {
      void invoke<boolean>("tray_action", { action }).then((copied) => {
        if (!copied) { acted.current = false; setCopyError(true); }
      }).catch(() => { acted.current = false; setCopyError(true); });
    } else {
      void invoke("tray_action", { action }).catch(() => {});
    }
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
          setIndex((i) => (i + 1) % items.length);
          break;
        case "ArrowUp":
          e.preventDefault();
          setIndex((i) => (i + items.length - 1) % items.length);
          break;
        case "Enter":
          e.preventDefault();
          act(items[index].id);
          break;
        case "Escape":
          e.preventDefault();
          act("close");
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [phase, index, items]);

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
              aria-activedescendant={`tray-item-${items[index].id}`}
              tabIndex={0}
              className="ember-bubble flex w-full flex-col outline-none"
              style={{ borderRadius: 12, padding: PAD }}
            >
              <div className="flex items-center gap-2 px-2 pb-1" style={{ height: HEADER_H }}>
                <Logo size={18} />
                <span className="text-sm font-semibold text-fg">Ember</span>
              </div>
              <div className="relative" style={{ height: items.length * ITEM_H }}>
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
                {items.map(({ id, label, Icon }, i) => (
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
                    <span className="truncate">{id === "copy-result" && copyError ? "Copy unavailable. Retry" : label}</span>
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
