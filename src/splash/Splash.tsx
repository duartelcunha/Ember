import { LazyMotion, domAnimation, m, MotionConfig } from "motion/react";
import type { TargetAndTransition, Transition } from "motion/react";
import { invoke } from "@tauri-apps/api/core";
import iconUrl from "../../src-tauri/icons/128x128.png";

type Mode = "install" | "startup" | "quit";

function currentMode(): Mode {
  const q = window.location.search;
  if (q.includes("mode=quit")) return "quit";
  if (q.includes("mode=startup")) return "startup";
  return "install";
}

/** No fim da animacao: quit sai da app (acopla a saida ao fim real da animacao), instalacao/
 *  arranque so fecham a janela de splash. Silencioso: fora do Tauri (dev) o invoke falha e
 *  nao ha nada a fazer, nem queremos ruido na consola. */
const finish = (mode: Mode) => {
  invoke(mode === "quit" ? "finalize_quit" : "close_splash").catch(() => {});
};

// Animacoes SO-compositor: transform (scale/rotate) + opacity. Sem filtros animados
// (blur/drop-shadow), que re-rasterizam cada frame em ecra inteiro e sao o pior dreno de fps
// desta app. A profundidade vem de um brilho (gradiente radial) cujo unico movimento e a
// opacidade e a escala. Reduced-motion mantem so o fade (o transform e neutralizado), e o
// onAnimationComplete continua a fechar a janela.
type IconAnim = { initial: TargetAndTransition; animate: TargetAndTransition; transition: Transition };

const ICON: Record<Mode, IconAnim> = {
  install: {
    initial: { opacity: 0, scale: 0.6 },
    animate: { opacity: [0, 1, 1, 0], scale: [0.6, 1.08, 1, 0.96] },
    transition: { duration: 1.7, times: [0, 0.28, 0.72, 1], ease: [0.22, 1, 0.36, 1] },
  },
  startup: {
    initial: { opacity: 0, scale: 0.9 },
    animate: { opacity: [0, 1, 1, 0], scale: [0.9, 1.03, 1, 0.98] },
    transition: { duration: 0.95, times: [0, 0.3, 0.7, 1], ease: [0.22, 1, 0.36, 1] },
  },
  quit: {
    initial: { opacity: 1, scale: 1 },
    animate: { opacity: [1, 1, 0], scale: [1, 1.06, 0.82], rotate: [0, 0, -8] },
    transition: { duration: 0.5, times: [0, 0.35, 1], ease: [0.4, 0, 1, 1] },
  },
};

const GLOW: Record<Mode, { initial: TargetAndTransition; animate: TargetAndTransition }> = {
  install: {
    initial: { opacity: 0, scale: 0.7 },
    animate: { opacity: [0, 0.9, 0.85, 0], scale: [0.7, 1.15, 1.05, 0.95] },
  },
  startup: {
    initial: { opacity: 0, scale: 0.8 },
    animate: { opacity: [0, 0.7, 0.65, 0], scale: [0.8, 1.1, 1.02, 0.95] },
  },
  quit: {
    initial: { opacity: 0.6, scale: 1 },
    animate: { opacity: [0.6, 0.5, 0], scale: [1, 1.1, 0.85] },
  },
};

export default function Splash() {
  const mode = currentMode();
  const icon = ICON[mode];
  const glow = GLOW[mode];
  return (
    <LazyMotion features={domAnimation} strict>
      <MotionConfig reducedMotion="user">
        <div className="grid h-screen w-screen place-items-center overflow-hidden bg-transparent">
          <div className="relative grid place-items-center">
            <m.div
              aria-hidden
              className="pointer-events-none absolute h-48 w-48 rounded-full"
              style={{
                background:
                  "radial-gradient(circle, rgba(255,122,24,0.45) 0%, rgba(255,122,24,0) 70%)",
                willChange: "transform, opacity",
              }}
              initial={glow.initial}
              animate={glow.animate}
              transition={icon.transition}
            />
            <m.img
              src={iconUrl}
              alt="Ember"
              className="h-32 w-32 select-none"
              style={{ willChange: "transform, opacity" }}
              initial={icon.initial}
              animate={icon.animate}
              transition={icon.transition}
              onAnimationComplete={() => finish(mode)}
            />
          </div>
        </div>
      </MotionConfig>
    </LazyMotion>
  );
}
