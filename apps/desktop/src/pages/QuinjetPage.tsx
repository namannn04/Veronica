import { useCallback, useEffect, useState } from "react";

import { ipc } from "../lib/ipc";
import type { QuinjetBoard, QuinjetWorktree } from "../lib/types";

/**
 * Quinjet.
 *
 * Discovery and launch, over Quinjet's own JSON contract — the same one Edith
 * uses, so a machine with Quinjet installed behaves identically on both.
 *
 * Edith hosts the TUI in an embedded terminal, which is what gives it tabs and
 * a session switcher. Veronica has none and opens the installed terminal
 * instead: the same decision the Herdr page made, for the same reason. Quinjet
 * stays the owner of the review, and this page is how you get into one.
 */
export function QuinjetPage() {
  const [board, setBoard] = useState<QuinjetBoard | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    setBusy(true); setError("");
    try { setBoard(await ipc.quinjetProjects()); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }, []);

  useEffect(() => { void load(); }, [load]);

  const open = async (worktree: QuinjetWorktree) => {
    setError("");
    try { await ipc.quinjetOpen(worktree.path); }
    catch (reason) { setError(String(reason)); }
  };

  if (board && !board.installed) {
    return <div className="page">
      <div className="page-head edith-head"><div><h1>Quinjet</h1><div className="page-sub">Review pull requests and live workspace changes</div></div></div>
      <section className="settings-info">
        <div className="info-glyph">◇</div>
        <h2>Quinjet is not installed</h2>
        <p>
          Quinjet is its own tool. Veronica discovers your projects through it and opens its
          review TUI in your terminal; nothing else in Veronica needs it. Install it, then
          reopen this page.
        </p>
      </section>
    </div>;
  }

  return <div className="page quinjet-page">
    <div className="page-head edith-head">
      <div><h1>Quinjet</h1><div className="page-sub">Review pull requests and live workspace changes</div></div>
      <div className="head-actions">
        <button className="button" disabled={busy} onClick={() => void load()}>{busy ? "Reading…" : "Refresh"}</button>
      </div>
    </div>

    {error && <div className="banner error">{error}</div>}

    {board?.projects.length === 0 && !busy
      ? <div className="empty"><div className="empty-glyph">◈</div><h3>No recent projects</h3><p>Quinjet lists the repositories you have opened in it. Open one, then refresh.</p></div>
      : <section className="card">
        <div className="card-head"><h2>Projects</h2><span className="card-note">opens in your terminal</span></div>
        <div className="list">
          {board?.projects.map((project) => {
            const worktrees = project.worktrees.filter((worktree) => !worktree.bare && worktree.prunable === null);
            const current = worktrees.find((worktree) => worktree.current) ?? worktrees[0];
            return <div key={project.commonDir}>
              <div className="list-row">
                <button className="quinjet-name" onClick={() => setExpanded(expanded === project.commonDir ? null : project.commonDir)}>
                  <strong>{project.name}</strong>
                  <small>{current ? branchOf(current) : "no openable worktree"} · {worktrees.length} worktree{worktrees.length === 1 ? "" : "s"}</small>
                </button>
                <button className="button primary" disabled={!current} onClick={() => current && void open(current)}>Review</button>
              </div>
              {expanded === project.commonDir && <div className="quinjet-worktrees">
                {project.worktrees.map((worktree) => {
                  const openable = !worktree.bare && worktree.prunable === null;
                  return <div className="list-row" key={worktree.path}>
                    <div>
                      <strong>{branchOf(worktree)}{worktree.current ? " · on this one" : ""}</strong>
                      <small>{worktree.path}</small>
                    </div>
                    {openable
                      ? <button className="button" onClick={() => void open(worktree)}>Open</button>
                      : <span className="pill">{worktree.bare ? "Bare" : "Prunable"}</span>}
                  </div>;
                })}
              </div>}
            </div>;
          })}
        </div>
      </section>}
  </div>;
}

/** The same rule `Worktree::label` applies in the core. */
function branchOf(worktree: QuinjetWorktree) {
  if (worktree.branch) return worktree.branch;
  if (worktree.detached) return `detached at ${worktree.head.slice(0, 8)}`;
  return worktree.path;
}
