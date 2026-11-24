import { Copy, ExternalLink, MonitorPlay, RefreshCw, SquareTerminal } from "lucide-react";
import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react";

import { ipc } from "../lib/ipc";
import { startVisiblePolling } from "../lib/polling";
import type { HerdrAgent, HerdrBoard, HerdrSession } from "../lib/types";
import {
  Badge,
  Button,
  ConfirmDialog,
  Dialog,
  EmptyState,
  FilterChips,
  IconButton,
  KeyValueList,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
  Section,
  Select,
  Skeleton,
  Spinner,
  StatusDot,
  type Tone,
  Toolbar,
} from "../ui";
import "./HerdrPage.css";

const AgentControls = lazy(() =>
  import("../components/AgentControls").then((m) => ({ default: m.AgentControls })),
);

const STATUSES: HerdrAgent["status"][] = ["blocked", "working", "unknown", "done", "idle"];
const STATUS_LABEL: Record<HerdrAgent["status"], string> = {
  blocked: "Blocked",
  working: "Working",
  unknown: "Unknown",
  done: "Done",
  idle: "Idle",
};
const STATUS_TONE: Record<HerdrAgent["status"], Tone> = {
  blocked: "warning",
  working: "accent",
  unknown: "neutral",
  done: "good",
  idle: "neutral",
};
const ICON = { strokeWidth: 1.75 };

export function HerdrPage() {
  const [terminalFocus, setTerminalFocus] = useState<string>();
  const [board, setBoard] = useState<HerdrBoard | null>(null);
  const [error, setError] = useState("");
  const [session, setSession] = useState("all");
  const [kinds, setKinds] = useState<string[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [attachTarget, setAttachTarget] = useState<HerdrAgent | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  const load = useCallback(async () => {
    try {
      setBoard(await ipc.herdrBoard());
      setError("");
    } catch (reason) {
      setError(String(reason));
    }
  }, []);

  useEffect(() => startVisiblePolling(load, 2000), [load]);

  const refresh = async () => {
    setRefreshing(true);
    await load();
    setRefreshing(false);
  };

  const kindChoices = useMemo(
    () => [...new Set(board?.agents.map((agent) => agent.kind) ?? [])].sort(),
    [board],
  );
  const agents = useMemo(
    () =>
      (board?.agents ?? []).filter(
        (agent) =>
          (session === "all" || agent.session === session) &&
          (kinds.length === 0 || kinds.includes(agent.kind)),
      ),
    [board, kinds, session],
  );
  const selectedAgent = board?.agents.find((agent) => agent.id === selected) ?? null;

  const open = (name: string, pane?: string) => {
    void ipc.herdrOpen(name, pane).catch((reason) => setError(String(reason)));
  };

  const toggleKind = (kind: string) =>
    setKinds((current) =>
      current.includes(kind) ? current.filter((item) => item !== kind) : [...current, kind],
    );

  const copyAttach = async (agent: HerdrAgent) => {
    try {
      await navigator.clipboard.writeText(agent.attachCommand);
      setCopied(agent.id);
      window.setTimeout(() => setCopied((id) => (id === agent.id ? null : id)), 1_200);
    } catch (reason) {
      setError(`Cannot copy the attach command: ${reason}`);
    }
  };

  const attachInApp = (agent: HerdrAgent) => {
    setAttachTarget(null);
    setSelected(null);
    void ipc
      .agentHerdrAttach(agent.session, agent.paneId, true)
      .then((view) => setTerminalFocus(view.id))
      .catch((reason) => setError(String(reason)));
  };

  const firstSession = board?.sessions[0];

  return (
    <Page
      header={
        <PageHeader
          title="Agents & Herdr"
          subtitle="Live coding-agent sessions across persistent terminal workspaces"
          actions={
            <>
              <Button
                icon={<RefreshCw {...ICON} />}
                busy={refreshing}
                onClick={() => void refresh()}
              >
                Refresh
              </Button>
              {firstSession && (
                <Button icon={<ExternalLink {...ICON} />} onClick={() => open(firstSession.name)}>
                  Open Herdr
                </Button>
              )}
            </>
          }
        />
      }
    >
      {error && (
        <Notice
          tone="critical"
          title="Herdr did not answer"
          actions={
            <Button size="small" onClick={() => void refresh()}>
              Retry
            </Button>
          }
        >
          {error}
        </Notice>
      )}

      {!board && !error && <BoardSkeleton />}

      {board && !board.installed && (
        <Notice
          tone="neutral"
          title="Herdr is not installed"
          actions={
            <Button size="small" onClick={() => void refresh()}>
              Check again
            </Button>
          }
        >
          The agent terminals below work on their own. Install Herdr if you also want its persistent
          session board.
        </Notice>
      )}

      {!error && board?.installed && board.error && (
        <Notice tone="warning" title="Herdr answered with a problem">
          {board.error}
        </Notice>
      )}

      {board?.installed && (
        <Section title="Board" description="Agents grouped by what they are doing right now">
          {board.sessions.length === 0 ? (
            <div className="herdr-empty">
              <EmptyState
                icon={<SquareTerminal {...ICON} />}
                title="No Herdr sessions yet"
                action={<Button onClick={() => void refresh()}>Check again</Button>}
              >
                Launch Herdr once and the board starts following its agents.
              </EmptyState>
            </div>
          ) : (
            <>
              <Toolbar
                className="herdr-toolbar"
                leading={
                  <>
                    <Select
                      variant="chip"
                      size="small"
                      value={session}
                      onChange={(event) => setSession(event.target.value)}
                      aria-label="Session"
                    >
                      <option value="all">All sessions</option>
                      {board.sessions.map((item) => (
                        <option value={item.name} key={item.name}>
                          {item.name}
                          {item.running ? " (running)" : " (stopped)"}
                        </option>
                      ))}
                    </Select>
                    {kindChoices.length > 1 && (
                      <FilterChips
                        label="Agent kind"
                        chips={kindChoices.map((kind) => ({ value: kind, label: kind }))}
                        selected={kinds}
                        onToggle={toggleKind}
                        onClear={() => setKinds([])}
                      />
                    )}
                  </>
                }
                trailing={
                  <span className="herdr-live ui-num">
                    <StatusDot tone={agents.length ? "good" : "neutral"} />
                    {agents.length} live
                  </span>
                }
              />
              {board.agents.length === 0 ? (
                <div className="herdr-empty">
                  <EmptyState
                    icon={<MonitorPlay {...ICON} />}
                    title="No agents detected yet"
                    size="compact"
                  >
                    Agents show up here as soon as a Herdr session reports one.
                  </EmptyState>
                </div>
              ) : agents.length === 0 ? (
                <div className="herdr-empty">
                  <EmptyState
                    title="No agents match these filters"
                    size="compact"
                    action={
                      <Button
                        size="small"
                        onClick={() => {
                          setKinds([]);
                          setSession("all");
                        }}
                      >
                        Clear filters
                      </Button>
                    }
                  />
                </div>
              ) : (
                <div className="herdr-lanes">
                  {STATUSES.map((status) => (
                    <Lane
                      key={status}
                      status={status}
                      agents={agents.filter((agent) => agent.status === status)}
                      copied={copied}
                      onSelect={setSelected}
                      onOpen={(agent) => open(agent.session, agent.paneId)}
                      onCopy={(agent) => void copyAttach(agent)}
                    />
                  ))}
                </div>
              )}
            </>
          )}
        </Section>
      )}

      {board?.installed && board.sessions.length > 0 && (
        <Section
          title="Sessions"
          description="Each session keeps its terminals alive between launches"
        >
          <List label="Herdr sessions">
            {board.sessions.map((item) => (
              <SessionRow
                key={item.name}
                session={item}
                agentCount={board.agents.filter((agent) => agent.session === item.name).length}
                onOpen={() => open(item.name)}
              />
            ))}
          </List>
        </Section>
      )}

      <Suspense
        fallback={
          <div className="herdr-loading">
            <Spinner label="Opening agent terminals" />
          </div>
        }
      >
        <AgentControls requestedId={terminalFocus} />
      </Suspense>

      <Dialog
        open={selectedAgent !== null}
        onClose={() => setSelected(null)}
        title={selectedAgent?.title ?? ""}
        description={
          selectedAgent ? `${selectedAgent.kind} in ${selectedAgent.session}` : undefined
        }
        width={520}
        footer={
          selectedAgent && (
            <>
              <Button icon={<Copy {...ICON} />} onClick={() => void copyAttach(selectedAgent)}>
                {copied === selectedAgent.id ? "Copied" : "Copy command"}
              </Button>
              <Button onClick={() => setAttachTarget(selectedAgent)}>View in app</Button>
              <Button
                variant="primary"
                onClick={() => open(selectedAgent.session, selectedAgent.paneId)}
              >
                Open terminal
              </Button>
            </>
          )
        }
      >
        {selectedAgent && (
          <div className="herdr-detail">
            <KeyValueList
              items={[
                {
                  label: "Status",
                  value: (
                    <Badge tone={STATUS_TONE[selectedAgent.status]} dot>
                      {STATUS_LABEL[selectedAgent.status]}
                    </Badge>
                  ),
                },
                { label: "Session", value: selectedAgent.session, mono: true },
                { label: "Workspace", value: selectedAgent.workspace || "Not reported" },
                { label: "Directory", value: selectedAgent.cwd || "Not reported", mono: true },
                { label: "Pane", value: selectedAgent.paneId, mono: true },
              ]}
            />
            <div className="herdr-attach">
              <span className="herdr-attach-label">Attach from any terminal</span>
              <code className="herdr-attach-command ui-mono ui-selectable">
                {selectedAgent.attachCommand}
              </code>
            </div>
          </div>
        )}
      </Dialog>

      <ConfirmDialog
        open={attachTarget !== null}
        onClose={() => setAttachTarget(null)}
        onConfirm={() => attachTarget && attachInApp(attachTarget)}
        title="Attach this pane inside Veronica?"
        message="Its existing frontend may be disconnected. The Herdr backend and the agent stay owned by Herdr."
        confirmLabel="Attach pane"
      />
    </Page>
  );
}

function Lane({
  status,
  agents,
  copied,
  onSelect,
  onOpen,
  onCopy,
}: {
  status: HerdrAgent["status"];
  agents: HerdrAgent[];
  copied: string | null;
  onSelect: (id: string) => void;
  onOpen: (agent: HerdrAgent) => void;
  onCopy: (agent: HerdrAgent) => void;
}) {
  return (
    <section
      className="herdr-lane"
      data-empty={agents.length === 0 || undefined}
      aria-label={`${STATUS_LABEL[status]} agents`}
    >
      <header className="herdr-lane-head">
        <StatusDot tone={STATUS_TONE[status]} />
        <h3>{STATUS_LABEL[status]}</h3>
        <span className="herdr-lane-count ui-num">{agents.length}</span>
      </header>
      <div className="herdr-lane-body">
        {agents.map((agent) => (
          <article className="herdr-card" key={agent.id}>
            <button type="button" className="herdr-card-main" onClick={() => onSelect(agent.id)}>
              <span className="herdr-card-top">
                <span className="herdr-card-kind">{agent.kind}</span>
                {agent.focused && <Badge tone="accent">Focused</Badge>}
              </span>
              <span className="herdr-card-title">{agent.title}</span>
              <span className="herdr-card-path ui-mono">
                {agent.cwd || agent.workspace || agent.session}
              </span>
            </button>
            <footer className="herdr-card-actions">
              <Badge tone={STATUS_TONE[status]}>{STATUS_LABEL[status]}</Badge>
              <span className="herdr-card-spacer" />
              <IconButton
                size="small"
                label={copied === agent.id ? "Copied" : "Copy attach command"}
                icon={<Copy {...ICON} />}
                onClick={() => onCopy(agent)}
              />
              <Button size="small" onClick={() => onOpen(agent)}>
                Open
              </Button>
            </footer>
          </article>
        ))}
        {agents.length === 0 && (
          <p className="herdr-lane-empty">No {STATUS_LABEL[status].toLowerCase()} agents</p>
        )}
      </div>
    </section>
  );
}

function SessionRow({
  session,
  agentCount,
  onOpen,
}: {
  session: HerdrSession;
  agentCount: number;
  onOpen: () => void;
}) {
  const detail =
    session.error ||
    (session.running
      ? agentCount
        ? `${agentCount} agent${agentCount === 1 ? "" : "s"}`
        : "Running, no agents detected yet"
      : "Stopped");
  return (
    <ListRow
      leading={
        <StatusDot
          tone={session.error ? "critical" : session.running ? "good" : "neutral"}
          label={session.running ? "Running" : "Stopped"}
        />
      }
      title={session.name}
      mono
      subtitle={detail}
      meta={session.default ? <Badge>Default</Badge> : undefined}
      trailing={
        <Button size="small" onClick={onOpen}>
          {session.running ? "Attach" : "Start"}
        </Button>
      }
    />
  );
}

function BoardSkeleton() {
  return (
    <div className="herdr-lanes" role="status" aria-label="Reading Herdr sessions">
      {STATUSES.map((status) => (
        <div className="herdr-lane" key={status}>
          <div className="herdr-lane-head">
            <Skeleton width={64} height={11} />
          </div>
          <div className="herdr-lane-body">
            <Skeleton height={84} radius={10} />
            <Skeleton height={84} radius={10} />
          </div>
        </div>
      ))}
    </div>
  );
}
