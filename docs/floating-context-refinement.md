# Floating surfaces and authorized context

## Decision and behavior

Keep the existing pixel artwork. Anchor its 15 by 15 logical pixel ring independently
of the SVG padding and project labels, 18 logical pixels beside the cursor and 6 below.
Near an edge, change sides with hysteresis and clamp within the work area. Native events
carry a geometry generation, physical origin, size and scale. The frontend hides content
until the window dimensions and DPI agree, then applies the latest cursor sample per frame.

Messages and comparison cards have solid backgrounds. Comparison width follows content
between 240 and 420 logical pixels, bounded by the viewport, with original and result
stacked. The existing keyboard gate retains ownership of Enter, Escape and pagination.
Pages preserve graphemes and all source text. A conservative 24-column, two-line page
budget keeps the stacked card within 40% of the tested work areas. Browser coverage includes
a 320 by 480 logical viewport. Smaller work areas still require qualification.

Labels are independent because measuring the combined orb and label caused long names to
move the visible pixels. Replacing the floating window architecture was rejected: the
existing non-focusing surfaces remain, with coordinated native and rendered geometry.

## Context selection and authorization

Selection order is pinned project, canonical path inside a registered project, then an
explicit full application-path association. Multiple projects associated with an application
produce no automatic match. None disables project context while retaining personal preferences.
Window-title path hints select within registered roots; they never authorize additional files.
There is no inference from selected text. A dedicated active-editor integration is still
needed to establish scope where titles do not expose a usable path.

Project settings expose application associations and individual authorized sources. Saving
validates canonical paths and reads only text files within that project's root. Scanned
sources require the explicit authorization action and a save. New imports are not followed
by the refresh worker, even when referenced by an authorized file. Existing scanner cycle
detection remains in place; refresh has no recursive import traversal.

One background reader checks metadata every two seconds and reconciles fingerprints at
least every thirty seconds. It makes no provider calls. Reads are limited to 32 sources,
512 KiB combined and 32 KiB extracted per source. Read, boundary or extraction failure keeps
the last approved context and reports the condition. Refine uses an in-memory snapshot.
Refreshes persist in memory; after restart the saved approved snapshot is available while
the authorized files are checked again.

Sources apply from root through ancestors of the known active path. Without a specific path,
only root sources apply. A source's directory defines its scope; authorizing an imported file
does not inherit the importer's scope. Extracted writing preferences and technical facts are
separate. More specific fields replace matching general fields; explicit Ember preferences
take precedence over derived context. Operational directions, code examples, external memory
references and unknown sections are excluded conservatively. This parser can omit useful
prose and is not a complete semantic security classifier.

The existing project prompt budget remains 2,000 characters. If composition exceeds it,
derived sources are omitted, saved preferences remain, and the inspector reports the limit.
The inspector lists the sources actually used. The cache fingerprint includes the resulting
context as well as the existing model, connection and processing identity.

Each context snapshot belongs to one run. It distinguishes Prepared, Sending, Sent,
Reused result and Delivery unconfirmed. Sent means response headers were received, including
an HTTP error; it does not mean refinement succeeded. A transport failure can leave delivery
uncertain. Late completion of an older run cannot update the newer inspector snapshot.

## Migration and rollback

Configuration schema 2 adds versioned associations and source authorizations. Migration
creates a recovery copy before changing the schema. Existing project data, profile text and
schema 1 retention choices survive. Schema 0 retains the earlier explicit-consent migration.
Existing files and associations are not automatically authorized.

Operational legacy profiles remain stored unchanged. Personalization offers original text,
a filtered draft and exclusions. Applying the draft requires an explicit save. The original
profile is archived on the first replacement or reset. Until review, refinement uses only
the filtered preference data. Empty extraction never falls back to operational raw text.
Global imported profiles remain reviewed snapshots and require another import to update.

Schema 2 cannot be opened by rc.2. Rollback requires closing Ember, keeping a copy of the
schema 2 configuration, restoring its pre-migration backup and then installing the previous
compatible version. Later edits must be recovered manually; never overwrite that backup.

## Evidence and delivery gate

Local automated checks on 2026-09-05:

* `cargo test --workspace --locked --quiet`: 53 shell and 347 core tests passed.
* `npm test`: 13 tests passed, 0 failed, including real React components with mocked IPC.
* `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0.
* `npm run build`: 5,095 modules, built in 10.23 seconds.
* `powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/publication-guards.test.ps1`: 3 passed, 0 failed with mocked GitHub operations.
* `node scripts/sync-versions.mjs`: versions in sync at 1.1.0-rc.2.

The offline native example uses the production overlay frontend and positioning code,
without configuration, credentials, input hooks or providers. The 16-scenario run
`20260905-145544-floating.json` preserved focus and matched work areas on both monitors.
Both monitors were 96 DPI. Artifact SHA-256:
`F623628ED5D01BEC56DB2E4ED0E6BD551C77685D60C14A164A101EDBF1432EDF`.
Representative orb, project-edge and comparison captures were inspected locally. Screenshots
include desktop content and are not published. The first run was interrupted by a foreground
change; a subsequent isolated run passed. That first run is not counted as a success.

These are geometry smoke results, not installed-candidate or paste qualification. Mixed DPI,
physical drag/input, hotplug, focus under real requests and the complete platform matrix
remain open. Native Windows qualification with two different scales is required before a
new prerelease or installation. No candidate version is incremented by this implementation.
The `-RequireMixedDpi` run refused qualification before moving the cursor because both
connected displays were 96 DPI.
macOS and Linux native parity remain production blockers.

## Pointer anchoring follow-up

The running rc.2 still uses its published geometry. The follow-up also corrects a difference
in the pending frontend: review and result cards now share the orb's logical pixel
cursor gap. Visual feedback found the initially requested 10 by 2 gap too close, so the
shared gap is now 18 beside and 6 below. At a stationary cursor, reducing the review to a result pill cannot switch
sides merely because the smaller content would fit. Moving the pointer or changing monitor
geometry permits normal side selection again. Fractional sizes are retained and the edge
facing the cursor is snapped to physical pixels, avoiding width-dependent rounding drift.

The added rendered-component regression first failed at `314 !== 310`. It now checks
review, acceptance-result and orb transitions at 100%, 125%, 150%, 175% and 200%, including
negative monitor origins, with at most half a physical pixel of rounding. `npm test`
reports 14 passing tests. Rust still reports 53 shell and 347 core tests passing;
`npm run build` completed in 15.82 seconds. The acceptance test exercises the state emitted
after Enter; it does not claim to exercise the native keyboard hook or actual paste.

Two additional native smoke attempts at the initial spacing stopped when foreground
ownership changed to another application. Work-area bounds matched in every recorded sample;
the fixture did not own foreground focus. These interrupted attempts are not qualification
passes for this follow-up. Mixed-DPI and real Enter/paste qualification remain pending.

## Exclusive loading and review states

Hint pills no longer render a decorative cursor icon. Their text is sufficient, and the icon
could be mistaken for a second pointer. Phase changes now replace the old subtree immediately.
The former presence boundary retained the orb's nested exit animation even though its parent
had a zero-duration exit, so loading and review briefly coexisted.

Two rendered regressions reproduced both defects before correction: the hint contained one
unexpected SVG, and a DOM observer recorded an orb and review together. After correction,
`npm test` reports 16 passing tests, including no overlap at the transition commit and no
retained orb afterward. `cargo test --workspace --locked --quiet` reports 53 shell and 347
core tests passing. `npm run build` completed in 16.15 seconds. The installed application
remains rc.2; these changes still require native qualification and delivery.
