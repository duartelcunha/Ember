# Native qualification evidence

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
