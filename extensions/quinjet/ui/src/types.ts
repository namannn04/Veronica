export interface QuinjetWorktree {
  path: string;
  head: string;
  branch: string | null;
  current: boolean;
  bare: boolean;
  detached: boolean;
  locked: string | null;
  prunable: string | null;
}

export interface QuinjetProject {
  name: string;
  commonDir: string;
  worktrees: QuinjetWorktree[];
}

export interface QuinjetBoard {
  installed: boolean;
  projects: QuinjetProject[];
}
