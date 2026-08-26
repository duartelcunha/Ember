# macOS parity: status + remaining spec

The Windows behavior must stay untouched: every change here is gated behind `#[cfg(target_os = "macos")]`
or `[target.'cfg(target_os = "macos")'.dependencies]`.

## Status

**Already done on this branch (compiles; Windows build unaffected):**
- **§3 Core capture/paste** — the copy/paste modifier is now per-OS: Cmd on macOS (`enigo`'s `Key::Meta`),
  Ctrl elsewhere. `enigo` + `arboard` are cross-platform, so the whole capture -> refine -> paste loop
  works on macOS through the same code. This is the core function of the app.
- **§6 Packaging** — `app.macOSPrivateApi: true` (+ the `macos-private-api` Tauri feature) for transparent
  windows, `bundle.targets` now includes `app` + `dmg` (Tauri builds only the host's targets, so the
  Windows `nsis` build is unchanged), and `bundle.macOS.minimumSystemVersion`.

**Remaining (needs a Mac to compile + test):** the frontmost-app read, which now gates two features
instead of one (§2 and §4), any window-level tweak if the orb doesn't float over fullscreen apps (§5),
CI signing/notarization (§7), and the runtime Accessibility prompt (below). These were intentionally not written blind: a native
objc compile error would break the whole macOS build, which is worse than the graceful degradation the app
has today (on macOS, project-context detection returns `None` and refining falls back to the global profile,
exactly as if no project were detected).

> **Accessibility permission (required on macOS):** `enigo` needs the Accessibility permission to
> synthesize the Cmd+C / Cmd+V keystrokes. On first run the app must be granted Accessibility in
> System Settings > Privacy & Security > Accessibility, or the paste silently no-ops. Add a runtime
> `AXIsProcessTrustedWithOptions` prompt and a clear Settings note when it is not granted.

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

Because mac copy is Cmd+C everywhere, the Windows Ctrl+Shift+C/V terminal special-case largely collapses,
so `is_terminal_foreground()` stays `#[cfg(windows)]` and mac copy/paste runs Cmd+C/V unconditionally.

**But mac now needs terminal detection for a second reason, and this is a real gap, not a nicety.**
The select-all fallback (fire the hotkey with nothing selected, and Ember selects the field you are typing
in and refines it) must never run in a terminal: `Cmd+A` in Terminal.app or iTerm selects the entire
scrollback, and pasting over that would be destructive. Windows decides this with `is_terminal_foreground`,
which on macOS always returns `false` — every terminal would look like an ordinary app.

Until the frontmost-app read in §2 exists, `foreground::select_all_is_safe_here()` returns `cfg!(windows)`,
so **the fallback is off on macOS**: firing the hotkey with nothing selected shows "Select text first",
exactly as before the feature existed. Nothing regresses, but the feature is missing there, and the
Diagnostics panel says so in words rather than leaving the user to wonder why a toggle they switched on
does nothing.

The second safety net is also Windows-only: a select-all capture always goes through the preview gate, and
`preview_hook` uses a Windows keyboard hook. So enabling the fallback on macOS means shipping **both** the
frontmost-app read and a mac preview gate, not just the first one.

To close it: implement `foreground_exe()` per §2, extend `TERMINALS` with the mac binaries
(`Terminal`, `iTerm2`, `alacritty`, `kitty`, `WezTerm`, `Ghostty`, `warp`) or match on bundle ids, and
change `select_all_is_safe_here()` to `true`. The pure sequencing in `ember_core::selection` already takes
`select_all_fallback` as a parameter and already refuses to run it when `terminal` is true, so no logic
changes there, only the platform gate.

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

1. `cargo test --workspace` (the pure tests already pass cross-platform; `ember-core` also
   `cargo check --target aarch64-apple-darwin` clean from a Windows host, though the `ember` shell crate
   cannot be cross-checked without a C toolchain for the target).
2. `npm run tauri dev`; grant Accessibility when prompted.
3. Hotkey -> capture -> refine -> paste in a normal editor AND a terminal; confirm Cmd+C/V is used and the
   original clipboard is restored. Try each of the three shortcuts (main, Polish, Turbo) and confirm the
   `CmdOrCtrl` accelerators map to Cmd, not Ctrl.
4. With the select-all fallback still gated off, fire the hotkey with nothing selected and confirm it says
   "Select text first" rather than selecting anything. Only after §2 lands should this refine the field,
   and the first place to test it then is a terminal, where it must still refuse.
5. Confirm the transparent overlay orb and the splash/quit animations render and float correctly.
6. Enable Project context, focus an IDE/terminal in a repo with a `CLAUDE.md`, refine, and confirm it merges
   (check the Diagnostics panel / logs for the detected source path).
7. `npm run tauri build`; sign + notarize; install the dmg and repeat 3-6.
