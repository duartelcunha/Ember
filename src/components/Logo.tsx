/** "Tile + spark": flat orange app-tile with a four-point spark cut from its center. */
export function Logo({ size = 32 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" fill="none" aria-label="Ember">
      <path
        fill="#ff7a18"
        fillRule="evenodd"
        clipRule="evenodd"
        d="M20 6 h24 a14 14 0 0 1 14 14 v24 a14 14 0 0 1 -14 14 h-24 a14 14 0 0 1 -14 -14 v-24 a14 14 0 0 1 14 -14 Z
           M32 17 C 33 28 36 31 47 32 C 36 33 33 36 32 47 C 31 36 28 33 17 32 C 28 31 31 28 32 17 Z"
      />
    </svg>
  );
}
