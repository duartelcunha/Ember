# Threat model for the hardening checkout

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
| Paste into another window or selection | Exact HWND/process, focused HWND, Windows UIA element/range lease, recapture comparison and run phase checks | Complete input-generation continuity, app compatibility and native application matrix |
| Clipboard data loss or stale copy | Bounded Windows format snapshot, no terminal stale fallback, ownership-aware restore | Ownership gaps around synthetic capture, delayed rendering, unsupported formats and real clipboard races |
| Alteration of code or Unicode | Mask before normalization, exact marker cardinality/order, restore after cleanup | Ambiguous command fragments and multilingual evaluation |
| Prompt injection or secret disclosure | Framed/redacted sources, full PEM block removal, no raw project fallback, authorized directory checks | Global source authorization, semantic extraction and adversarial model evaluation |
| Credential sent to a changed server | HTTPS endpoint-bound vault references, no redirects, generation-checked validation | Named/local unauthenticated connections and production OAuth support contract |
| Stale logout or retention commit | Generation checks, serialized writes, separate session/persistent caches | Deterministic native race and vault-failure testing |
| Incomplete or duplicated paid request | Bounded response reads, explicit stream completion, uncertain-result handling, no automatic replacement request after failed join | End-to-end billing and provider fault matrix |
| Privileged overlay invocation | Explicit application command manifest and window capabilities | Negative invocation tests against real packaged webviews |
| Premature distribution | Explicit prerelease channel, pinned build tooling and CI matrix | Signing, notarization, artifacts, manifests and qualified promotion procedure |

Compromise of the local user account, OS or a configured provider is outside the vault's
protection boundary. The vault and encryption do not protect decrypted text inside a running
compromised process. Prompt markers reduce exposure but do not prove immunity to prompt
injection. Production approval remains blocked while the listed integrity controls or their
native evidence are incomplete.
