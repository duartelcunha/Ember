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
