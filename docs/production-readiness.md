# Production readiness

## Scope decision, 2026-09-09

Release 1.2.0 is a stable release for Windows only. The audit record below this section was
written for a three-platform product; it stays as the record of what was examined, but the
release decision is now taken against the Windows scope. macOS is the next track, planned on
its own; no Mac hardware is available at the time of writing, so its native qualification
stays open until one exists. Linux follows macOS.

What blocks a Windows stable release is anything that can still corrupt the user's text. Two
such paths were found in the rc.4 code and closed in 1.2.0:

- **Clipboard takeover before the paste.** Between arming the refined text and sending Ctrl+V,
  another writer (a clipboard bridge, a sync tool, any application) could replace the clipboard
  and the paste would insert its content into the document. `ember_core::selection::still_armed`
  re-reads content and sequence number right before injection, the accessibility check runs once
  more after it so that it stays the last operation before the keys go out, and a takeover ends
  in its own outcome (`ClipboardChanged`) with nothing sent. The guard decision is logged on
  every run, so a false-refusal rate can be read from `Ember.log`.
- **Terminals paid for a result nobody could reach.** Generic terminal replacement stays
  disabled. A terminal refine now leaves the flattened result on the clipboard and says so
  (`TerminalHandoff`), after the same run-lease, window and clipboard-content checks as a paste.
  No keys are ever sent to a terminal.

Deferred, with the residual risk in 1.2.0 stated:

| Item | Residual risk in 1.2.0 |
| --- | --- |
| macOS and Linux adapters, parity, notarization | Not shipped. The macOS track starts after 1.2.0 and covers CI builds only until a Mac exists |
| Windows Authenticode signing | SmartScreen warns first-time installers. No publisher certificate exists; stated in README and release notes |
| Root-to-active-file scope hierarchy and precedence editor | Feature gap. Reviewed sources stay a flat, explicit list |
| Terminal and editor adapters | Feature gap. Handoff instead of replacement; no destructive shortcuts |
| Persistence fault harness (logout, retention toggle, vault failure, interrupted writes) | Untested in code. Two manual checks belong to every release: retention off deletes the retained file, logout removes the credential |
| Mixed-DPI, multi-monitor, hotplug and remote-session matrix | Not proven for 1.2.0. Evidence comes from one machine with one monitor at 100% |
| Continuous input epoch | Input between the last accessibility check and `SendInput` is not detectable without a hook. The window is milliseconds and that check is the last operation before injection |
| Shell wiring of the two fixes (`src-tauri/src/flow.rs`) | No unit tests, as for the rest of that file. The pure pieces are tested; the wiring is proven by the logged hands-on runs below |

Evidence contract for 1.2.0: real use of the installed build on the actual applications, by
hand, read back from `Ember.log` (capture with lease, request, guard decision with both
revisions, paste sent or handoff, outcome, zero error lines) and recorded in
[native qualification](native-qualification.md). Injected input does not exercise Ember
(empty captures, clicks off target), so no fixture stands in for a person here.

Automated checks on the release branch, Windows checkout: `cargo test --workspace --locked`
55 shell and 379 core tests passed; `cargo clippy --workspace --all-targets --locked -- -D warnings`
exit 0; `npx tsc --noEmit` exit 0; `npm test` 29 passed; `npm run build` 5,151 modules;
publication guards 9 passed with GitHub and git mocked; `git diff --check` clean.

Release mechanics: release-please creates a draft and its source tag. The build job checks that
both the draft's target and tag match the merge commit, keeps the draft off the stable channel,
then uploads the artifacts. Publication is a separate manual step with
`scripts/verify-prerelease.ps1 -Publish`; stable promotion uses `-Promote`. Both paths verify
the installer, manifest and updater signature before changing the channel. The guards are
pinned in `scripts/publication-guards.test.ps1` and `scripts/assert-draft-release.test.ps1`.

After a full release, release-please proposes the next candidate as `X.Y.Z-rc` with no number
(observed after 1.2.0: PR #50 read `1.2.1-rc`), and only the candidates after it count up. The
candidate workflow and `verify-prerelease.ps1` refuse that form on purpose, since every guard
and tag pattern expects `-rc.N`. The first candidate of a train is therefore given its number
with a `Release-As: X.Y.Z-rc.1` footer on a commit to `main`, the way #43 gave rc.4 its number;
release-please rewrites the open release PR to that version on its next run, and the following
candidates increment on their own.

## 1.2.0 delivery, 2026-09-09

PR #45 was squash-merged into `main` as `ab44b25` with `Release-As: 1.2.0` in its footer (the
repository's squash setting drops the PR description, so the message was written explicitly).
release-please opened PR #46 at 1.2.0 in the manifest, `Cargo.toml`, `package.json` and the
CHANGELOG; its merge (`1db9d81`) produced tag `v1.2.0`. The release was created as a full release
and the build job's first step marked it a prerelease before any artifact existed, so the updater
channel never left v1.0.0. Workflow run 34298979550 attached the installer, its signature and
`latest.json`.

The artifacts were verified from the release commit: updater signature valid, SHA-256
`8845a4b11e3c5e2745852deab6587873808e3a117d93424ca9e04b4b464b3ac6`, manifest version and URL as
expected, Authenticode NotSigned. The CI-built installer was installed with `/S /UPDATE` over a
local build of the same code; configuration, projects, credentials and the retention setting were
preserved. Details are in [native qualification](native-qualification.md).

The hands-on smoke before the merge was skipped by decision (the log showed no runs on the local
build) in favour of one hands-on pass on the CI artifact. That pass happened on 2026-09-09: three
runs on the installed build (Notepad and a Brave text field pasted with the clipboard guard armed,
Windows Terminal handed the result to the clipboard), zero error lines, recorded in
[native qualification](native-qualification.md). Promotion then ran from the release commit
(`scripts/verify-prerelease.ps1 -Tag v1.2.0 -Promote`), re-verified the artifacts, uploaded the
checksums and made v1.2.0 the full release; the updater endpoint reports 1.2.0. What that pass did
not exercise stays listed as unproven in the same record.

## 1.3.0 delivery, 2026-09-18

PR #53 was rebase-merged into `main`, so its seven commits reached the changelog by name. One of
them is a feature (the Refining tab), which is why release-please proposed 1.3.0-rc.1 rather than
1.2.1-rc.2. Its merge produced tag `v1.3.0-rc.1`; the build job attached the installer, its
signature and `latest.json`. CI was green on all three platforms, after one macOS re-run of the
same commit.

What this train fixes: a tray menu that could be left visible and empty, with every later click
refocusing it and no way to quit; a Quit that never left, because it built the one window not
created at startup on the thread its own command runs on and the build never returned; a shortcut
rule that accepted Shift plus a letter and so took that letter from every application; a refine
that failed without writing down which gate refused it; and the Refining tab, rebuilt around what
the four modes actually do.

The hands-on pass ran on the CI artifact rather than a local build, and is recorded in
[native qualification](native-qualification.md): quit through the tray in under a second, and a
refine of 503 characters answered by Gemini and pasted through the preview gate, with zero
warning or error lines in the log. The first press of that refine was refused by the
accessibility guard and the second went through; both are in the record.

## Codex and ChatGPT editor recovery, 2026-09-23

The Windows UIA guard now accepts a focused Edit control when the selection's read-only
attribute is unsupported but both its document range and ValuePattern prove editability.
An inconsistent `HasKeyboardFocus` property is not a veto for that independently verified
Edit control. The element, selection endpoints, text and native target are still rechecked
before automatic paste. For a full-field selection, Chromium can return different UIA
endpoints for the selection and document even when their text is identical. In that case,
the guard also requires the editable document's full text to match at seal and apply time.
A non-password writable Edit without a verifiable TextPattern can
refine only an explicit selection and leave the result on the clipboard for manual paste.
It never runs select-all or sends paste keys. A clipboard revision changed during the model
request prevents the handoff and preserves the newer clipboard contents.

Local Windows candidate evidence: `cargo test --workspace --locked --quiet` passed 58 shell
and 380 core tests; `npx tsc --noEmit` and `npm test` passed (31 frontend tests).
The optimized executable and NSIS installer were produced. The bundle command returned 1
at updater signing because no private signing key is available locally.

The candidate ran from the isolated worktree without replacing the installed executable.
Read-only UIA inspection of the installed Codex composer found an enabled non-password Edit
with writable ValuePattern and TextPattern; its selection attribute was unsupported while
its document range was editable. A direct native UIA test with the composer focused accepted
it as an automatic candidate. A selected-text refinement in the running candidate captured
56 characters from `ChatGPT.exe` without a field-detection refusal. The Gemini request then
ended `Uncertain` after 30 seconds, so no preview or paste followed and the request was not
automatically repeated. A separate native test on the actual Codex composer selected its
entire 42-character test field, sealed the field and rechecked the same selection. It did not
call a model or paste. The test text was removed and the composer returned to its empty state.
In a native WinForms Edit fixture without TextPattern, the installed build refused
`no_text_pattern`; the candidate returned `ManualHandoff` for an explicit 63-character
selection without modifying either field. The no-selection case returned
`ManualSelectionRequired` without select-all. Password and read-only controls were refused.
Changing focus to an equal-text field during a fresh request returned `ForegroundChanged`
and left both fields and the image clipboard intact. A newer clipboard copy during a fresh
request returned `ClipboardChanged`; the newer copy remained intact. The original image
was restored after the controlled clipboard trials, and the installed 1.3.0 process was
restarted.

At that point this was partial native qualification, not Codex or ChatGPT acceptance. Successful model
completion, preview confirmation, automatic replacement in the composer, selection movement
during a request, cancellation and other editors still need direct candidate runs before
release claims. The installed executable and original checkout were not replaced.

On 2026-09-24, the selected-text native run in the Codex desktop composer captured an
80-character test selection from `ChatGPT.exe`. Gemini returned HTTP 503 for the chosen
and alternate free models; the configured ChatGPT subscription fallback returned an
81-character refinement. The preview gate then recorded `REJECT (Esc consumed by hook)`.
The test did not confirm or paste the result, and the synthetic composer text was removed.
The test probe also searched for the wrong preview phrase, so its failure to observe the
preview cannot establish that the overlay was absent. Subsequent probes stopped before
typing or calling a provider because the desktop was active or the visible page was not an
empty composer. The candidate process was stopped and the clipboard was unchanged after
the completed run. With result retention disabled, that result was not available after
the candidate stopped. That attempt provided no paste evidence; later trials below did.

The final local checks passed: `cargo test --workspace --locked` (58 shell tests passed,
2 interactive tests ignored, and 380 core tests passed), `npx tsc --noEmit`, `npm test`
(31 passed), and `cargo clippy --workspace --locked -- -D warnings`. A local unsigned
NSIS candidate built with updater artifact creation disabled, without a signing key.
The executable SHA-256 is `BE838DF736569C2E54F2202C2B844F7D1C6212702E59E5537A64775027A1EFD2`;
the installer SHA-256 is `C6A63A0944546C3902F6940A4B34D73771317E4F49C93C14CB10ED988EA618D2`.
Installation was held until the later native preview and paste proof below.

The subsequent local Codex runs completed both application paths. With the whole field
unselected, the candidate captured 66 characters, displayed the whole-field preview,
consumed Enter, and logged `applied=Ok(Ok(Pasted))` and `Success`; the 70-character result
appeared in the same composer. With an explicit selection, it captured 80 characters,
displayed the selection preview, consumed Enter, and logged the same paste and success
outcomes; the 83-character result appeared in that composer. In both runs, the clipboard
revision stayed stable through application, the original clipboard formats and text were
restored, and the synthetic composer content was removed after observation. The probe also
observed a `ForegroundChanged` refusal when its first Enter mechanism moved focus. No
paste followed that refusal. A separate UIA probe process crashed before sending a hotkey;
its synthetic field and temporary config were recovered without a provider call. These
are harness failures, not successful application runs.

After those two successful candidate runs, the previous installed files and config were
copied to `%LOCALAPPDATA%\Ember-recovery-20260924-1730` and hash-verified. The local NSIS
installer ran with `/S /UPDATE` and returned 0. The installed executable SHA-256 is
`8CEC1CF138F982FB93D2F7434C661D1EA985AB9223F8A416F52D2DF509B6D86E`.
It has the same length as the candidate and differs in only the three bytes of the Tauri
bundle-type marker, which the installer changes from `UNK` to `NSS`. The installed process
started from `%LOCALAPPDATA%\Ember\ember.exe`, logged version 1.3.0, remained running for
the five-second startup check and was stopped by the smoke harness. The first further installed
Codex refinement was not run because the desktop did not provide a ten-second idle window.

A later installed Codex run with an explicit selection did pass. The probe began only after an
empty composer and an idle desktop were verified. It observed the preview, confirmed with Enter,
read the changed text from the same composer, and recorded `Pasted` and `Success` in `Ember.log`.
The probe cleared its test text, Ember restored the original clipboard formats and text, and the
temporary provider setting was restored. Its structured result reported
`PreviewObserved=true`, `ResultChanged=true`, `ComposerCleared=true`,
`ClipboardRestoredByEmber=true` and `ForegroundAfterEnterSameAsCodex=true`.

The installed whole-field trial captured 66 characters and received a model result, but the
probe detected a focus change before confirmation and sent no Enter. The next attempt stopped
before typing because the visible composer contained a draft. Both attempts stopped the Ember
test process and restored the temporary provider setting. They do not establish whole-field
paste on the installed build. The source checkout on `main` remained clean and untouched.
This remains local Windows qualification, not proof of the exact CI release artifact or
production publication.

## Audit record (three-platform bar)

The audit at `179e397` is the baseline. Production approval on that bar requires native evidence
on Windows, macOS and Linux, in addition to automated checks. A successful local build is not
production approval. The scope decision above supersedes this bar for 1.2.0.

## Implementation ledger

The approved six-stage plan is **partially implemented**. Production release approval on the three-platform bar remains blocked; see the scope decision above for 1.2.0. Windows evaluation candidate 1.1.0-rc.2 is published and installed locally; its signature, migration and version-specific native limitations are recorded in [native qualification evidence](native-qualification.md). The two-monitor picker pass belongs to rc.1. This ledger supersedes production claims in older audits
and demonstration recordings. Baseline: `179e397`, `feat/picker-follows-the-pointer`.

The subsequent [floating and context implementation](floating-context-refinement.md) adds
visible-pixel anchoring, compact opaque review, authorized source refresh and recoverable
profile migration. It is not a published candidate. A native geometry harness completed 16
scenarios on two 100% monitors; mixed-DPI qualification remains required before delivery.

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
| A01 | Reject unknown/different HWND/PID; Windows UIA editable element and selection-range lease before capture, after capture and before paste | Native browser/editor selection qualification; AX/AT-SPI adapters; input generation spanning every transition |
| A02 | Hook install failure and unsupported platforms reject; actual original/result comparison with keyboard paging | Native focus and screen reader qualification |
| A03 | Remove stale terminal clipboard fallback and generic line clearing; preserve bounded HGLOBAL formats; ownership-aware restore before network wait | Close ownership gaps around capture; delayed rendering tests; safe terminal adapters. Generic terminal replacement is disabled |
| A04 | Mask original bytes before normalization; check token count/order/unknown markers; restore after cleanup; preserve joiners; protect fences, inline code, paths and explicit shell prompts | Broader linguistic/provider evaluation and ambiguous command fragments |
| A05 | Complete PEM block redaction; explicit bounded global imports, reviewed snapshots and source provenance; oversized profiles refused before network | Semantic extraction limits and adversarial model evaluation |
| A06 | HTTPS URL validation; no redirects; endpoint-bound vault entry; legacy credential binds to old configured endpoint before URL change; connection generations | Named connection schema and explicit local HTTP without authentication |
| A07 | All custom commands listed in application ACL manifest; settings/overlay/picker/tray/animation capabilities separated; static security contract tests | Negative IPC invocations in real packaged webviews |
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
the initial selection was empty and the resulting range covers the editable document, or
both ranges expose the exact same complete text when Chromium gives them different endpoints.
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

## Initial candidate delivery, 1.1.0-rc.1

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

## Candidate delivery, 1.1.0-rc.2

[The qualified workflow](https://github.com/duartelcunha/Ember/actions/runs/33945377139)
passed the three-platform checks, built and signed the Windows updater artifacts, verified
the uploaded files and published a prerelease. Tag `v1.1.0-rc.2` points to
`970a21c7b030500d101767201eedf0cfe028cecb`. The downloaded installer was independently verified
locally and installed with `/S /UPDATE` after a fresh recovery copy. Both executable and
uninstall registration report `1.1.0-rc.2`. Existing profile and project data were preserved,
the profile fits the displayed 8 KiB budget, and retention remains disabled. Stable latest
continues to be `v1.0.0`.

The current session exposes one 1920 by 1080 monitor. The two-monitor smoke test stopped at
its environment requirement. An explicit single-monitor attempt stopped before any shortcut
because the fixture could not acquire foreground focus. No new native positioning or paste
success is claimed for rc.2. The five-position two-monitor evidence above belongs to rc.1.

The startup log contained the rc.2 marker, zero error-level lines, zero panic markers and zero
permission-denial markers in the inspected startup tail. A ten-second idle sample of the
Ember parent process measured zero additional CPU seconds and 29.6 MiB working set; WebView
child processes and GPU activity were not measured. This is not a complete performance budget.

A proposed packaged-webview permission test using temporary remote debugging was blocked by
the execution environment's automatic approval review. It did not run. Static ACL and mocked
browser checks remain the available permission evidence. A separate native refinement fixture
also failed to acquire focus and sent no request. Neither failure is counted as a passing test.

Fresh dependency scans still report npm total 0 and the existing optional `rkyv` advisory.
`cargo tree --locked -i rkyv --target all` reports `nothing to print`. Gitleaks scanned the three
new commits and reported one private-key finding in synthetic profile redaction fixtures;
inspection confirmed literal `synthetic-body` and `x` test payloads, not a credential.

Follow-up workflow cleanup removes an unsupported `toolchain` input. The pinned action
[embeds Rust 1.96.1](https://raw.githubusercontent.com/dtolnay/rust-toolchain/e1648915a4fdfbb5afab579a8afeb8eba8f6528c/action.yml),
which the job logs also report. The compiler version is unchanged.
