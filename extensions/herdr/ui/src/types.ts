export interface HerdrSession {
  name: string;
  running: boolean;
  default: boolean;
  session_dir: string;
  socket_path: string;
  error?: string | null;
}

export interface HerdrAgent {
  id: string;
  session: string;
  kind: string;
  status: "blocked" | "working" | "unknown" | "done" | "idle";
  title: string;
  workspace: string;
  cwd: string;
  paneId: string;
  focused: boolean;
  attachCommand: string;
}

export interface HerdrBoard {
  installed: boolean;
  executable: string | null;
  sessions: HerdrSession[];
  agents: HerdrAgent[];
  error: string | null;
}

export interface AttachedTerminal {
  id: string;
  name: string;
  pid: number;
  cwd: string;
  status: string;
  owned: boolean;
}

export interface TerminalChunk {
  id: string;
  bytes?: string;
  cursor?: number;
  status: string;
}
