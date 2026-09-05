# Production readiness

The audit at `179e397` is the baseline. Production approval requires native evidence on
Windows, macOS and Linux, in addition to automated checks. A successful local build is
not production approval.

## Implementation ledger

The approved six-stage plan is **partially implemented**. Production release approval remains blocked. Windows evaluation candidate 1.1.0-rc.1 is published and installed locally; its signature, migration and two-monitor picker smoke results are recorded in [native qualification evidence](native-qualification.md). This ledger supersedes production claims in older audits
and demonstration recordings. Baseline: `179e397`, `feat/picker-follows-the-pointer`.

| Area | Required evidence | State |
| --- | --- | --- |
| Text integrity and destination | Core regressions implemented; Windows checks window/control HWND, UIA element/range and recaptured text | Continuous input generation and native application matrix open |
| Input ownership and cancellation | Run phase coordinator, retained native job ownership, shared input lease, joined watcher, fail-closed confirmation | Input-generation continuity and native abort/hook matrix open |
| Overlay and project picker | Shared monitor surface, measured DOM bounds, sequence snapshots, paging, reduced motion | Browser and geometry tests pass; native matrix open |
| Context and projects | None/Auto/Pinned behavior, resolved snapshot inspector, draft revisions, bounded imports, source fingerprint | Explicit reviewed global imports implemented; full scope hierarchy open |
| Persistence and connections | Endpoint-bound keys, encrypted opt-in results, generations, atomic config, bounded streams | Deterministic native race/failure tests open |
| Platform parity | Three-OS CI passed tests, strict lint and frontend build; real Linux credential backend | macOS/Linux input/clipboard/target adapters not implemented |
| Distribution | Signed updater candidate published and installed, pinned tooling, opt-in uninstall data removal | Publisher signing/notarization, wider native qualification and recovery matrix open |

## Findings and implementation

"Implemented" below identifies code changes. It does not close a finding whose native evidence
or remaining implementation is listed in the last column.

| Audit IDs | Implemented change | Remaining work |
| --- | --- | --- |
| A01 | Reject unknown/different HWND/PID; Windows UIA editable element and selection-range lease before capture, after capture and before paste; reapply uses same path | Native browser/editor selection qualification; AX/AT-SPI adapters; input generation spanning every transition |
| A02 | Hook install failure and unsupported platforms reject; actual original/result comparison with keyboard paging | Native focus and screen reader qualification |
| A03 | Remove stale terminal clipboard fallback and generic line clearing; preserve bounded HGLOBAL formats; ownership-aware restore before network wait | Close ownership gaps around capture; delayed rendering tests; safe terminal adapters. Generic terminal replacement is disabled |
| A04 | Mask original bytes before normalization; check token count/order/unknown markers; restore after cleanup; preserve joiners; protect fences, inline code, paths and explicit shell prompts | Broader linguistic/provider evaluation and ambiguous command fragments |
| A05 | Complete PEM block redaction; explicit bounded global imports, reviewed snapshots and source provenance; oversized profiles refused before network | Semantic extraction limits and adversarial model evaluation |
| A06 | HTTPS URL validation; no redirects; endpoint-bound vault entry; legacy credential binds to old configured endpoint before URL change; connection generations | Named connection schema and explicit local HTTP without authentication |
| A07 | All custom commands listed in application ACL manifest; settings/overlay/picker/animation capabilities separated; static security contract tests | Negative IPC invocations in real packaged webviews |
| A08 | Retention and diagnostic generations; one writer; separate session/persistent cache; authenticated encryption; logout invalidates before waiting and rejects stale commits | Native concurrent logout/login/refresh, disable/re-enable/write, vault failure and interrupted-write tests |
| A09 | release-please explicitly creates prereleases; automatic promotion removed; actions/toolchains pinned | Three-platform artifacts, signature/notarization gates and controlled promotion |
| A10/A11/A12 | Shared stationary monitor-sized click-through surface; physical cursor and logical content coordinates; 500 ms topology reconciliation; change-only events; measured bounds and edge hysteresis | Native mixed-DPI/hotplug/taskbar/remote session tests and measured idle/GPU/latency budgets. Display-change event invalidation remains preferable to polling |
| A13 | Explicit run phase coordinator and retained native job ownership; shared input lease; joined watcher; cancellation and owned key tails | Continuous input generation, lost key-up recovery and complete mouse button/drag matrix |
| A14 | Run ID plus monotonic sequence, state snapshots after subscribing; stale run and sequence rejection | Native webview reload and transition evidence |
| A15 | Draft epoch gates scan/distillation; obsolete colour responses retire with their editor; colour commits preserve newer fields; shared icon/accent registry | Native project editor visual review and wider async save/delete matrix |
| A16 | Known root files plus scoped Markdown imports; technical facts retained by distillation; bounded read, cycles, provenance and stale-source warning | Root-to-active-file scope hierarchy and source precedence editor |
| A17 | Explicit No project, Auto and Pinned behavior; Auto only chooses registered roots; empty brief cannot impersonate applied context; inspector exposes resolution | Persisted selection remains backward-compatible fields; inspector shows resolved context, not a provider delivery receipt |
| A18 | Original bytes in exact cache key; no fuzzy lookup in automatic path; SHA-256 request/chain/connection/credential identity | Paid-provider cache/billing acceptance matrix |
| A19 | Serialized atomic config writer; revision conflict rejection; schema migration and preserved recovery copy; hotkey/autostart compensation on failure | Real concurrent UI/OS failure and rollback scenarios |
| A20 | Header/connect/total/useful-progress bounds; bounded JSON and stream buffers; explicit completion required; uncertain requests not automatically retried; failed join cannot spawn duplicate | Fault matrix against real providers and supported OAuth integration contract |

## Next implementation order

| Priority | Work package | Completion condition |
| --- | --- | --- |
| P0 | Windows destination and input transaction | Element/selection snapshot and input epoch cover capture through application; duplicate text in different controls, changed selection and user copy cannot cause wrong replacement |
| P0 | Clipboard and terminal adapters | Format and ownership race tests pass in actual applications; terminal behavior qualified per terminal/editor without generic destructive shortcuts |
| P0 | Context authorization and extraction | Explicit global sources, deterministic project hierarchy, visible precedence and omission; operational instructions never become runtime authority |
| P0 | State and persistence fault harness | Controlled schedules prove logout cannot resurrect auth and old operations cannot persist after disable/re-enable; disk/vault failures leave recoverable state |
| P0 | Linux feasibility and macOS adapters | Qualify X11/Wayland GNOME/KDE, accessibility permissions, cursor observation and paste identity. Compilation or copying a result does not satisfy parity |
| P1 | Native floating surface qualification | Complete the monitor/input matrix below with latency, frame loss and stationary CPU/GPU measurements |
| P1 | Context editor and language evaluation | Async project component tests, accessibility review, Portuguese/English/mixed-text evaluation with literal preservation |
| P1 | Release operations | Signed/notarized artifacts and update manifest; clean install, upgrade interruption, reinstall and both uninstall choices pass |

## Architecture decisions

Keep the Rust core free of I/O. Existing Tauri windows remain; a shared geometry surface
replaces repeated native window motion. This reduces competing coordinate systems, but a
monitor-sized transparent compositor surface needs native GPU/remote-session measurements.
A complete visual rewrite was not required to establish the geometry contract.

Keep configuration interfaces backward compatible and reject revision conflicts rather than
silently merging unrelated stale saves. A full transactional settings service remains an
alternative if conflicts become frequent. Result storage uses a separate authorized aggregate
so later writes cannot serialize memory-only results from an earlier policy generation.

Automatic replacement remains fail-closed for unknown native targets and terminals without
adapters. A successful input injection is reported as "Paste sent. Check your text." because
it does not prove the destination replaced the intended content.

## Destination and lifecycle increment, 2026-09-05

Windows now acquires a UI Automation selection lease before clipboard capture. It rejects
password, disabled, nonfocused, read-only, mixed and unsupported selection attributes.
The clipboard text must equal the accessible selection. Select-all is accepted only when
the initial selection was empty and the resulting range covers the editable document.
Before application, the original element, both range endpoints, text digest and native
window/control must still match. The original text remains the exact cache input.

The accessibility service has one MTA thread, one pending request, a 1.5-second caller
budget and 250-millisecond provider connection/transaction timeouts. Expired requests
cannot authorize a later paste. Objects remain on their creating thread and expire after
ten minutes. No focus-setting API is called. The coordinator serializes capture, request,
review, application and cancellation; completion releases only its own run. Native jobs
retain run ownership if their async caller is aborted before those jobs return.

This choice adds capability checks to the existing clipboard adapter. A clipboard-only
comparison was rejected because identical text can appear in different fields or positions.
A full accessibility-based replacement adapter remains an alternative after app qualification.
The cost is reduced compatibility: editors without TextPattern or a definite editable attribute,
and providers with different clipboard/accessibility newline representations, may be refused.
There is no permissive fallback. Rollout remains local/pre-release. Reverting this increment
requires returning to a production-blocked build, never claiming equivalent destination safety.

Automated regression tests cover cancelled/obsolete run transitions, timeout and queue
saturation, unknown editability values and native COM client initialization with focus changes
disabled. They do not prove selection comparison in Chrome, Edge, VS Code, Notepad or Office.
A continuous input epoch and atomic clipboard ownership across all reads/writes remain open.
UIA ranges can track document edits; range equality alone is not evidence that no input occurred.
The final check and input injection are still separate operations.

API references: Microsoft's [threading requirements](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading),
[range endpoint comparison](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtextrange-compare),
and [provider timeout](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomation2-put_transactiontimeout).

## Verification boundary

Automated checks cover pure text/selection logic, bounded HTTP stream fixtures, authenticated
storage envelopes, configuration migration/conflicts, source imports and browser-rendered
floating components, plus Windows COM client initialization. They do not exercise actual target selection changes, the OS clipboard, vault, hooks, native monitor
transitions or installers. Checks executed in this Windows checkout:

| Command | Observed output |
| --- | --- |
| `cargo test --workspace --locked --quiet` | Windows shell: `43 passed; 0 failed`. Core: `328 passed; 0 failed` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Exit 0, `Finished dev profile` in 5.31s |
| `npx tsc --noEmit` | Exit 0, no diagnostics |
| `npm test` | `tests 6`, `pass 6`, `fail 0`, including browser layout, snapshots and ACL contracts |
| `npm run build` via Tauri build | `5094 modules transformed`, `built in 19.46s` |
| `node scripts/sync-versions.mjs` | `Versions in sync: 1.0.0` |
| `git diff --check` | No whitespace errors; Git emitted line-ending conversion warnings |
| `npm audit --json` | `total: 0` vulnerabilities |
| `cargo audit --json` | Exit 1, one rkyv advisory plus informational notices |
| `cargo tree --locked -i rkyv --target all` | `warning: nothing to print` |
| Standard `npm run tauri -- build --bundles nsis` | Optimized executable and NSIS produced; exit 1 because updater private signing key is absent |
| Local audit bundle with `--config target/local-audit-bundle.json` | Exit 0, optimized build in 4m 36s, `Finished 1 bundle`. The temporary override disables updater artifacts only for this local unsigned build |

The earlier local installer is `target/release/bundle/nsis/Ember_1.0.0_x64-setup.exe`.
It predates the execution coordinator and accessibility guard described below and must not be used to validate them. It was not installed or published. Its local audit configuration is in ignored build output;
checked-in release settings still require signed updater artifacts.

Browser screenshots were inspected for the long project/status labels, comparison and picker.
These use mocked Tauri IPC and are not native screenshots. The 16-case
[synthetic language corpus](language-evaluation.json) covers European Portuguese, English,
mixed technical text, numbers, commands, paths, URLs, protected code and Unicode. No live
model quality scores are claimed.

`npm audit --json` reported zero vulnerabilities. `cargo audit --json` still reports
`RUSTSEC-2026-0235` for `rkyv 0.7.46` through an optional `rust_decimal` lockfile dependency;
`cargo tree --locked -i rkyv --target all` returned `nothing to print`. GTK/glib maintenance
and unsoundness notices also require platform-specific review. No blanket audit suppression
was added. A lockfile advisory without an active chain is not proof of binary exploitability.

The local WSL distribution has no Cargo or required WebKit/pkg-config environment. macOS hardware is unavailable here. GitHub run [33941268636](https://github.com/duartelcunha/Ember/actions/runs/33941268636) passed Rust tests, strict Clippy, browser regressions, TypeScript and frontend build on Windows, macOS and Ubuntu 24.04. This establishes CI compilation and automated checks, not native functional parity.

## Release boundary

Results are intended to remain in memory by default. Existing user data must not be
silently deleted by migration. Unsupported target validation must reject automatic
replacement and retain the result for explicit recovery.

## Native qualification matrix

Exercise one, two and three monitors, negative coordinates, portrait orientation,
100 to 200 percent scaling, taskbars and docks on every edge, monitor removal, resume,
remote sessions and virtual desktops. Exercise held hotkeys, quick cancellation,
typing during requests, IME, mouse drag and focus changes. Check clipboard ownership
with plain text, rich text, images and a new user copy during the request.

Run the same input, context, network and persistence fault cases on each platform.
Record platform versions, application versions, commands, output and native evidence
with the release candidate. Missing native evidence keeps that platform unqualified.

## Supporting documents

See [data handling and rollback](data-policy.md) and the [threat model](threat-model.md).
Authoritative API references used include [Tauri application capabilities](https://v2.tauri.app/security/capabilities/),
[Tauri system prerequisites](https://v2.tauri.app/start/prerequisites/),
[keyring backend selection](https://docs.rs/keyring/3.6.3/keyring/), and the
[Tauri 2.11.4 NSIS template](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi).

## Candidate delivery

Version 1.1.0-rc.1 is a Windows evaluation candidate. The qualified-prerelease workflow runs
the three-platform CI checks, builds in a draft, downloads the uploaded installer and
verifies its signature with the existing updater public key. The manifest URL, version and
signature must match before publication as a prerelease. This does not promote the stable
update channel or establish native platform parity. Windows publisher signing is unavailable.

The release-please workflow now retains its releases as drafts. Manual candidate delivery
is separate from production promotion. The signature verification utility can be run locally
with `cargo run --locked -p ember --example verify_update -- <installer> <signature>`.

Cross-platform CI initially found Windows-only dead code in macOS/Linux builds. The pure key ownership decisions and their six regressions now live in ember-core; only native adapter entry points are platform-gated. Strict lint remains enabled.

The candidate delivery workflow completed successfully. Tag `v1.1.0-rc.1` points to the
qualified merge commit `5fc4b14d56eeb6f76fa4ba1b54e91bb2a0f367aa`. The release contains
the NSIS installer, detached signature, `latest.json` and `SHA256SUMS.txt`; stable latest
remains `v1.0.0`. Release-please was reconciled after publication and its erroneous
pre-publication downgrade proposal was closed. Remaining production work is tracked in
[issue #26](https://github.com/duartelcunha/Ember/issues/26).

Follow-up publication guards reject an already published release and a draft whose source
revision differs from the checkout, before downloading or uploading artifacts. The real
PowerShell script passes three command-mocked regressions, including the matching-source
path reaching artifact verification. The tests do not issue remote mutations.

## Reviewed global profile increment, 1.1.0-rc.2

Ambient global discovery is removed. Explicit local import prepares a conservative draft;
only reviewed text is saved and used. Sources carry fingerprints and are not reopened during
refinement. Unknown sections, code examples and common operational directions are excluded.
Oversized drafts remain available for editing, and legacy oversized profiles stop the request.
The inspector exposes approved source provenance. Delayed imports cannot overwrite newer
edits or a reset. A timed-out filesystem worker retains ownership until it actually ends.

Automatic extraction followed by immediate use was rejected because the user could not
review omitted or conflicting rules. A complete semantic parser remains an alternative, but
would require a much broader evaluation contract. The present heuristic requires review and
can omit useful technical prose. No claim of complete prompt-injection immunity is made.

Existing manual overrides are preserved. The legacy discovery flag is retained for a migration
notice; no config schema change is needed. Rolling back to rc.1 can re-enable legacy discovery
if the old flag was never changed. Review Personalization before using an older build.

Local validation: `cargo test --workspace --locked --quiet` reported `49 passed; 0 failed`
in the shell and `340 passed; 0 failed` in the core. Strict Clippy exited 0. `npm test` reported
`tests 8`, `pass 8`, `fail 0`, including project distillation and colour ownership regressions.
`npm run build` completed with 5,095 modules in 11.12 seconds before the additional project fix;
that fix also passed `npx tsc --noEmit`.
Publication guards reported `3 passed; 0 failed`; version consistency reported `1.1.0-rc.2`.
The four browser captures in `target/profile-browser-evidence` were inspected locally.
They use synthetic fixtures and mocked IPC. Publication and installation evidence will be
recorded after the new candidate passes the qualified release workflow.

Compatibility review found an existing approved profile larger than the old 2,000-byte cap.
The reviewed profile budget is now 8 KiB and is supplied to Settings by the backend. The UI
shows the size used on every request. Existing profiles within that budget remain complete;
larger profiles still require explicit editing. Inspector and provider use the same redaction
and escaping function. Neutralizing a delimiter cannot truncate text at the budget boundary.

Budget compatibility validation: `cargo test --workspace --locked --quiet` reported 49 shell
and 342 core tests passing; `npm test` reported 8 passing, 0 failing; `npm run build` completed
with 5,095 modules in 10.51 seconds. The earlier candidate workflow was cancelled before
artifact generation so the next publication uses this corrected source revision.
