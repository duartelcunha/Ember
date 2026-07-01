import { AnimatePresence, domAnimation, LazyMotion, MotionConfig } from "motion/react";
import { useOverlayState } from "./useOverlayController";
import { Orb } from "./Orb";
import { Pill } from "./Pill";

/** Raiz do overlay junto ao cursor: orb (refining) ou pilha (success/error/hint). */
export function Overlay() {
  const s = useOverlayState();
  return (
    <LazyMotion features={domAnimation} strict>
      <MotionConfig reducedMotion="user">
        <div className="grid h-screen place-items-center p-2">
          <AnimatePresence mode="popLayout">
            {s.phase === "refining" && <Orb key="orb" />}
            {s.phase === "success" && <Pill key="ok" kind="success" text="Refined" />}
            {s.phase === "error" && (
              <Pill key="err" kind="error" text={s.message ?? "Something went wrong."} />
            )}
            {s.phase === "hint" && (
              <Pill key="hint" kind="hint" text={s.message ?? "Select text first"} />
            )}
          </AnimatePresence>
        </div>
      </MotionConfig>
    </LazyMotion>
  );
}
