# Agent Usage

Agent Usage reads what Claude Code and Codex have spent on this computer:
tokens, cost, models, projects and the live rate-limit windows. A bundled
collector script walks the local session logs and writes `usage.json`;
nothing leaves the machine except the rate-limit lookups the providers
themselves answer.

## Commands

- `vr usage summary`, `sources`, `models`, `projects` and `calendar` read the
  collected data. `--days` narrows the window and `--source` picks collectors.
- `vr usage limits` shows the session and weekly windows as gauges.
- `vr usage alerts` explains what the alert notifier would do right now,
  without posting anything.
- `vr usage export` renders shareable PNG (or `--svg`) cards that never show a
  repository, folder, chat title or cost.
- `vr usage refresh` runs the collector; `--progress` prints each phase.
- `vr alerts state`, `test` and `clear` inspect, try and reset the notifier.

Every command accepts `--json`.
