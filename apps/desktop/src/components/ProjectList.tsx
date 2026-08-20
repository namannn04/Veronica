import { useState } from "react";

import { ipc } from "../lib/ipc";
import { money, timeAgo, tokens } from "../lib/format";
import type { ProjectRollup } from "../lib/types";

/**
 * Spend by project, expanding into the chats that produced it.
 *
 * The repository name is preferred over the folder when the collector
 * identified one, so the same repository checked out twice reads as one project.
 */
export function ProjectList({ projects }: { projects: ProjectRollup[] }) {
  const [open, setOpen] = useState<string | null>(null);

  if (projects.length === 0) return <p className="card-note">No project spend in this window.</p>;

  const max = Math.max(...projects.map((p) => p.cost), 0);

  return (
    <div>
      {projects.map((project) => {
        const key = project.repositoryID ?? project.path;
        const expanded = open === key;
        const share = max > 0 ? (project.cost / max) * 100 : 0;
        return (
          <div key={key} className="ext-row" style={{ alignItems: "stretch" }}>
            <div className="ext-body">
              <button
                onClick={() => setOpen(expanded ? null : key)}
                style={{ width: "100%", textAlign: "left" }}
                aria-expanded={expanded}
              >
                <div className="ext-title">
                  <span aria-hidden="true" style={{ color: "var(--ink-muted)", fontSize: 10 }}>
                    {expanded ? "▾" : "▸"}
                  </span>
                  {project.projectName}
                  <span className="pill">
                    {project.chats.length} {project.chats.length === 1 ? "chat" : "chats"}
                  </span>
                </div>
                <div className="ext-sub truncate" title={project.repositoryID ?? project.path}>
                  {project.repositoryID ?? project.path}
                </div>
                <div style={{ height: 5, background: "var(--grid)", borderRadius: 3, marginTop: 6 }}>
                  <div
                    style={{
                      width: `${Math.max(share, 1.5)}%`,
                      height: "100%",
                      background: "var(--series-1)",
                      borderRadius: 3,
                    }}
                  />
                </div>
              </button>

              {expanded && (
                <div style={{ marginTop: 10, paddingLeft: 14 }}>
                  {project.repositoryURL && (
                    <button
                      className="button"
                      style={{ marginBottom: 8 }}
                      onClick={() => void ipc.openExternal(project.repositoryURL!)}
                    >
                      Open repository
                    </button>
                  )}
                  <table>
                    <thead>
                      <tr>
                        <th>Chat</th>
                        <th>Source</th>
                        <th className="num">Cost</th>
                        <th className="num">Tokens</th>
                        <th className="num">Last active</th>
                      </tr>
                    </thead>
                    <tbody>
                      {project.chats.map((chat) => (
                        <tr key={chat.id}>
                          <td>
                            <span className="truncate" title={chat.title || chat.id}>
                              {chat.title || chat.id}
                            </span>
                          </td>
                          <td className="mono">{chat.source}</td>
                          <td className="num">{money(chat.cost)}</td>
                          <td className="num">{tokens(chat.tokens)}</td>
                          <td className="num">
                            {chat.lastTs ? timeAgo(new Date(chat.lastTs).toISOString()) : "—"}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
            <div
              style={{
                textAlign: "right",
                fontVariantNumeric: "tabular-nums",
                minWidth: 96,
                flex: "none",
              }}
            >
              <div style={{ fontWeight: 620 }}>{money(project.cost)}</div>
              <div className="card-note">{tokens(project.tokens)}</div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
