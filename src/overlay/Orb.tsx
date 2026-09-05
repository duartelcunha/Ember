import { m } from "motion/react";
import { useSyncExternalStore } from "react";
import { ORB_INK } from "../components/floatingGeometry";

const SPARK_SIZE = 40;

const PX = 3;

// Shared visible bounds anchor the original pixel artwork next to the cursor.
const GRID = ORB_INK;

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
  return `@keyframes ember-chase{0%{opacity:0}1%{opacity:1}100%{opacity:0}}${steps}
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
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0, transition: { duration: 0.1 } }}
      transition={{
        opacity: { duration: 0.18, ease: "easeOut" },
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
          <rect
            key={i}
            className={`ember-px-${i}`}
            x={GRID.x + col * PX}
            y={GRID.y + row * PX}
            width={PX}
            height={PX}
            fill="var(--color-accent)"
            shapeRendering="crispEdges"
            opacity={1 - i * 0.11}
          />
        ))}
      </svg>
    </m.div>
  );
}
