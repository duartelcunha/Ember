# Data handling and recovery

This describes the hardening checkout, not a certification of an installed release.

## Results and diagnostics

New installations retain results in session memory. Configuration schema 1 also resets the
legacy default retention flag to off and preserves the original configuration in a
`config.json.v0-<timestamp>.bak` recovery file. Existing plaintext `refine_cache.json` is not
loaded or deleted during migration. Settings provides an explicit delete action.

Enabling result retention creates `refine_cache.enc` in the configuration directory. The
envelope uses AES-256-GCM with a fresh random nonce per write and a 32-byte key in the OS
credential vault. A failed authentication or unavailable vault does not load the file.
Results expire after 24 hours. Expiry is checked on reads and writes and every minute while
the application runs. An application that is not running cannot perform disk cleanup.

Disabling retention invalidates in-flight write permissions and deletes retained result
files. Session memory remains available until expiry or exit. A later opt-in does not persist
results obtained while retention was disabled. Storage failures are logged as categories;
native vault failure and concurrent policy tests remain part of qualification.

Prompt diagnostics is a separate, default-off option. Its `prompts.jsonl` is plaintext and
contains user text. Turning logging off prevents new entries but does not erase existing
diagnostics. Review and delete that file explicitly through the log directory when finished.
Ordinary logs are less verbose but have not yet passed the full privacy review.

## Context and network

Selected text and the prepared context go to configured model providers. Provider retention
policies apply separately. Project extraction is a distinct user action which sends redacted
source data. A window title can select an already registered project; it cannot authorize a
new filesystem source. The context inspector shows the last resolved request snapshot.

Known project files and explicit `@relative/path.md` lines are read inside the selected
canonical directory. Import cycles and excluded sources produce warnings. Limits are
512 KiB combined, 32 source files and eight import levels. Imports must occupy their own line.
Global profiles no longer discover or read ambient `CLAUDE.md` or `AGENTS.md` files during
refinement. Personalization can import up to eight explicitly selected Markdown/text files,
with 64 KiB per file and 256 KiB combined. A five-second caller deadline and one retained
worker lease bound stalled filesystem reads. Import is local and does not call a provider.

The extraction keeps recognized writing preferences and technical facts, excludes code,
unknown sections and common operational directions, and removes secret-like content before
showing a draft. Exclusions are visible. This conservative heuristic can omit useful content
and does not prove semantic immunity to malicious instructions. The user reviews and saves
the draft before it becomes context. No raw-source fallback exists.

Saved profiles have a 2,000-byte UTF-8 limit. Oversized drafts remain editable and cannot be
saved; legacy oversized overrides stop refinement before a provider request. Existing manual
preferences remain intact. Legacy automatic file loading is disabled with a Settings notice.
Source paths and SHA-256 fingerprints are provenance for approved snapshots, not permission
to reload files. Import again to review changes. New manual saves also reject detected secrets.
Full project scope hierarchy and adversarial instruction evaluation remain release work.

OpenAI-compatible credentials are bound to their HTTPS endpoint. Changing that endpoint does
not send an existing connection's credential to the new server. Plain HTTP local connections
are currently unavailable. OAuth uses an experimental integration with no production support
contract established by this work.

## Recovery and rollback

Exit Ember before manipulating configuration. Copy the complete configuration directory and
preserve access to its OS vault before testing migration on a disposable account. Do not
share those copies, because configuration, projects and diagnostics may contain private text.

Corrupt JSON is preserved in a unique `.corrupt-*.bak` file before recovery. Future schema
versions are not overwritten. Failed or conflicting saves return an error; reload Settings
before retrying. Configuration replacement is atomic, with unique temporary files.

To return to the previous build, stop Ember and preserve the schema 1 directory first. Restore
the original version 0 configuration from its backup only in a compatible previous version.
That version's retention behavior was enabled by default, so review it before resuming work.
Encrypted results cannot be imported by the old plaintext reader. Do not delete the encryption
key while you intend to retain encrypted results. No rollback installation has been exercised
in this environment.

The Windows uninstaller delegates data removal to Tauri's explicit delete-data checkbox.
Only that choice, outside update mode, invokes the owned-credential cleanup helper. Neither
the helper nor the install/update/uninstall data matrix has been executed on a user account.
