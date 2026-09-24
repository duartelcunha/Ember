# Threat model for Ember 1.3.1 on Windows

## Assets and boundaries

Assets include the selected text, its destination, clipboard formats, project sources,
provider credentials, retained results and release artifacts. The Rust core makes decisions
without I/O; the native shell owns filesystem, vault, network and input access. Webviews
receive only their declared application commands. Provider keys are not returned to JS.

Untrusted inputs include model responses, provider error bodies, repository Markdown,
window titles, custom URLs, IPC parameters and persisted files. A repository document is
data. It does not authorize commands, filesystem mutation, deployment or agent operations.

## Controls and remaining risks

| Threat | Current control | Remaining proof or implementation |
| --- | --- | --- |
| Paste into another window or selection | Exact HWND/process, focused HWND, Windows UIA element/range lease, recapture comparison and run phase checks; full-field selection also requires the editable document's complete text to match when UIA endpoints differ | Complete input-generation continuity, app compatibility and native application matrix |
| Clipboard data loss or stale copy | Bounded Windows format snapshot, no terminal stale fallback, ownership-aware restore; manual handoff checks the clipboard revision under the write lock | Ownership gaps around synthetic capture, delayed rendering, unsupported formats and real clipboard races |
| Alteration of code or Unicode | Mask before normalization, exact marker cardinality/order, restore after cleanup | Ambiguous command fragments and multilingual evaluation |
| Prompt injection or secret disclosure | Framed/redacted sources, full PEM block removal, no raw fallback, authorized directory checks, explicit reviewed global imports | Full project hierarchy, semantic extraction limits and adversarial model evaluation |
| Credential sent to a changed server | HTTPS endpoint-bound vault references, no redirects, generation-checked validation | Named/local unauthenticated connections and production OAuth support contract |
| Stale logout or retention commit | Generation checks, serialized writes, separate session/persistent caches | Deterministic native race and vault-failure testing |
| Incomplete or duplicated paid request | Bounded response reads, explicit stream completion, uncertain-result handling, no automatic replacement request after failed join | End-to-end billing and provider fault matrix |
| Privileged overlay invocation | Explicit application command manifest and window capabilities | Negative invocation tests against real packaged webviews |
| Premature distribution | Explicit prerelease channel, pinned build tooling and CI matrix | Signing, notarization, artifacts, manifests and qualified promotion procedure |

The 2026-09-24 dependency review updated `rustls` from 0.23.41 to 0.23.45 and
`event-listener` from 5.4.1 to 5.4.2, closing those dependency advisories. `rustls`
is used by the Windows provider and updater clients. `cargo audit` still reports
RUSTSEC-2026-0235 in `rkyv` 0.7.46, pulled into the
lockfile as an optional `rust_decimal` feature via `byte-unit` and `tauri-plugin-log`.
`cargo tree --locked --target all -i rkyv` finds no enabled path, and Ember never reads
rkyv archives. This is a lockfile finding, not an observed Windows execution path.
The six unmaintained dependency notices and the old `glib` unsoundness are tracked as
upstream maintenance risk; `glib` is outside the Windows build. The JavaScript dependency
audit reported zero vulnerabilities. The Git-history secret scan found five synthetic
test fixtures and no verified credential; it cannot prove undiscovered secrets are absent.
GitHub vulnerability alerts, secret scanning with push protection, and private vulnerability
reporting were enabled on the public repository on 2026-09-24. Automated security update PRs
remain disabled; an alert requires a maintainer decision and a qualified release.

Compromise of the local user account, OS or a configured provider is outside the vault's
protection boundary. The vault and encryption do not protect decrypted text inside a running
compromised process. Prompt markers reduce exposure but do not prove immunity to prompt
injection. Windows 1.3.1 stable promotion requires the exact signed updater artifact to pass
native Codex editor, wrong-target, clipboard and rollback checks. macOS and Linux remain
unsupported until separately qualified.

For Windows Edit controls whose provider cannot supply a verifiable selection, the manual
route requires a non-password, enabled, writable focused control and a fresh explicit
selection copied after the hotkey. It never selects the entire field or injects paste keys.
The result replaces the clipboard only if its revision is still the one recorded after
capture. A changed clipboard remains untouched and the result stays in the refine cache.
Unknown password state, mixed or read-only text attributes, and controls that cannot be
classified remain closed. An unsupported selection read-only attribute permits automatic
replacement only when an Edit control's ValuePattern and document range independently prove
editability. This fallback does not establish universal field detection or
production qualification.
