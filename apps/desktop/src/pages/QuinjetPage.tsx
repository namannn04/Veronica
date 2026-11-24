import { useCallback, useEffect, useState } from "react";
import { ChevronRight, FolderGit2, GitBranch, RefreshCw } from "lucide-react";

import { ipc } from "../lib/ipc";
import type { QuinjetBoard, QuinjetProject, QuinjetWorktree } from "../lib/types";
import {
  Badge,
  Button,
  EmptyState,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
  Section,
  SkeletonRows,
} from "../ui";
import "./QuinjetPage.css";

const ICON = { strokeWidth: 1.75 };

export function QuinjetPage() {
  const [board, setBoard] = useState<QuinjetBoard | null>(null);
  const [expanded, setExpanded] = useState<Set<string> | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      setBoard(await ipc.quinjetProjects());
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const open = async (worktree: QuinjetWorktree) => {
    setError("");
    try {
      await ipc.quinjetOpen(worktree.path);
    } catch (reason) {
      setError(String(reason));
    }
  };

  const openSet = expanded ?? new Set(board?.projects[0] ? [board.projects[0].commonDir] : []);
  const toggle = (id: string) => {
    const next = new Set(openSet);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setExpanded(next);
  };

  const installed = board?.installed !== false;

  return (
    <Page
      header={
        <PageHeader
          title="Quinjet"
          subtitle="Review pull requests and live workspace changes"
          actions={
            installed && (
              <Button icon={<RefreshCw {...ICON} />} busy={busy} onClick={() => void load()}>
                {busy ? "Reading…" : "Refresh"}
              </Button>
            )
          }
        />
      }
    >
      {error && (
        <Notice
          tone="critical"
          title="Quinjet did not answer"
          actions={
            <Button size="small" onClick={() => void load()}>
              Retry
            </Button>
          }
        >
          {error}
        </Notice>
      )}

      {board && !board.installed && (
        <Notice
          tone="neutral"
          title="Quinjet is not installed"
          actions={
            <Button size="small" busy={busy} onClick={() => void load()}>
              Check again
            </Button>
          }
        >
          Quinjet is its own tool. Veronica finds your projects through it and opens its review view
          in your terminal; nothing else in Veronica needs it.
        </Notice>
      )}

      {!board && !error && (
        <Section title="Projects">
          <List label="Reading Quinjet projects">
            <SkeletonRows rows={4} label="Reading Quinjet projects" />
          </List>
        </Section>
      )}

      {board?.installed && board.projects.length === 0 && !busy && (
        <div className="quinjet-empty">
          <EmptyState
            icon={<FolderGit2 {...ICON} />}
            title="No recent projects"
            action={<Button onClick={() => void load()}>Refresh</Button>}
          >
            Quinjet lists the repositories you have opened in it. Open one there, then refresh.
          </EmptyState>
        </div>
      )}

      {board?.installed && board.projects.length > 0 && (
        <Section title="Projects" description="Review and Open launch Quinjet in your terminal">
          <List label="Quinjet projects" className="quinjet-list">
            {board.projects.map((project) => (
              <ProjectGroup
                key={project.commonDir}
                project={project}
                expanded={openSet.has(project.commonDir)}
                onToggle={() => toggle(project.commonDir)}
                onOpen={(worktree) => void open(worktree)}
              />
            ))}
          </List>
        </Section>
      )}
    </Page>
  );
}

function ProjectGroup({
  project,
  expanded,
  onToggle,
  onOpen,
}: {
  project: QuinjetProject;
  expanded: boolean;
  onToggle: () => void;
  onOpen: (worktree: QuinjetWorktree) => void;
}) {
  const worktrees = project.worktrees.filter(
    (worktree) => !worktree.bare && worktree.prunable === null,
  );
  const current = worktrees.find((worktree) => worktree.current) ?? worktrees[0];
  return (
    <>
      <ListRow
        className="quinjet-project"
        leading={
          <ChevronRight
            {...ICON}
            className="quinjet-chevron"
            data-open={expanded || undefined}
            aria-hidden="true"
          />
        }
        title={project.name}
        subtitle={
          <>
            {current ? (
              <span className="ui-mono">{branchOf(current)}</span>
            ) : (
              "No openable worktree"
            )}
            {` · ${worktrees.length} worktree${worktrees.length === 1 ? "" : "s"}`}
          </>
        }
        onClick={onToggle}
        trailing={
          <Button size="small" disabled={!current} onClick={() => current && onOpen(current)}>
            Review
          </Button>
        }
      />
      {expanded &&
        project.worktrees.map((worktree) => {
          const openable = !worktree.bare && worktree.prunable === null;
          return (
            <ListRow
              key={worktree.path}
              className="quinjet-worktree"
              leading={<GitBranch {...ICON} aria-hidden="true" />}
              title={branchOf(worktree)}
              mono
              subtitle={<span className="ui-mono">{worktree.path}</span>}
              meta={worktree.current ? <Badge tone="accent">Current</Badge> : undefined}
              trailing={
                openable ? (
                  <Button size="small" variant="plain" onClick={() => onOpen(worktree)}>
                    Open
                  </Button>
                ) : (
                  <Badge tone="warning">{worktree.bare ? "Bare" : "Prunable"}</Badge>
                )
              }
            />
          );
        })}
    </>
  );
}

function branchOf(worktree: QuinjetWorktree) {
  if (worktree.branch) return worktree.branch;
  if (worktree.detached) return `detached at ${worktree.head.slice(0, 8)}`;
  return worktree.path;
}
