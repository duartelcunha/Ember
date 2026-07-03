import { m } from "motion/react";

/**
 * O orb (estado "a pensar"): bolinha terracota 2D + glow pulsante por baixo.
 * Partilha `layoutId` com a pilha para o morph. Pulse = scale (compositor).
 */
export function Orb() {
  return (
    <m.div
      layoutId="refiner-surface"
      className="relative grid place-items-center"
      style={{ borderRadius: 9999, width: 17, height: 17 }}
      initial={{ opacity: 0, scale: 0.6 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.6 }}
    >
      <m.div
        className="ember-glow"
        animate={{ opacity: [0.5, 1, 0.5], scale: [0.9, 1.15, 0.9] }}
        transition={{ repeat: Infinity, duration: 1.6, ease: "easeInOut" }}
      />
      <m.div
        className="ember-orb"
        animate={{ scale: [1, 1.1, 1] }}
        transition={{ repeat: Infinity, duration: 1.6, ease: "easeInOut" }}
      />
    </m.div>
  );
}
