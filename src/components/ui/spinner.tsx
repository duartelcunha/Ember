/**
 * Spinners com a identidade Ember, para as Settings. Dois registos:
 *
 * - `arc`: o clássico arco em âmbar sobre trilho escuro, para botões e esperas pequenas onde
 *   qualquer coisa mais seria barulho.
 * - `embers`: três brasas a subir com desfasamento, para estados inline com texto ao lado
 *   ("checking…", destilação). É o "ellipsis" universal com física de fogueira.
 *
 * SVG/CSS puro, sem dependências (o lucide não entra só para isto). As cores vêm de
 * `currentColor`/tokens, portanto herdam o tema e a cor do contexto onde caem.
 */

const RISE = `
@keyframes ember-rise {
  0%, 100% { transform: translateY(0); opacity: 0.45; }
  40% { transform: translateY(-5px); opacity: 1; }
}
@keyframes ember-arc-spin {
  to { transform: rotate(360deg); }
}
`;

export type SpinnerProps = {
  variant?: "arc" | "embers";
  /** Lado do quadrado, px. */
  size?: number;
  className?: string;
};

export function Spinner({ variant = "arc", size = 16, className }: SpinnerProps) {
  if (variant === "embers") {
    return (
      <span
        className={`ember-spinner ${className ?? ""}`}
        role="status"
        aria-label="Loading"
        style={{ display: "inline-flex", alignItems: "center", gap: size / 5, height: size }}
      >
        <style>{RISE}</style>
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            style={{
              width: size / 4,
              height: size / 4,
              borderRadius: "9999px",
              background: i === 1 ? "var(--color-ember-glow, #ffd9a8)" : "var(--color-accent, #fd8c3c)",
              animation: `ember-rise 1.1s cubic-bezier(.33,.66,.66,1) ${i * 0.15}s infinite`,
            }}
          />
        ))}
      </span>
    );
  }
  return (
    <span
      className={`ember-spinner ${className ?? ""}`}
      role="status"
      aria-label="Loading"
      style={{ display: "inline-flex", width: size, height: size }}
    >
      <style>{RISE}</style>
      <svg
        viewBox="0 0 24 24"
        fill="none"
        style={{ width: size, height: size, animation: "ember-arc-spin 0.9s linear infinite" }}
      >
        <circle cx="12" cy="12" r="10" stroke="currentColor" strokeOpacity="0.18" strokeWidth="2.5" />
        <circle
          cx="12"
          cy="12"
          r="10"
          stroke="var(--color-accent, #fd8c3c)"
          strokeWidth="2.5"
          strokeLinecap="round"
          strokeDasharray="17 46"
        />
      </svg>
    </span>
  );
}
