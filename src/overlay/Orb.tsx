import { m } from "motion/react";
import { useSyncExternalStore } from "react";
import { orbInk, ORB_PX } from "../components/floatingGeometry";

const SPARK_SIZE = 40;

/**
 * The eight cells of the ring, as coordinates on a 5x5 grid.
 *
 * The grid is the geometry: the cursor anchor, the morph and the floating tests all measure the
 * ink box the grid fills, never the artwork inside it. That is what lets the skin and the size
 * change without a single placement constant moving.
 */
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

const VARIANT = {
  work: { chase: 0.8 },
  retry: { chase: 0.5 },
} as const;

export type OrbSkin = "ember" | "pulse" | "ring";

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

/**
 * What sits beside the cursor while the model answers.
 *
 * Three skins and three sizes, all drawing inside the same ink box: `orbInk(px)` is the single
 * place a size exists, and every skin fills it. Sizes are whole multiples of the drawing pixel
 * (2, 3 or 4, so 10, 15 or 20px of ink) because the mark is pixel art and a fractional multiple
 * puts its edges between two screen pixels, where anti-aliasing turns it into a smudge.
 *
 * Motion is SMIL rather than CSS keyframes for the pulse and the ring. The heat glow already
 * uses it here, and two orbs on screen at once (the settings preview draws real ones) would
 * otherwise need generated class names to stop sharing a keyframe.
 */
export function Orb({
  variant = "work",
  skin = "ember",
  px = ORB_PX,
}: {
  variant?: keyof typeof VARIANT;
  skin?: OrbSkin;
  px?: number;
}) {
  const v = VARIANT[variant];
  const still = usePrefersReducedMotion();
  const ink = orbInk(px);
  const centre = { x: ink.x + ink.width / 2, y: ink.y + ink.height / 2 };
  // Every measurement below is a ratio of the drawing pixel, so all three sizes are the same
  // artwork rather than three drawings that happen to look alike.
  const dotR = px * 0.533;
  const heatR = px * (10 / 3);
  const period = v.chase;

  return (
    <m.div
      className="relative shrink-0"
      style={{
        width: SPARK_SIZE,
        height: SPARK_SIZE,
        willChange: "opacity",
        transformOrigin: `${(centre.x / SPARK_SIZE) * 100}% ${(centre.y / SPARK_SIZE) * 100}%`,
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
        viewBox={`0 0 ${SPARK_SIZE} ${SPARK_SIZE}`}
        fill="none"
        aria-hidden
        data-orb-skin={skin}
        style={{
          filter: "drop-shadow(0 0 1.5px rgba(0,0,0,0.55))",
        }}
      >
        {skin === "ember" && <style>{css(period)}</style>}
        <defs>
          <radialGradient id="ember-heat">
            <stop offset="0%" stopColor="var(--color-accent)" stopOpacity="0.26" />
            <stop offset="45%" stopColor="var(--color-accent)" stopOpacity="0.1" />
            <stop offset="100%" stopColor="var(--color-accent)" stopOpacity="0" />
          </radialGradient>
        </defs>

        {/* The heat is the one thing all three share: it is what says "warm" before the shape
            says anything at all, and it is what the accent colour of an active project rides on. */}
        <circle cx={centre.x} cy={centre.y} r={heatR} fill="url(#ember-heat)">
          {!still && (
            <animate
              attributeName="r"
              values={`${heatR};${heatR * 1.06};${heatR * 1.25};${heatR * 1.5}`}
              keyTimes="0;0.2;0.5;1"
              keySplines="0.4 0 0.6 1; 0.4 0 0.6 1; 0.4 0 0.6 1"
              dur="30s"
              begin="0s"
              repeatCount="1"
              fill="freeze"
              calcMode="spline"
            />
          )}
        </circle>

        {skin === "ember" &&
          RING.map(([col, row], i) => (
            <circle
              key={i}
              className={`ember-px-${i}`}
              cx={ink.x + col * px + px / 2}
              cy={ink.y + row * px + px / 2}
              r={dotR}
              fill="var(--color-accent)"
              shapeRendering="geometricPrecision"
              opacity={1 - i * 0.11}
            />
          ))}

        {/* Pulse: one dot breathing. For anyone who wants to know a refine is running without
            being invited to watch it. */}
        {skin === "pulse" && (
          <circle cx={centre.x} cy={centre.y} r={px} fill="var(--color-accent)">
            {!still && (
              <>
                <animate
                  attributeName="r"
                  values={`${px * 0.72};${px * 1.15};${px * 0.72}`}
                  dur={`${period * 1.6}s`}
                  repeatCount="indefinite"
                  calcMode="spline"
                  keyTimes="0;0.5;1"
                  keySplines="0.4 0 0.6 1; 0.4 0 0.6 1"
                />
                <animate
                  attributeName="opacity"
                  values="0.45;1;0.45"
                  dur={`${period * 1.6}s`}
                  repeatCount="indefinite"
                  calcMode="spline"
                  keyTimes="0;0.5;1"
                  keySplines="0.4 0 0.6 1; 0.4 0 0.6 1"
                />
              </>
            )}
          </circle>
        )}

        {/* Ring: a faint track with one arc going round it. The most neutral of the three, and
            the one that reads as a spinner in any application it lands on. */}
        {skin === "ring" && (
          <>
            <circle
              cx={centre.x}
              cy={centre.y}
              r={px * 2}
              fill="none"
              stroke="var(--color-accent)"
              strokeOpacity="0.22"
              strokeWidth={px * 0.5}
            />
            <circle
              cx={centre.x}
              cy={centre.y}
              r={px * 2}
              fill="none"
              stroke="var(--color-accent)"
              strokeWidth={px * 0.5}
              strokeLinecap="round"
              // A quarter of the circumference lit, the rest open.
              strokeDasharray={`${Math.PI * px * 2 * 0.5} ${Math.PI * px * 4}`}
              transform={still ? undefined : `rotate(-90 ${centre.x} ${centre.y})`}
            >
              {!still && (
                <animateTransform
                  attributeName="transform"
                  type="rotate"
                  from={`0 ${centre.x} ${centre.y}`}
                  to={`360 ${centre.x} ${centre.y}`}
                  dur={`${period * 1.4}s`}
                  repeatCount="indefinite"
                />
              )}
            </circle>
          </>
        )}
      </svg>
    </m.div>
  );
}
