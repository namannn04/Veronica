import {
  Button,
  ConfirmDialog,
  EmptyState,
  List,
  ListRow,
  ListSkeleton,
  Notice,
  Page,
  PageHeader,
  SearchField,
  Section,
  StatusDot,
  type Tone,
  Toolbar,
} from "@veronica/ui";
import { Container, Network, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ipc } from "../lib/ipc";
import { startVisiblePolling } from "../lib/polling";
import { endpointLabel, matchesContainer, matchesPort } from "../lib/services";
import type {
  DockerContainer,
  DockerSnapshot,
  ListeningPort,
  PortOwner,
  PortsSnapshot,
} from "../lib/types";
import "./ServicesPage.css";

const STROKE = 1.75;

type ContainerAction = "start" | "stop" | "restart";
type Change =
  | { kind: "process"; row: ListeningPort; owner: PortOwner }
  | { kind: "container"; container: DockerContainer; endpoint: string; action: ContainerAction };

const ACTION_LABEL: Record<ContainerAction, string> = {
  start: "Start",
  stop: "Stop",
  restart: "Restart",
};

function containerTone(state: string): Tone {
  if (state === "running") {
    return "good";
  }
  if (state === "paused" || state === "restarting") {
    return "warning";
  }
  if (state === "dead") {
    return "critical";
  }
  return "neutral";
}

function dockerHeadline(docker: DockerSnapshot | null, error: string | null) {
  if (docker?.status === "ready") {
    return null;
  }
  if (docker?.status === "missing") {
    return "Docker is not installed";
  }
  if (docker?.status === "remote") {
    return "Docker uses an external endpoint";
  }
  if (docker?.status === "unavailable" || error) {
    return "Docker is unavailable";
  }
  return null;
}

export function ServicesPage() {
  const [ports, setPorts] = useState<PortsSnapshot | null>(null);
  const [docker, setDocker] = useState<DockerSnapshot | null>(null);
  const [portsError, setPortsError] = useState<string | null>(null);
  const [dockerError, setDockerError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [pending, setPending] = useState<Change | null>(null);
  const [busy, setBusy] = useState(false);
  const [readingPorts, setReadingPorts] = useState(false);
  const [readingDocker, setReadingDocker] = useState(false);
  const live = useRef(true);
  const portRead = useRef(false);
  const dockerRead = useRef(false);
  const actionRunning = useRef(false);

  const loadPorts = useCallback(async () => {
    if (portRead.current) {
      return;
    }
    portRead.current = true;
    setReadingPorts(true);
    try {
      const data = await ipc.portsSnapshot();
      if (live.current) {
        setPorts(data);
        setPortsError(null);
      }
    } catch (error) {
      if (live.current) {
        setPorts(null);
        setPortsError(String(error));
      }
    } finally {
      portRead.current = false;
      if (live.current) {
        setReadingPorts(false);
      }
    }
  }, []);

  const loadDocker = useCallback(async () => {
    if (dockerRead.current) {
      return;
    }
    dockerRead.current = true;
    setReadingDocker(true);
    try {
      const data = await ipc.dockerSnapshot();
      if (live.current) {
        setDocker(data);
        setDockerError(null);
      }
    } catch (error) {
      if (live.current) {
        setDocker(null);
        setDockerError(String(error));
      }
    } finally {
      dockerRead.current = false;
      if (live.current) {
        setReadingDocker(false);
      }
    }
  }, []);

  useEffect(() => {
    live.current = true;
    const stopPorts = startVisiblePolling(async () => {
      if (!actionRunning.current) {
        await loadPorts();
      }
    }, 5000);
    const stopDocker = startVisiblePolling(async () => {
      if (!actionRunning.current) {
        await loadDocker();
      }
    }, 10000);
    return () => {
      live.current = false;
      stopPorts();
      stopDocker();
    };
  }, [loadPorts, loadDocker]);

  const refresh = () => {
    void loadPorts();
    void loadDocker();
  };

  const apply = async () => {
    if (!pending || actionRunning.current) {
      return;
    }
    const change = pending;
    actionRunning.current = true;
    setBusy(true);
    setActionError(null);
    setMessage(null);
    try {
      if (change.kind === "process") {
        const result = await ipc.portsStop({
          pid: change.owner.pid,
          startedAt: change.owner.startedAt,
          inode: change.row.inode,
        });
        if (live.current) {
          setMessage(result);
        }
      } else {
        await ipc.dockerAction(change.endpoint, change.container.id, change.action);
        if (live.current) {
          setMessage(`${change.container.name}: ${change.action} completed.`);
        }
      }
      if (live.current) {
        setPending(null);
      }
    } catch (error) {
      if (live.current) {
        setActionError(String(error));
      }
    } finally {
      actionRunning.current = false;
      if (live.current) {
        setBusy(false);
        refresh();
      }
    }
  };

  const ask = (change: Change) => {
    setActionError(null);
    setPending(change);
  };

  const portRows = ports?.ports.filter((row) => matchesPort(row, query)) ?? [];
  const containers = docker?.containers.filter((row) => matchesContainer(row, query)) ?? [];
  const running = docker?.containers.filter((row) => row.state === "running").length ?? 0;
  const dockerProblem = dockerHeadline(docker, dockerError);
  const reading = readingPorts || readingDocker;

  return (
    <Page
      className="svc-page"
      header={
        <PageHeader
          title="Ports & Docker"
          subtitle="See what is running on this computer and free a busy port"
          actions={
            <Button
              icon={<RefreshCw strokeWidth={STROKE} />}
              busy={reading && (ports !== null || docker !== null)}
              disabled={busy || reading}
              onClick={refresh}
            >
              Refresh
            </Button>
          }
        />
      }
    >
      <Toolbar
        leading={
          <SearchField
            className="svc-search"
            value={query}
            onChange={setQuery}
            placeholder="Filter by port, process or container"
          />
        }
      />

      {message && (
        <Notice
          tone="good"
          compact
          actions={
            <Button size="small" variant="plain" onClick={() => setMessage(null)}>
              Dismiss
            </Button>
          }
        >
          {message}
        </Notice>
      )}

      <Section
        title="Listening ports"
        description={
          ports
            ? `${ports.ports.length} socket${ports.ports.length === 1 ? "" : "s"} in the host network namespace · updates every 5 seconds`
            : "TCP listeners and unconnected UDP sockets, IPv4 and IPv6"
        }
      >
        {ports?.warnings.map((warning) => (
          <Notice tone="warning" compact key={warning}>
            {warning}
          </Notice>
        ))}
        {portsError ? (
          <Notice
            tone="critical"
            title="Listening ports could not be read"
            actions={
              <Button size="small" onClick={() => void loadPorts()}>
                Retry
              </Button>
            }
          >
            {portsError}
          </Notice>
        ) : !ports ? (
          <ListSkeleton rows={6} leading={0} trailing={88} label="Reading listening ports" />
        ) : portRows.length === 0 ? (
          <List label="Listening ports">
            {query ? (
              <EmptyState
                size="compact"
                title={`No ports match “${query}”`}
                action={
                  <Button size="small" onClick={() => setQuery("")}>
                    Clear filter
                  </Button>
                }
              >
                Try a port number, a process name or a PID.
              </EmptyState>
            ) : (
              <EmptyState
                size="compact"
                icon={<Network strokeWidth={STROKE} />}
                title="Nothing is listening"
              >
                No process on this computer is waiting for connections.
              </EmptyState>
            )}
          </List>
        ) : (
          <List label="Listening ports">
            {portRows.map((row) => (
              <ListRow
                key={`${row.protocol}-${row.inode}-${row.address}-${row.port}`}
                className="svc-port-row"
                leading={<span className="svc-protocol">{row.protocol}</span>}
                title={endpointLabel(row)}
                mono
                subtitle={
                  row.owners.length === 0
                    ? "Owner hidden by permissions or no longer running"
                    : row.owners
                        .map(
                          (owner) =>
                            `${owner.name} · PID ${owner.pid}${owner.canStop ? "" : " · runs as another user"}`,
                        )
                        .join(", ")
                }
                trailing={row.owners
                  .filter((owner) => owner.canStop)
                  .map((owner) => (
                    <Button
                      key={`${owner.pid}-${owner.startedAt}`}
                      size="small"
                      disabled={busy || !!portsError}
                      aria-label={`Stop ${owner.name} (PID ${owner.pid})`}
                      onClick={() => ask({ kind: "process", row, owner })}
                    >
                      {row.owners.length > 1 ? `Stop ${owner.name}` : "Stop process"}
                    </Button>
                  ))}
              />
            ))}
          </List>
        )}
      </Section>

      <Section
        title="Containers"
        description={
          docker?.status === "ready"
            ? `${docker.containers.length === 0 ? "Docker is available" : `${running} of ${docker.containers.length} running`}${docker.version ? ` · Docker ${docker.version}` : ""}`
            : "Local Docker daemon and its published ports"
        }
        actions={
          docker?.endpoint ? (
            <span className="svc-endpoint ui-mono ui-selectable" title={docker.detail}>
              {docker.endpoint}
            </span>
          ) : undefined
        }
      >
        {dockerError ? (
          <Notice
            tone="critical"
            title="Docker could not be reached"
            actions={
              <Button size="small" onClick={() => void loadDocker()}>
                Retry
              </Button>
            }
          >
            {dockerError}
          </Notice>
        ) : !docker ? (
          <ListSkeleton rows={3} leading={8} trailing={112} label="Checking Docker" />
        ) : dockerProblem ? (
          <List label="Containers">
            <EmptyState
              size="compact"
              icon={<Container strokeWidth={STROKE} />}
              title={dockerProblem}
            >
              {docker.detail}
            </EmptyState>
          </List>
        ) : containers.length === 0 ? (
          <List label="Containers">
            {query ? (
              <EmptyState
                size="compact"
                title={`No containers match “${query}”`}
                action={
                  <Button size="small" onClick={() => setQuery("")}>
                    Clear filter
                  </Button>
                }
              >
                Try a name, an image or a published port.
              </EmptyState>
            ) : (
              <EmptyState
                size="compact"
                icon={<Container strokeWidth={STROKE} />}
                title="No containers yet"
              >
                Docker is available. Containers you create appear here.
              </EmptyState>
            )}
          </List>
        ) : (
          <>
            <List label="Containers">
              {containers.map((container) => {
                const actions: ContainerAction[] =
                  container.state === "running" ? ["stop", "restart"] : ["start"];
                return (
                  <ListRow
                    key={container.id}
                    className="svc-container-row"
                    leading={
                      <StatusDot tone={containerTone(container.state)} label={container.state} />
                    }
                    title={
                      <span className="svc-container-title">
                        <span>{container.name}</span>
                        <span className="svc-state">{container.state}</span>
                      </span>
                    }
                    subtitle={
                      <span className="svc-container-meta">
                        <span>
                          {container.image} · {container.status}
                        </span>
                        <span className="ui-mono">{container.ports || "No published ports"}</span>
                      </span>
                    }
                    trailing={actions.map((action) => (
                      <Button
                        key={action}
                        size="small"
                        disabled={busy || docker.status !== "ready"}
                        onClick={() => {
                          if (docker.endpoint) {
                            ask({
                              kind: "container",
                              container,
                              endpoint: docker.endpoint,
                              action,
                            });
                          }
                        }}
                      >
                        {ACTION_LABEL[action]}
                      </Button>
                    ))}
                  />
                );
              })}
            </List>
            <p className="svc-footnote">
              Ports read <span className="ui-mono">host:port → container:port</span>. A bare port is
              exposed inside Docker, not published to the host.
            </p>
          </>
        )}
      </Section>

      <ConfirmDialog
        open={pending !== null}
        onClose={() => {
          if (!busy) {
            setPending(null);
          }
        }}
        onConfirm={() => void apply()}
        busy={busy}
        destructive={!(pending?.kind === "container" && pending.action === "start")}
        title={
          pending?.kind === "process"
            ? `Stop ${pending.owner.name} (PID ${pending.owner.pid})?`
            : pending
              ? `${ACTION_LABEL[pending.action]} ${pending.container.name}?`
              : ""
        }
        message={
          pending?.kind === "process"
            ? `This requests a graceful stop of the whole process on ${endpointLabel(pending.row)}, including any other ports it owns. A supervisor may restart it.`
            : pending?.kind === "container" && pending.action === "start"
              ? "The container starts on the local Docker endpoint shown on this page."
              : "Connections served by this container will be interrupted."
        }
        confirmLabel={
          pending?.kind === "process"
            ? "Stop process"
            : pending
              ? `${ACTION_LABEL[pending.action]} container`
              : "Confirm"
        }
      >
        {actionError && (
          <Notice tone="critical" compact>
            {actionError}
          </Notice>
        )}
      </ConfirmDialog>
    </Page>
  );
}
