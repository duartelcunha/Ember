import { m } from "motion/react";

/**
 * A faísca: o indicador de "a trabalhar" do Ember, a orbitar o PRÓPRIO cursor.
 *
 * A estrela saiu do overlay por decisão de design (2026-08-30): durante o refine o que a pessoa
 * está a olhar é o ponto onde vai aterrar o texto, e a marca inteira ali era peso. Fica uma
 * brasa a orbitar o ponteiro, com um rasto curto, nas cores do projeto ativo. A janela é
 * posicionada pelo Rust de forma a que o CENTRO deste componente caia exatamente no cursor
 * (ver `orb_target` em lib.rs, que usa metade do `SPARK_SIZE` daqui); o seguimento é rígido,
 * sem suavização, porque a vida visual vem da órbita e o arrasto punha o centro a nadar.
 *
 * Camadas: um rotor (rotate infinito) com a cabeça da faísca e dois fantasmas atrás no mesmo
 * círculo, mais um brilho mínimo no centro. Tudo compositor-only (transform/opacity); zero
 * backdrop-filter, zero layout por frame.
 */

/** Lado do quadrado da faísca, em px lógicos. ESPELHADO em `SPARK_SIZE` (lib.rs), que o usa
 *  para centrar a órbita no meio visual da seta do cursor. Muda um, muda o outro.
 *
 *  O raio da órbita é `SPARK_SIZE / 2 - DOT_INSET` = 17px, escolhido para o anel envolver a
 *  seta inteira (~12x19 lógicos) e não só a pontinha dela. */
const SPARK_SIZE = 40;
/** Distância do centro do ponto ao topo da caixa. Define o raio junto com o SPARK_SIZE. */
const DOT_INSET = 3;

/** Um ponto no rotor: `angle` é o atraso angular em relação à cabeça (0 = cabeça). */
function Dot({ angle, size, opacity }: { angle: number; size: number; opacity: number }) {
  return (
    <div className="absolute inset-0" style={{ transform: `rotate(${angle}deg)` }}>
      <div
        className="absolute left-1/2 top-0"
        style={{
          width: size,
          height: size,
          marginLeft: -size / 2,
          marginTop: -size / 2 + DOT_INSET,
          borderRadius: "9999px",
          background: "var(--color-ember-glow)",
          opacity,
          boxShadow:
            angle === 0
              ? "0 0 6px 2px color-mix(in srgb, var(--color-accent) 65%, transparent)"
              : undefined,
        }}
      />
    </div>
  );
}

/** Em que ponto do trabalho estamos. Sai do que o núcleo já diz, e não de um estado inventado:
 *  sem mensagem é o caminho normal; com mensagem, o núcleo está a fazer retry ou a passar para
 *  o provider de reserva (`flow.rs` emite "Trying/Retrying <provider>..."), ou seja, isto vai
 *  demorar mais do que o normal. */
export type OrbVariant = "work" | "retry";

/** Escala do rotor por estado. Cresce quando o refine tropeça: a espera passa a ser visível no
 *  próprio ponteiro, antes de a pessoa ler a legenda ao lado. */
const VARIANT = {
  work: { scale: 1, spin: 1.3 },
  retry: { scale: 1.32, spin: 1.9 },
} as const;

export function Orb({ variant = "work" }: { variant?: OrbVariant }) {
  const v = VARIANT[variant];
  return (
    <m.div
      className="relative"
      style={{ width: SPARK_SIZE, height: SPARK_SIZE, willChange: "opacity, transform" }}
      initial={{ opacity: 0, scale: 0.4 }}
      animate={{ opacity: 1, scale: 1 }}
      // A saída é o primeiro tempo do morph para a pilula: a faísca COLAPSA para o centro
      // (scale down rápido) e a pilula acende logo a seguir a partir do mesmo sítio (ver a
      // entrada em Pill.tsx). Sem layoutId, de propósito: se qualquer metade falhar, cada
      // uma degrada para um fade curto, nunca para uma posição esquisita.
      exit={{ opacity: 0, scale: 0.15, transition: { duration: 0.13, ease: "easeIn" } }}
      transition={{ duration: 0.18, ease: "easeOut" }}
    >
      {/* Brilho mínimo no centro: dá "lareira" ao ponteiro sem o tapar. Estático. */}
      <div
        className="absolute"
        style={{
          inset: 13,
          borderRadius: "9999px",
          background:
            "radial-gradient(circle, color-mix(in srgb, var(--color-accent) 32%, transparent) 0%, transparent 70%)",
        }}
      />
      {/* O rotor: cabeça + dois fantasmas atrás no círculo formam o rasto.
          A mudança de tamanho entre estados é `scale` NESTA camada e nunca no tamanho da caixa:
          a caixa é o que o Rust usa para centrar a órbita no ponteiro (`SPARK_SIZE` em lib.rs),
          portanto mexer-lhe descentrava a órbita. Escalar a partir do centro mantém o ponto fixo
          e o crescimento lê-se como o mesmo objeto a inchar, não como outro a aparecer. */}
      <m.div
        className="absolute inset-0"
        style={{ willChange: "transform", transformOrigin: "center" }}
        animate={{ rotate: 360, scale: v.scale }}
        transition={{
          rotate: { repeat: Infinity, duration: v.spin, ease: "linear" },
          scale: { type: "spring", stiffness: 260, damping: 24 },
        }}
      >
        <Dot angle={0} size={4.5} opacity={1} />
        <Dot angle={-24} size={3.5} opacity={0.45} />
        <Dot angle={-46} size={2.5} opacity={0.2} />
      </m.div>
    </m.div>
  );
}
