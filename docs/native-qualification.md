# Native qualification evidence

## Offline floating geometry harness

Build the current frontend with `npm run build`, then build the isolated example with
`cargo build -p ember --example floating_native --features native-qualification --locked`.
Run `powershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File scripts/native-floating-smoke.ps1 -CaptureHelper <path> -RequireMixedDpi`
for the mixed-DPI gate. The helper contract is the same as the picker capture helper below.
Omitting `-RequireMixedDpi` allows a separately labelled same-scale smoke test.

This example loads the production overlay and geometry code without settings, credentials,
hooks or provider calls. It does not install or replace Ember. It cycles through orb, long
project name, comparison and hint scenes, with pointer positions on both monitor centers
and lower right edges. Focus, work-area bounds, screenshots and the executable hash are
recorded locally. Run it without other UI automation or manual focus changes. Its own
process exits after 25 seconds even if the controller stops. The controller restores the
pointer and closes its child processes in cleanup.

The [current implementation record](floating-context-refinement.md#evidence-and-delivery-gate)
separates this harness from installed-candidate, mixed-DPI and input qualification.

## Windows picker smoke test

Run `powershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File scripts/native-picker-smoke.ps1` from the repository after installing the candidate. This local test requires two connected monitors, a single running Ember instance, the `CmdOrCtrl+Shift+P` picker shortcut and no active Ember window or operation.

The harness briefly covers the desktop with synthetic windows. It opens the actual installed picker with an injected shortcut, moves the pointer through five positions and cancels with Escape. It asserts that the native picker surface matches the active monitor work area and that foreground focus stays on the synthetic source window. Cancellation must close the surface and preserve the configured project selection. It restores the pointer and closes its own windows when it exits.

An optional `-CaptureHelper <path>` accepts a PowerShell screenshot helper supporting `-Mode temp -Region x,y,width,height`. Reports go to `target/native-qualification`. Screenshots are local diagnostic evidence and may include project names; review before sharing them. Neither report nor screenshot is automatically uploaded.

The test does not read or replace clipboard content, issue a refinement request, select a different project, change display settings or qualify physical keyboard/mouse event tails. It is a native integration smoke test, not the full production matrix.

## Baseline observed on 2026-09-05

The existing installed executable identified itself as 0.10.0; its uninstall registry still identified 0.9.1. Two monitors were connected:

| Monitor | Physical bounds | Work area |
| --- | --- | --- |
| Primary | `(0, 0)` to `(2560, 1440)` | `(0, 0)` to `(2560, 1392)` |
| Secondary | `(2560, 87)` to `(4480, 1167)` | `(2560, 87)` to `(4480, 1119)` |

The baseline failed at `primary-center`. After the pointer moved to `(800, 450)`, the picker window retained the secondary work area `(2560, 87)` to `(4480, 1119)`. Foreground focus remained on the synthetic source window. The primary-region screenshot contained the synthetic background with no picker. This is a reproduced native positioning failure in the old installed build.

## Candidate observed on 2026-09-05

The published [1.1.0-rc.1 candidate](https://github.com/duartelcunha/Ember/releases/tag/v1.1.0-rc.1) was built from `5fc4b14d56eeb6f76fa4ba1b54e91bb2a0f367aa`. The downloaded installer passed verification against the application's updater public key:

```text
Updater signature valid. SHA256: fc1e56ff7ffcd03bdcae6ed68bbb258c615f934d7baca7a455b3898a313b5708
```

The installed executable and uninstall registration both report `1.1.0-rc.1`. The installer was invoked with `/S /UPDATE`, after preserving the old executable, configuration and uninstall registration in a local recovery directory. Migration produced schema 1 with `keep_results=false`. The project objects matched the backup, the legacy results file retained the same SHA-256, and a version 0 configuration recovery copy exists. No credential was exported. The Settings window rendered and showed the existing signed-in account and saved fallback credential. This does not establish provider request success.

The native smoke command, with the optional screenshot helper, exited 0:

```text
Native picker smoke passed: 5 positions, 2 monitors, focus preserved, Escape closed the surface.
```

All five screenshots were inspected locally. The picker appeared at the primary center, primary right edge, secondary left edge, secondary bottom-right corner and back on the primary monitor. It remained readable and inside the work area. Each native surface matched the current monitor; foreground focus stayed on the synthetic source window. Escape closed the picker without changing project selection.

This closes the reproduced old-build monitor placement failure for this scenario. Physical input tails, drag interactions, mixed DPI, hotplug, resume, remote sessions, latency/GPU budgets and the wider application/clipboard matrix remain unqualified. The installer has a valid updater signature but no Windows Authenticode publisher signature. Interrupted upgrade, rollback and uninstall paths were not executed.

## Candidate 1.1.0-rc.2 on 2026-09-05

[Version 1.1.0-rc.2](https://github.com/duartelcunha/Ember/releases/tag/v1.1.0-rc.2) was built
from `970a21c7b030500d101767201eedf0cfe028cecb` by the successful qualified workflow
[33945377139](https://github.com/duartelcunha/Ember/actions/runs/33945377139). The installer,
detached signature, `latest.json` and `SHA256SUMS.txt` are published. Local verification returned:

```text
Updater signature valid. SHA256: d337240b59423adec90e34289218237ac54f1bb0e3be745cf89805cd09cdc68d
Installed 1.1.0-rc.2.
```

Executable and uninstall registration agree on the version. A recovery copy was created at
`%LOCALAPPDATA%/EmberRecovery/20260905-055643` before `/S /UPDATE`. The saved profile and project
objects match that backup. Retention is off. The legacy plaintext results file was absent both
before and after this upgrade; this upgrade did not remove it. The installer has a verified
updater signature and `Authenticode: NotSigned`.

The environment now reports one primary display, with bounds `(0, 0)` to `(1920, 1080)` and
work area `(0, 0)` to `(1920, 1032)`. The standard test correctly refused a two-monitor claim.
The harness now offers `-AllowSingleMonitor` for explicitly separate single-monitor evidence.
That attempt returned `Fixture did not obtain focus; no shortcut sent` before taking a picker
screenshot. Its synthetic windows were closed by cleanup. The earlier successful two-monitor
record remains valid for rc.1 only. Native qualification of rc.2 remains open.

## Candidate 1.2.0 on 2026-09-09

[Version 1.2.0](https://github.com/duartelcunha/Ember/releases/tag/v1.2.0) was built from
`1db9d816910c7a0be1928bb061035f29e1c27951` (the squash of release PR #46) by release workflow run
[34298979550](https://github.com/duartelcunha/Ember/actions/runs/34298979550). release-please
created it as a full release, as it does for any version without a prerelease part, and the build
job's first step marked it a prerelease before any artifact existed (step "Keep the release off
the stable channel until its artifacts are verified": success). `/releases/latest` stayed on
v1.0.0 throughout the build. The installer, detached signature and `latest.json` were attached
by the same run.

Local verification, from the release commit checked out and with `Cargo.lock` carried to 1.2.0
the way the workflow does it:

```text
Updater signature valid. SHA256: 8845a4b11e3c5e2745852deab6587873808e3a117d93424ca9e04b4b464b3ac6
manifest: version 1.2.0, url https://github.com/duartelcunha/Ember/releases/download/v1.2.0/Ember_1.2.0_x64-setup.exe
manifest signature equals the published .sig
Authenticode: NotSigned
```

Installed with `/S /UPDATE` over a locally built 1.2.0-rc.4 of the same code (itself installed
over 1.2.0-rc.3 earlier the same night), with the configuration backed up before each install.
Installer exit code 0; executable and uninstall registration both report 1.2.0. The configuration
kept its 37 keys, the two projects, the four Credential Manager entries and the retention setting
(off, no retained file). The startup log shows the version, a renewed OAuth token and no
error-level lines.

Environment: Windows 11 23H2 (build 22631), two displays, primary 2560 by 1440 at 100% scaling.
Applications on the machine: Microsoft Edge 152.0.4191.66, Word and Outlook 16.0.20326.20132,
Windows Terminal 1.24.11911.0, Notepad 11.2607.14.0, Brave (version not recorded by the log).

Hands-on runs by Duarte on this installed build, 03:46 to 03:49 local time on 2026-09-09, read
back from `Ember.log` (32 lines since the 1.2.0 start, zero error or warning lines). Preview
before paste was on, so every run went through the confirmation and its Enter was consumed by
the hook:

| Run | Application | What the log recorded |
| --- | --- | --- |
| 1 | Notepad 11.2607.14.0 | capture armed (5 chars); OpenAI-compatible `gpt-5.6-terra` answered in 2035 ms; gate ACCEPT; clipboard guard revision 271 -> 271, armed; `Pasted` |
| 2 | Brave, web text field | capture armed (5 chars); exact cache hit, no model call; gate ACCEPT; clipboard guard revision 310 -> 310, armed; `Pasted` |
| 3 | Windows Terminal 1.24.11911.0 | detected as terminal; capture armed (5 chars); exact cache hit; gate ACCEPT; `HandedOff`; the clipboard held the five-character result with no newline afterwards |

Clipboard guard over the pastes: 2 armed, 0 refusals. The same five characters were used in all
three runs on purpose: runs 2 and 3 show a paid result served twice more without a second call.
Not exercised in this pass, and therefore still unproven for 1.2.0: Word and Outlook, clipboard
history switched on, preview off, the retention toggle and logout. Injected input was not used
for any of this evidence: it gives Ember empty captures and clicks beside the target.

Promotion followed the runs, on 2026-09-09, with
`scripts/verify-prerelease.ps1 -Tag v1.2.0 -Promote -NotesFile docs/release-notes/1.2.0.md`
from the release commit: the artifacts were verified again, `SHA256SUMS.txt` was uploaded and one
edit made v1.2.0 the full release. `/releases/latest/download/latest.json` then reported version
1.2.0 with the expected installer URL, so installed copies of 1.0.0 are offered it by the updater.

Not proven by this record in any case: mixed-DPI and hotplug behaviour, remote sessions,
applications other than those listed, and any macOS or Linux behaviour.

## 1.3.0-rc.1 hands-on pass, 2026-09-18

Environment: Windows 11 Home (build 22631), two displays, primary 2560 by 1440 with a work area
of 2560 by 1392, secondary 1920 by 1080 starting at y=87. The CI installer
`Ember_1.3.0-rc.1_x64-setup.exe` (2,786,523 bytes) was downloaded from the release and installed
with `/S` over a local build of the same commits, so this pass is on the artifact that shipped,
not on a developer build. Configuration, the saved shortcut and credentials survived the install.

Read back from `Ember.log`: every line since the 1.3.0-rc.1 start, zero warning and zero error
lines.

| What | When | What the log recorded |
| --- | --- | --- |
| Quit from the tray menu | 11:08:37 | menu opened at (1873,1256); menu closing (blur=false); `quit: requested`, animation shown, finalizing, exiting, exit requested (quitting=true), event loop exited, all within one second |
| Refine, first press | 17:36:59 | Claude desktop 2.2553.1.0, mode Polish; `guard: refused no_keyboard_focus`; `capture: refused guard_begin`; `outcome=TargetUnverifiable` |
| Refine, second press | 17:37:00 to 17:37:15 | capture armed, 503 chars, no select-all fallback; Gemini `gemini-3.5-flash-lite` answered in 7,014 ms with 485 chars; preview gate ACCEPT with the Enter consumed by the hook; clipboard guard revision 1697 -> 1697, armed; `Pasted`; `outcome=Success { provider: "Gemini" }` |

The first press is worth recording rather than rounding away: in that application the
accessibility layer did not report keyboard focus on the field yet, the guard refused, and the
press one second later went through. The refusal is the guard doing its job, not a crash, and it
is visible in the log only because this release made every refusal name itself.

Verified earlier on local builds of the same commits, not repeated on this artifact: the tray
recovering from a menu window left visible and empty (`tray: the open flag was already set
(visible=Ok(true)); asserting the menu again`, followed by the menu opening), and the startup
repair of an unsafe saved shortcut (`saved hotkey 'Shift+E' is not safe to register
(NeedsModifier { key: "e" }); picking another` then `hotkey chosen automatically:
CmdOrCtrl+Shift+E`).

Not exercised in this pass, and therefore still unproven for 1.3.0: Notepad, Word, Outlook,
Brave and Windows Terminal (the refine evidence here is one Electron application), the select-all
fallback, the project picker, preview off, the retention toggle, logout, mixed-DPI behaviour and
any macOS or Linux behaviour. Injected input was not used for any of this evidence: it gives
Ember empty captures and clicks beside the target.

One flake to note for whoever reads CI next: the macOS job failed once on the tray fold
animation (`floating-components.test.mjs`, "never started") and passed on a re-run of the same
commit, with Windows and Linux green both times. The same macOS job has been failing on `main`
since 2026-09-09 on an unrelated settings-fade assertion.

## Codex editor recovery local candidate, 2026-09-24

The isolated Windows candidate on `codex/ember-codex-field-refine` at `dd89ae0` includes the
editor guard fix. In the Codex desktop composer, one run with an explicit selection and one
with the whole field unselected each showed the preview, accepted Enter, and recorded
`applied=Ok(Ok(Pasted))` followed by `Success`. The refined text appeared in the same composer.
The test text was removed and the original clipboard text and formats were restored in both
runs. A separate focus change produced `ForegroundChanged` with no paste. The fixture checks
also refused password and read-only fields and preserved a newer clipboard copy.

The local NSIS installer was installed with `/S /UPDATE` after a verified backup of the previous
installation and configuration. The installed executable reported version 1.3.0. Its SHA-256
is `8CEC1CF138F982FB93D2F7434C661D1EA985AB9223F8A416F52D2DF509B6D86E`. A subsequent
installed run with an explicit selection passed the preview, paste, same-composer, focus,
clipboard restoration and test-text cleanup checks. The installed whole-field trial obtained
a result but stopped when the probe found that focus had changed before confirmation. A later
attempt found a nonempty composer and did not type or call a provider. The installed
whole-field path is therefore not yet proven.

These are local build checks, not checks of the CI-signed release installer. A separate
ChatGPT desktop installation was not qualified. The exact release artifact, whole-field
installed path, changed-destination refusal and recovery to v1.3.0 remain release gates.

## 1.3.1 CI artifact and installation, 2026-09-24

PR #57 was squash-merged as `bbc6877`, and the release PR #58 was squash-merged as
`1abcebc`. CI passed on Windows, macOS and Ubuntu for both the fix and the release commit.
Release run `36037494878` built and uploaded the Windows installer, detached updater
signature and `latest.json` from `1abcebc`. The release is still a prerelease, and
`/releases/latest` still resolves to v1.3.0.

The downloaded `Ember_1.3.1_x64-setup.exe` has SHA-256
`546035796061c6f369128093d7e19dbe997438bfbe8a14747277c73e10b374d7`.
The manifest names version 1.3.1 and the exact installer URL; its signature matches the
downloaded `.sig`. The repository's `verify_update` example accepted that signature against
the configured updater public key. Windows Authenticode reports `NotSigned`, so this does not
establish publisher identity. The signed updater artifact and the verified v1.3.0 recovery
installer are saved outside the checkout under `%LOCALAPPDATA%`.

The official installer ran with `/S /UPDATE` and returned 0. The installed executable reports
1.3.1 and SHA-256 `919434c4d03bacbd9150b80665d1d82a49b2d1d399161716a5395a283ddb0899`.
The config file's hash matched its pre-install backup. Ember started from the installed path,
remained running and logged `Ember 1.3.1 started` without a warning or error in the startup
slice. This proves install and startup only. The desktop session was locked during this pass,
so the exact installed artifact has not yet completed the selected-text, whole-field,
wrong-target, clipboard-restoration or rollback checks in the real Codex composer. Stable
promotion remains blocked on that native qualification.
