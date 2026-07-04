import { m } from "motion/react";

/**
 * O orb (estado "a pensar"): bolinha terracota 2D + glow pulsante por baixo. Orb <-> pilula
 * fazem crossfade (nao morph): a pilula tem backdrop-filter e um morph arrasta-a-blur cada
 * frame (re-amostra o fundo). O pulse aqui e so scale/opacity (o orb nao tem blur).
 */
export function Orb() {
  return (
    <m.div
      className="relative grid place-items-center"
      style={{ borderRadius: 9999, width: 17, height: 17 }}
      initial={{ opacity: 0, scale: 0.6 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.6 }}
    >
      <m.div
        className="ember-glow"
        animate={{ opacity: [0.3, 1, 0.3], scale: [0.85, 1.25, 0.85] }}
        transition={{ repeat: Infinity, duration: 2.0, ease: [0.25, 1, 0.5, 1] }}
      />
      <m.div
        className="ember-orb"
        animate={{ scale: [1, 1.15, 1] }}
        transition={{ repeat: Infinity, duration: 2.0, ease: [0.25, 1, 0.5, 1] }}
      />
    </m.div>
  );
}
