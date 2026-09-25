## 2026-09-23: Recover refinement in Windows editors with inconsistent UI Automation focus
- Mode: full
- Verdict: GO WITH CONDITIONS (verified automatic replacement and bounded manual clipboard handoff)
- User decision: followed (approved the implementation plan)
- Deciding argument: Skeptic: removing the focus flag alone cannot establish the editable target when UIA returns a pane.
- Residual risk: a provider may report a stable editor while keyboard input has moved to another field.
- Kill criteria: any wrong-field paste, password capture, or inability to distinguish the original editor during a same-window focus change.
- Confidence: medium
- Review on: 2026-10-23
- Outcome: pending review

## 2026-09-24: Close Ember with a final Windows editor-recovery release
- Mode: full
- Verdict: GO WITH CONDITIONS (Windows 1.3.1, then maintenance only)
- User decision: followed (approved security review, squash merge and release)
- Deciding argument: Skeptic: the exact updater artifact must prove target-safe paste before stable promotion.
- Residual risk: a future editor update can change focus or selection timing; Windows publisher signing is unavailable.
- Kill criteria: wrong-field paste, protected-text capture, failed CI, invalid updater signature, or failed rollback.
- Confidence: medium
- Review on: 2026-10-24
- Outcome: pending review

## 2026-09-25: Review complete refinement before paste and recover refused results
- Mode: full
- Verdict: GO WITH CONDITIONS (readable no-focus preview and explicit recovery copy)
- User decision: followed (approved the next product slice with "siga")
- Deciding argument: Skeptic: a partial preview or stale webview state would give false confidence or expose private text.
- Residual risk: an opted-in visible result can be seen in a screen capture.
- Kill criteria: unreadable long result, stale result after any exit, wrong-target paste, or clipboard change without a user gesture.
- Confidence: medium
- Review on: 2026-10-25
- Outcome: pending review

## 2026-09-25: Integrate review code before native release qualification
- Mode: full
- Verdict: GO WITH CONDITIONS (squash-merge PR #61, close unreviewed bot PR #25, keep release blocked)
- User decision: followed (requested a clean merged GitHub state)
- Deciding argument: Judge: mainline integration does not publish a stable update; the release PR and manual promotion are separate.
- Residual risk: real Codex focus, selection and clipboard behavior is unproven, and a later promotion could overlook this gate.
- Kill criteria: changed PR head or checks, automatic stable publication, or additional unreviewed privacy changes.
- Confidence: medium
- Review on: 2026-10-25
- Outcome: pending review

## 2026-09-25: Publish the next candidate only after draft artifact verification
- Mode: full
- Verdict: GO WITH CONDITIONS (1.4.0-rc.1 as an opt-in prerelease, stable unchanged)
- User decision: followed (approved the prerelease with "siga")
- Deciding argument: Skeptic: the previous candidate became public before its signed installer finished building, so the draft and source tag must be checked before the build.
- Residual risk: opt-in users may encounter unqualified Codex or ChatGPT focus and paste behavior.
- Kill criteria: release becomes public before verification, tag or target differs from the merge commit, CI or signature fails, or the stable channel changes.
- Confidence: medium
- Review on: 2026-10-25
- Outcome: pending review
