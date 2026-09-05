# Ember 1.1.0-rc.2

Windows evaluation candidate. Production approval remains blocked. This release is excluded
from the stable automatic-update channel. macOS and Linux functional parity is incomplete.

This candidate adds explicit, reviewed global profile imports. Ambient agent files are no
longer loaded automatically. Import selected Markdown/text files in Personalization, review
the extracted preferences and technical facts, then save. Exclusions and source fingerprints
remain visible; oversized profiles are refused rather than silently truncated. Existing
manual preferences remain preserved.

Project colour responses now preserve newer draft edits and retire with their editor.
Browser regressions cover switching projects during distillation and editing a name while
a colour request is pending. Cancelled pointer gestures restore the colour-wheel preview.

The floating orb, project pills and picker now share measured cursor-following geometry,
monitor transitions and state snapshots. Projects expose resolved context and protect drafts
from obsolete asynchronous responses. Text protection preserves code, paths and Unicode.

Replacement validates the native window, accessible editable element, selection range and
original text. Unsupported editors may be refused. Confirmation shows original and result;
cancelled or obsolete runs cannot authorize a new application. Generic terminal replacement
is disabled until terminal-specific adapters are qualified.

Results stay in memory by default. Optional retained results use authenticated encryption
and the system vault. Configuration revisions, endpoint-bound credentials, retention and
logout generations protect against stale writes. Existing plaintext results are preserved
for an explicit migration/deletion choice; upgrading does not silently delete them.

Updater artifacts are verified against Ember's existing public key before publication.
This cryptographic updater signature is separate from Windows Authenticode: the installer
does not have a Windows publisher certificate and may display an unknown-publisher warning.

Open qualification includes continuous input generation, clipboard races, native browser/editor
compatibility, mixed-DPI/hotplug behavior, screen readers, and installer recovery. Automated
checks and browser fixtures do not establish native production readiness. See
[the implementation ledger](https://github.com/duartelcunha/Ember/blob/main/docs/production-readiness.md).

Back up the application configuration and installation before evaluating this candidate.
Keep the backup until rollback and all applications used with Ember have been qualified.

For an existing Windows installation, run the candidate installer with `/UPDATE` (or `/S /UPDATE` for a silent upgrade). Do not uninstall the old version first: older uninstallers can remove configuration and credentials. Back up configuration before upgrading.
