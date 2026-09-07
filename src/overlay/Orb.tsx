import { m } from "motion/react";
import { useSyncExternalStore } from "react";
import { ORB_INK } from "../components/floatingGeometry";

const SPARK_SIZE = 40;

const PX = 3;

// Shared visible bounds anchor the ring next to the cursor. The ring is eight dots on a
// 5x5 grid of 3px cells; the cells are the geometry the tests and the cursor anchor measure,
// so the dots can change shape without anything else moving.
const GRID = ORB_INK;
/** Dot radius. Slightly over half a cell, so neighbours read as a ring and not as beads. */
const DOT_R = 1.6;

const RING = [
  [2, 0], // top
  [3, 1], // top right
  [4, 2], // right
  [3, 3], // bottom right
  [2, 4], // bottom
  [1, 3], // bottom left
  [0, 2], // left
  [1, 1], // top left
] as const;

const CENTER = { x: GRID.x + PX * 2.5, y: GRID.y + PX * 2.5 };

const HEAT = {
  values: "10;10.6;12.5;15",
  times: "0;0.2;0.5;1",
  splines: "0.4 0 0.6 1; 0.4 0 0.6 1; 0.4 0 0.6 1",
  over: 30,
};

const VARIANT = {
  work: { chase: 0.8 },
  retry: { chase: 0.5 },
} as const;

function usePrefersReducedMotion() {
  return useSyncExternalStore(
    (onChange) => {
      const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
      mq.addEventListener("change", onChange);
      return () => mq.removeEventListener("change", onChange);
    },
    () => window.matchMedia("(prefers-reduced-motion: reduce)").matches,
    () => false,
  );
}

function css(chase: number) {
  const steps = RING.map(
    (_, i) => `.ember-px-${i}{animation:ember-chase ${chase}s ease-in-out ${(
      (chase / RING.length) *
      i
    ).toFixed(3)}s infinite}`,
  ).join("");
  // A rounded pulse travelling around the ring, not a flash followed by a long fade. The old
  // 0% -> 1% jump was what made the ring read as pixels blinking; with the dots round, the
  // same jump reads as beads switching on. Ease-in-out on both flanks keeps it soft.
  return `@keyframes ember-chase{0%{opacity:.18}14%{opacity:1}55%{opacity:.18}100%{opacity:.18}}${steps}
@media (prefers-reduced-motion: reduce){[class^="ember-px-"]{animation:none}}`;
}

export function Orb({ variant = "work" }: { variant?: keyof typeof VARIANT }) {
  const v = VARIANT[variant];
  const still = usePrefersReducedMotion();

  return (
    <m.div
      className="relative shrink-0"
      style={{
        width: SPARK_SIZE,
        height: SPARK_SIZE,
        willChange: "opacity",
        transformOrigin: `${(CENTER.x / SPARK_SIZE) * 100}% ${(CENTER.y / SPARK_SIZE) * 100}%`,
      }}
      initial={{ opacity: still ? 1 : 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0, transition: { duration: 0.1 } }}
      transition={{
        opacity: { duration: still ? 0 : 0.18, ease: "easeOut" },
      }}
    >
      <svg
        width={SPARK_SIZE}
        height={SPARK_SIZE}
        viewBox="0 0 40 40"
        fill="none"
        aria-hidden
        style={{
          filter: "drop-shadow(0 0 1.5px rgba(0,0,0,0.55))",
        }}
      >
        <style>{css(v.chase)}</style>
        <defs>
          <radialGradient id="ember-heat">
            <stop offset="0%" stopColor="var(--color-accent)" stopOpacity="0.26" />
            <stop offset="45%" stopColor="var(--color-accent)" stopOpacity="0.1" />
            <stop offset="100%" stopColor="var(--color-accent)" stopOpacity="0" />
          </radialGradient>
        </defs>

        <circle cx={CENTER.x} cy={CENTER.y} r={10} fill="url(#ember-heat)">
          {!still && (
            <animate
              attributeName="r"
              values={HEAT.values}
              keyTimes={HEAT.times}
              keySplines={HEAT.splines}
              dur={`${HEAT.over}s`}
              begin="0s"
              repeatCount="1"
              fill="freeze"
              calcMode="spline"
            />
          )}
        </circle>

        {RING.map(([col, row], i) => (
          <circle
            key={i}
            className={`ember-px-${i}`}
            cx={GRID.x + col * PX + PX / 2}
            cy={GRID.y + row * PX + PX / 2}
            r={DOT_R}
            fill="var(--color-accent)"
            shapeRendering="geometricPrecision"
            opacity={1 - i * 0.11}
          />
        ))}
      </svg>
    </m.div>
  );
}
