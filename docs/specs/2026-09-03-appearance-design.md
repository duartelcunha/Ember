# Appearance options: custom project colours and a configurable overlay

Status: **design, not started**. Written 2026-09-03.

## Context

Two appearance requests came out of using the app, and both were deliberately left out of the
cheap pass that shipped in `feat(projects): double the accent palette and the icon set`:

1. **Custom project colours.** The palette went from 8 to 16 hand-written accents, which covers
   most cases. What it does not cover is matching a specific brand colour, and the current data
   model cannot hold one at all.
2. **A configurable overlay.** The overlay is the part of Ember seen most often and the one with
   the fewest options: it is always 520x140, always born at the cursor, and always follows it.

Both were sized as "medium" rather than "an afternoon" because the first touches the persisted
data model and the second touches geometry that is mirrored between Rust and TypeScript. This
document is what has to exist before either is written.

## What is already decided

- Custom colours ship alongside the 16 fixed ones, not instead of them. The fixed palette
  stays the fast path.
- The overlay opens three axes: where it appears, how big it is, and how long it dwells.
- Nothing here may open a paid code path or change refine behaviour. This is appearance only.

## 1. Custom project colours

### The problem with the current model

`Project.accent` is a `u8`, an **index** into `projects::ACCENTS`
([projects.rs](../../crates/ember-core/src/projects.rs)). An arbitrary colour has nowhere to live.
Each accent is also three hand-written stops (`raw`, `mid`, `glow`), because the orb is a
three-stop gradient, so a single colour is not enough input.

### Approach: an optional second field, no migration

Add `accent_custom: Option<String>` (a `#rrggbb` string) next to the existing `accent`, with
`#[serde(default)]`. When it is present it wins; when it is absent everything behaves exactly as
today.

Changing `accent` into a serialised enum would need migration code for every config already on
disk. This needs none. A config written by a newer build and opened by an older one degrades to
the indexed accent instead of failing to parse, and `sanitize_projects` is the only thing standing
between a hand-edited config and an app that will not start.

### Deriving the three stops

A new pure function in `ember-core`:

```
derive_accent(hex: &str) -> Option<Accent>
```

`None` for anything unparseable, so a bad value falls back to the indexed accent rather than
painting the orb black.

Convert to **OKLCH**, then place the three stops by lightness while keeping hue:

- `mid` is the colour the user picked, with chroma clamped to what the display can show;
- `raw` keeps the hue at much lower lightness (the deep stop the gradient starts from);
- `glow` keeps the hue at high lightness and reduced chroma (the pale halo).

OKLCH and not HSL: HSL lightness is not perceptual, so the same numeric shift makes a yellow
look washed out and a blue look muddy, and the orb gradient is exactly where that shows.

Tests, all pure and offline:

- a known input produces stops ordered `raw` < `mid` < `glow` in lightness;
- hue is preserved across the three stops within a tolerance;
- the pale stop keeps a minimum contrast against the dark stop, so the gradient never collapses
  into a flat blob at orb size;
- garbage in (`""`, `"red"`, `"#12"`, `"#gggggg"`) returns `None`;
- extremes (pure black, pure white, fully saturated) still return three distinct, ordered stops.

### UI

The colour row keeps the 16 swatches and gains one **custom** slot at the end. Picking it reveals
a hex field with a live preview of the orb gradient, so the derivation is visible **before** it is
saved. The existing "duplicate" test does not apply to custom colours: two projects may legitimately
be given the same brand colour.

## 2. Overlay: placement, size, dwell

The geometry is already parameterised. `overlay_geom::Layout` is a struct with `DEFAULT_LAYOUT`
as one instance, injected as `&LAYOUT` and covered by geometry tests, so none of the three axes
needs new maths. They need new inputs.

### Placement

Today: born at the cursor and follows it while refining (`follow_cursor`). Add a config choice:

- `cursor` (default, today's behaviour);
- a fixed screen corner, on the monitor the cursor is on.

The follow loop already retires by generation, so the fixed mode simply does not start it. The
clamping logic is unchanged: a corner still has to respect the visible box for the current phase.

The overlay never takes focus, in either mode. That is inviolable rule 1 in CLAUDE.md, and it is
why every refine lands in the app the user was typing in.

### Size

A second `Layout` instance (`COMPACT_LAYOUT`), not a scale factor: the layout carries a
`pill_box` sized to the longest message, and shrinking that by multiplication would clip
error text, which is precisely when there is something worth reading. Compact therefore means
"orb and state, no prose", and the pill messages keep the standard layout.

### Dwell

`overlay::feedback_for` returns a per-outcome `hide_after_ms`, calibrated to message length (a
long clipboard warning stays longer than a cancellation). Expose a multiplier rather than absolute
values, so that relative calibration survives. Floor it, so a dwell too short to read cannot be
set at all.

## Config changes

Four new fields, every one defaulting to current behaviour:

| Field | Default | Effect |
|---|---|---|
| `Project.accent_custom` | `None` | Falls back to the indexed accent |
| `overlay_placement` | `cursor` | Today's behaviour |
| `overlay_size` | `normal` | `DEFAULT_LAYOUT` |
| `overlay_dwell` | `1.0` | Today's timings |

`Config::sanitize` rejects out-of-range values by replacing them with the default, the same way it
already handles an unknown icon and an orphan active project: a hand-edited config must never
break the app.

## What this does not include

- No colour picker widget. A hex field plus a live preview is the whole surface; a wheel is a
  bigger UI project for the same result.
- No per-project overlay settings. These are app-wide.
- No theme work. Following the Windows theme and a high-contrast theme are separate and cheaper,
  and should not be bundled with this.

## Risks

| Risk | Mitigation | Phase |
|---|---|---|
| A derived colour looks wrong on the orb even with valid maths | Live preview before saving, plus a contrast floor in the derivation, fixed by tests | 1 |
| Fixed placement puts the overlay under the caret or off-screen | Reuse the existing clamp per phase and per monitor; no new positioning maths | 2 |
| Compact size clips an error message | Compact carries no prose; pill messages keep the standard layout | 2 |
| A newer config opened by an older build | Additive optional field, so old builds ignore it instead of failing to parse | 1 |
| Dwell multiplier set so low nothing is readable | Hard floor in the pure function, with a test | 2 |
| Scope drifts into themes and a colour wheel | Both listed above as out of scope | all |

## Verification

- `cargo test --workspace` and `npx tsc --noEmit`, with the derivation tests listed above.
- A config from before the change loads unchanged, with the orb painting exactly as it did.
- A hand-broken config (invalid hex, unknown placement, dwell of `0`) loads and falls back
  visibly, with the fallback logged.
- Both placement modes and both sizes seen on a two-monitor setup, including near screen edges,
  which is where overlay geometry has broken before.
