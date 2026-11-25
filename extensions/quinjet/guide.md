# Quinjet

Quinjet opens a git worktree as a review workspace. This extension lists your
recent Quinjet projects and opens a worktree with one click.

## Commands

- `vr quinjet projects` lists recent projects and their worktrees.
  `--machine` looks on one of your SSH machines instead of this computer.
- `vr quinjet worktrees <path>` lists every worktree of one project.
- `vr quinjet open <path>` prints the command that opens a worktree, without
  running it. `--theme` and `--appearance` are passed through to Quinjet.
- `vr quinjet launch <path>` opens the worktree in your terminal.

Every command accepts `--json`.
