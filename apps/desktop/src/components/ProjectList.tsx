import { ChevronDown, ChevronRight, ExternalLink } from "lucide-react";
import { Fragment, useState } from "react";
import { money, timeAgo, tokens } from "../lib/format";
import { ipc } from "../lib/ipc";
import type { ProjectRollup } from "../lib/types";
import { Badge, Button, List, ListRow, Table } from "../ui";
import "./ProjectList.css";

export function ProjectList({ projects }: { projects: ProjectRollup[] }) {
  const [open, setOpen] = useState<string | null>(null);

  if (projects.length === 0) {
    return <p className="projects-empty">No project spend in this window.</p>;
  }

  const max = Math.max(...projects.map((p) => p.cost), 0);

  return (
    <List inset={false} label="Projects" className="projects-list">
      {projects.map((project) => {
        const key = project.repositoryId ?? project.path;
        const expanded = open === key;
        const share = max > 0 ? (project.cost / max) * 100 : 0;
        const repositoryUrl = project.repositoryUrl;
        return (
          <Fragment key={key}>
            <ListRow
              className="projects-row"
              selected={expanded}
              onClick={() => setOpen(expanded ? null : key)}
              leading={expanded ? <ChevronDown /> : <ChevronRight />}
              title={
                <span className="projects-title">
                  <span className="projects-name">{project.projectName}</span>
                  <Badge>
                    {project.chats.length} {project.chats.length === 1 ? "chat" : "chats"}
                  </Badge>
                </span>
              }
              subtitle={<span className="ui-mono">{project.repositoryId ?? project.path}</span>}
              meta={
                <span className="projects-figures">
                  <span className="projects-share" aria-hidden="true">
                    <span style={{ width: `${Math.max(share, 1.5)}%` }} />
                  </span>
                  <span className="projects-amounts">
                    <span className="projects-cost ui-num">{money(project.cost)}</span>
                    <span className="projects-tokens ui-num">{tokens(project.tokens)}</span>
                  </span>
                </span>
              }
            />
            {expanded && (
              <li className="projects-detail">
                {repositoryUrl && (
                  <div>
                    <Button
                      size="small"
                      icon={<ExternalLink />}
                      onClick={() => void ipc.openExternal(repositoryUrl)}
                    >
                      Open repository
                    </Button>
                  </div>
                )}
                <Table label={`Chats in ${project.projectName}`}>
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
                          <span className="projects-chat" title={chat.title || chat.id}>
                            {chat.title || chat.id}
                          </span>
                        </td>
                        <td className="ui-mono">{chat.source}</td>
                        <td className="num">{money(chat.cost)}</td>
                        <td className="num">{tokens(chat.tokens)}</td>
                        <td className="num muted">
                          {chat.lastTs ? timeAgo(new Date(chat.lastTs).toISOString()) : "Never"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </Table>
              </li>
            )}
          </Fragment>
        );
      })}
    </List>
  );
}
