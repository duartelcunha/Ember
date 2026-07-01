import { m } from "motion/react";

/**
 * O orb flat (estado a refinar): bolinha laranja 2D + anel spinner a rodar.
 * Partilha `layoutId` com a pilha para o morph. Pulse = scale (compositor).
 */
export function Orb() {
  return (
    <m.div
      layoutId="refiner-surface"
      className="relative grid place-items-center"
      style={{ borderRadius: 9999, width: 40, height: 40 }}
      initial={{ opacity: 0, scale: 0.6 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.6 }}
    >
      <m.div
        className="ember-ring"
        animate={{ rotate: 360 }}
        transition={{ repeat: Infinity, duration: 0.9, ease: "linear" }}
      />
      <m.div
        className="ember-orb"
        animate={{ scale: [1, 1.12, 1] }}
        transition={{ repeat: Infinity, duration: 1.2, ease: "easeInOut" }}
      />
    </m.div>
  );
}
