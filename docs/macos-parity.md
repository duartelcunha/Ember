# macOS parity: implementation spec

Everything below is the remaining work to ship Ember on macOS. It is written to be picked up in a
session running **on a Mac** (the native code cannot be compiled or tested on the Windows dev box, so it
was intentionally not written blind). The rest of the codebase is already cross-platform-ready: the
non-Windows code paths return safe stubs, all the decision logic in `ember-core` is portable, and the
project-context design already accounts for the macOS title source.

The Windows behavior must stay untouched: every change here is gated behind `#[cfg(target_os = "macos")]`
or `[target.'cfg(target_os = "macos")'.dependencies]`.

## 1. Dependencies (`src-tauri/Cargo.toml`)

Add a macOS-only dependency block (mirrors the existing Windows one):

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.5"
objc2-app-kit = { version = "0.2", features = ["NSWorkspace", "NSRunningApplication", "NSPasteboard"] }
objc2-foundation = { version = "0.2", features = ["NSString", "NSURL", "NSArray"] }
core-graphics = "0.24"          # CGEvent for synthetic Cmd+C / Cmd+V
accessibility-sys = "0.1"       # AXUIElement for the focused-window title
```

Pin exact versions against what compiles; the API shapes below are stable across recent releases.

## 2. Foreground detection (`src-tauri/src/foreground.rs`)

Two `#[cfg(target_os = "macos")]` functions to sit beside the Windows ones:

- **`foreground_exe() -> Option<String>`**: `NSWorkspace::sharedWorkspace().frontmostApplication()` returns
  an `NSRunningApplication`; read `executableURL` (or `bundleURL`) and return its filesystem path. Feed it
  to the existing pure `is_terminal_exe` (extend `TERMINALS` with mac names/bundle ids if you keep terminal
  detection, but see §4).
- **`foreground_title() -> Option<String>`**: get the frontmost app's `processIdentifier`, then
  `AXUIElementCreateApplication(pid)` -> copy `kAXFocusedWindowAttribute` -> copy `kAXTitleAttribute`.
  Requires the Accessibility permission (already needed for the CGEvent paste in §3, so it is free).
  This is the title that feeds the existing pure `ember_core::project::extract_path`, so project-context
  detection lights up on macOS with no change to the pure crate.

## 3. Selection / paste (`src-tauri/src/selection.rs`, `RealIo`)

macOS copies and pastes with **Cmd**, not Ctrl, in every app including terminals. Per-OS the copy/paste
modifier: Ctrl on Windows, Cmd (`kCGEventFlagMaskCommand`) on macOS, via `core_graphics::CGEvent` key-down /
key-up for the C / V key codes. Provide macOS impls (or safe defaults) for:

- `physical_modifiers()` -> read live modifier state with `CGEventSource::keyState` (so the sentinel capture
  can neutralise a held Cmd exactly as it neutralises Ctrl on Windows).
- `has_unpreservable_clipboard()` -> inspect `NSPasteboard::generalPasteboard().types` for file-URL / RTF
  types; return `false` if you cannot classify (never abort a normal text refine).

Keep the sentinel-based capture technique unchanged; only the key + modifier differ.

## 4. Terminal handling

Because mac copy is Cmd+C everywhere, the Windows Ctrl+Shift+C/V terminal special-case largely collapses.
Gate `is_terminal_foreground()` to `#[cfg(windows)]` (already stubbed to `false` elsewhere) and drive the
mac copy/paste with Cmd+C/V unconditionally. No mac terminal list needed.

## 5. Window behavior

- `tauri.conf.json` `app.macOSPrivateApi: true` is required for the transparent overlay / splash windows.
- The always-on-top orb over fullscreen apps needs an elevated NSWindow level and
  `NSWindowCollectionBehavior` that joins all spaces. Tauri v2 sets `alwaysOnTop`; if the orb does not float
  above fullscreen apps, set the level / collection behavior on the `overlay` window at creation via the
  `objc2-app-kit` handle from `WebviewWindow::ns_window()`.

## 6. Packaging (`tauri.conf.json`)

Do this only once §2 and §3 work on a Mac (a dmg built before then would ship a broken paste path):

- `bundle.targets`: add `"app"` and `"dmg"` (Tauri builds only the current platform's targets, so the
  Windows `nsis` build is unaffected).
- `bundle.icon`: `icons/icon.icns` is already listed.
- Add `bundle.macOS`: `{ "minimumSystemVersion": "11.0", "category": "public.app-category.productivity" }`.
- Runtime Accessibility prompt: on first run call `AXIsProcessTrustedWithOptions` with the prompt option so
  the user grants Accessibility (needed for both the CGEvent paste and the AXTitle read). Surface a clear
  message in Settings when it is not granted.

## 7. CI (`.github/workflows/release.yml`)

- Add `macos-latest` (Apple Silicon) to the build matrix.
- Signing + notarization via `tauri-apps/tauri-action`, wired to repo secrets:
  `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
  `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID`. Needs an Apple Developer account.
- Ensure the updater's mac artifacts (`.app.tar.gz` + signature) are attached to the release so
  `latest.json` covers macOS.

## 8. Uninstall / orphan cleanup

macOS uninstall is drag-to-trash (no hook). Document that `~/Library/Application Support/com.deleg8lab.ember`,
`~/Library/Logs/com.deleg8lab.ember`, and the Keychain items (`Ember` service) should be removed manually,
or add a small "reset all data" button in Settings that clears them.

## 9. Verification on the Mac

1. `cargo test --workspace` (the pure tests already pass cross-platform).
2. `npm run tauri dev`; grant Accessibility when prompted.
3. Hotkey -> capture -> refine -> paste in a normal editor AND a terminal; confirm Cmd+C/V is used and the
   original clipboard is restored.
4. Confirm the transparent overlay orb and the splash/quit animations render and float correctly.
5. Enable Project context, focus an IDE/terminal in a repo with a `CLAUDE.md`, refine, and confirm it merges
   (check the Diagnostics panel / logs for the detected source path).
6. `npm run tauri build`; sign + notarize; install the dmg and repeat 3-5.
