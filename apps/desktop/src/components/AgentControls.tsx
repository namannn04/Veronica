import { Pencil, Play, Plus, Trash2, X } from "lucide-react";
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import type { AgentProfile, AgentView } from "../lib/agents";
import { ipc } from "../lib/ipc";
import { startVisiblePolling } from "../lib/polling";
import {
  Badge,
  Button,
  ButtonGroup,
  Card,
  Checkbox,
  ConfirmDialog,
  Dialog,
  EmptyState,
  Field,
  FormGroup,
  FormRow,
  IconButton,
  List,
  ListRow,
  Notice,
  Section,
  Select,
  Spinner,
  StatusDot,
  TextField,
} from "../ui";
import "./AgentControls.css";

const AgentTerminal = lazy(() =>
  import("./AgentTerminal").then((m) => ({ default: m.AgentTerminal })),
);

const ICON = { strokeWidth: 1.75 };
const MAX_OWNED = 4;

type Confirmation = {
  title: string;
  message: string;
  confirmLabel: string;
  destructive?: boolean;
  run: () => void;
};

export function AgentControls({ requestedId }: { requestedId?: string }) {
  const [profiles, setProfiles] = useState<AgentProfile[]>([]);
  const [provider, setProvider] = useState("");
  const [cwd, setCwd] = useState("");
  const [owned, setOwned] = useState<AgentView[]>([]);
  const [external, setExternal] = useState<AgentView[]>([]);
  const [selected, setSelected] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [editingId, setEditingId] = useState("");
  const [notice, setNotice] = useState("");
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");
  const [exe, setExe] = useState("");
  const [args, setArgs] = useState("[]");
  const [formError, setFormError] = useState("");
  const [resume, setResume] = useState(false);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const live = useRef(true);
  const running = useRef(false);
  const terminalAnchor = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    const [a, b] = await Promise.all([ipc.agentList(), ipc.agentExternal()]);
    if (live.current) {
      setOwned(a);
      const ids = new Set(a.map((v) => v.pid));
      setExternal(b.filter((v) => !ids.has(v.pid)));
    }
  }, []);

  useEffect(() => {
    live.current = true;
    void ipc
      .agentProfiles()
      .then((p) => {
        if (live.current) {
          setProfiles(p);
          setProvider(p[0]?.id || "");
        }
      })
      .catch((e) => setError(String(e)));
    return () => {
      live.current = false;
    };
  }, []);

  useEffect(() => startVisiblePolling(refresh, 5000), [refresh]);

  useEffect(() => {
    if (requestedId) {
      setSelected(requestedId);
      void refresh()
        .then(() => terminalAnchor.current?.scrollIntoView({ block: "nearest" }))
        .catch((e) => setError(String(e)));
    }
  }, [requestedId, refresh]);

  const launch = async (profile: AgentProfile) => {
    if (running.current) {
      return;
    }
    running.current = true;
    setBusy(true);
    setError("");
    try {
      const resumeArguments =
        resume && profile.id === "claude"
          ? ["--resume"]
          : resume && profile.id === "codex"
            ? ["resume"]
            : [];
      const view = await ipc.agentStart(
        { ...profile, arguments: [...profile.arguments, ...resumeArguments] },
        cwd.trim(),
      );
      if (live.current) {
        setSelected(view.id);
        await refresh();
      }
    } catch (e) {
      if (live.current) {
        setError(String(e));
      }
    } finally {
      running.current = false;
      if (live.current) {
        setBusy(false);
      }
    }
  };

  const start = () => {
    if (running.current) {
      return;
    }
    const profile = profiles.find((p) => p.id === provider);
    if (!profile) {
      return;
    }
    if (profile.id === "codex-shared") {
      setConfirmation({
        title: "Open the Codex shared agent center?",
        message:
          "Its backend owns the tasks, which can continue after this window closes. Veronica only owns this terminal frontend.",
        confirmLabel: "Open agent center",
        run: () => void launch(profile),
      });
      return;
    }
    void launch(profile);
  };

  const save = async () => {
    setFormError("");
    try {
      const argumentsValue: unknown = JSON.parse(args);
      if (!Array.isArray(argumentsValue) || argumentsValue.some((v) => typeof v !== "string")) {
        throw new Error("Arguments must be a JSON array of strings.");
      }
      const profile = {
        id: editingId || crypto.randomUUID(),
        name: name.trim(),
        executable: exe.trim(),
        arguments: argumentsValue as string[],
      };
      if (!profile.name || !profile.executable) {
        throw new Error("Enter a provider name and executable.");
      }
      const next = editingId
        ? profiles.map((p) => (p.id === editingId ? profile : p))
        : [...profiles, profile];
      await ipc.agentProfilesSave(next);
      setProfiles(next);
      setProvider(profile.id);
      setEditing(false);
    } catch (e) {
      setFormError(String(e));
    }
  };

  const stop = async (id: string) => {
    try {
      await ipc.agentStop(id);
      if (selected === id) {
        setSelected("");
      }
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const addProvider = () => {
    setEditingId("");
    setName("");
    setExe("");
    setArgs("[]");
    setFormError("");
    setEditing(true);
  };

  const editProvider = () => {
    const p = profiles.find((item) => item.id === provider);
    if (!p) {
      return;
    }
    setEditingId(p.id);
    setName(p.name);
    setExe(p.executable);
    setArgs(JSON.stringify(p.arguments));
    setFormError("");
    setEditing(true);
  };

  const removeProvider = () =>
    setConfirmation({
      title: "Remove this provider profile?",
      message: "Running terminals that use it keep running.",
      confirmLabel: "Remove provider",
      destructive: true,
      run: () => {
        const next = profiles.filter((p) => p.id !== provider);
        void ipc
          .agentProfilesSave(next)
          .then(() => {
            setProfiles(next);
            setProvider(next[0]?.id || "");
            setEditing(false);
            setEditingId("");
          })
          .catch((e) => setError(String(e)));
      },
    });

  const stopAll = () =>
    setConfirmation({
      title: "Stop all terminals owned by Veronica?",
      message: "External backends keep running.",
      confirmLabel: "Stop all",
      destructive: true,
      run: () =>
        void Promise.all(owned.map((a) => ipc.agentStop(a.id)))
          .then(refresh)
          .catch((e) => setError(String(e))),
    });

  const connectTracking = () =>
    setConfirmation({
      title: "Connect Claude status hooks?",
      message:
        "Veronica adds status-only hooks to ~/.claude/settings.json. Existing hooks are kept. New Claude Code sessions use them.",
      confirmLabel: "Connect",
      run: () =>
        void ipc
          .agentTracking(true, true)
          .then(() =>
            setNotice(
              "Claude tracking connected. New sessions report activity; existing sessions may need a restart.",
            ),
          )
          .catch((e) => setError(String(e))),
    });

  const disconnectTracking = () =>
    setConfirmation({
      title: "Disconnect Claude status hooks?",
      message: "Only the hooks Veronica added are removed from your Claude settings.",
      confirmLabel: "Disconnect",
      destructive: true,
      run: () => void ipc.agentTracking(false, true).catch((e) => setError(String(e))),
    });

  const atLimit = owned.length >= MAX_OWNED;
  const resumable = ["claude", "codex"].includes(provider);
  const selectedOwned = selected && owned.some((a) => a.id === selected) ? selected : "";

  return (
    <>
      <Section
        title="Agent terminals"
        description="Run Claude Code, Codex or another installed CLI inside Veronica. Permissions and login prompts stay in its terminal."
      >
        {error && (
          <Notice
            tone="critical"
            actions={
              <IconButton
                size="small"
                label="Dismiss"
                icon={<X {...ICON} />}
                onClick={() => setError("")}
              />
            }
          >
            {error}
          </Notice>
        )}
        {notice && (
          <Notice
            tone="good"
            actions={
              <IconButton
                size="small"
                label="Dismiss"
                icon={<X {...ICON} />}
                onClick={() => setNotice("")}
              />
            }
          >
            {notice}
          </Notice>
        )}

        <Card className="agent-launch">
          <div className="agent-launch-fields">
            <Field label="Provider">
              {(id) => (
                <div className="agent-provider">
                  <Select
                    id={id}
                    value={provider}
                    onChange={(e) => setProvider(e.target.value)}
                    disabled={busy}
                  >
                    {profiles.map((p) => (
                      <option value={p.id} key={p.id}>
                        {p.name}
                      </option>
                    ))}
                  </Select>
                  <ButtonGroup>
                    <IconButton
                      variant="secondary"
                      label="Edit provider"
                      tooltip="Edit the selected provider"
                      icon={<Pencil {...ICON} />}
                      disabled={!provider}
                      onClick={editProvider}
                    />
                    <IconButton
                      variant="secondary"
                      label="Add provider"
                      tooltip="Add a provider profile"
                      icon={<Plus {...ICON} />}
                      onClick={addProvider}
                    />
                    <IconButton
                      variant="secondary"
                      label="Remove provider"
                      tooltip="Remove the selected provider"
                      icon={<Trash2 {...ICON} />}
                      disabled={profiles.length <= 1}
                      onClick={removeProvider}
                    />
                  </ButtonGroup>
                </div>
              )}
            </Field>
            <Field
              label="Project directory"
              hint={
                atLimit
                  ? "Close a terminal to start another. Up to four can run at once."
                  : undefined
              }
            >
              {(id) => (
                <TextField
                  id={id}
                  mono
                  value={cwd}
                  onChange={(e) => setCwd(e.target.value)}
                  placeholder="/home/you/projects/my-app"
                  disabled={busy}
                />
              )}
            </Field>
          </div>
          <div className="agent-launch-footer">
            <Checkbox
              label="Open the saved-session picker"
              description="Claude Code and Codex only"
              disabled={!resumable}
              checked={resume}
              onChange={setResume}
            />
            <Button
              variant="primary"
              icon={<Play {...ICON} />}
              busy={busy}
              disabled={!cwd.trim() || !provider || atLimit}
              onClick={start}
            >
              {busy ? "Starting…" : "Start agent"}
            </Button>
          </div>
        </Card>

        {owned.length > 0 && (
          <div className="agent-owned" ref={terminalAnchor}>
            <div className="agent-owned-head">
              <span className="agent-owned-title">Running here</span>
              <Badge>{`${owned.length} of ${MAX_OWNED}`}</Badge>
              <span className="agent-owned-spacer" />
              <Button size="small" variant="destructive-plain" onClick={stopAll}>
                Stop all
              </Button>
            </div>
            <List label="Terminals owned by Veronica">
              {owned.map((a) => (
                <ListRow
                  key={a.id}
                  leading={
                    <StatusDot
                      tone={a.status === "running" ? "good" : "neutral"}
                      label={a.status}
                    />
                  }
                  title={a.name}
                  subtitle={<span className="ui-mono">{a.cwd || "Directory unavailable"}</span>}
                  meta={<span className="ui-num">PID {a.pid}</span>}
                  selected={selected === a.id}
                  onClick={() => setSelected(a.id)}
                  trailing={
                    <Button
                      size="small"
                      variant="destructive-plain"
                      onClick={() => void stop(a.id)}
                    >
                      Close terminal
                    </Button>
                  }
                />
              ))}
            </List>
          </div>
        )}

        {selectedOwned && (
          <Suspense
            fallback={
              <Card className="agent-terminal-loading">
                <Spinner label="Opening terminal" />
              </Card>
            }
          >
            <AgentTerminal
              key={selectedOwned}
              id={selectedOwned}
              title={owned.find((a) => a.id === selectedOwned)?.name ?? "Terminal"}
            />
          </Suspense>
        )}

        <p className="agent-footnote">
          Up to four owned terminals. Closing the main window stops them; external agents are never
          stopped by Veronica. Managed host terminals need the Debian or AppImage installation.
        </p>
      </Section>

      <FormGroup
        title="Status tracking"
        description="Working and blocked states come from Herdr or from explicit Claude status hooks, never from guessing at process activity."
      >
        <FormRow
          label="Claude status hooks"
          description="Status-only hooks in ~/.claude/settings.json. Hook states are the last reported event."
          control={
            <>
              <Button size="small" variant="plain" onClick={disconnectTracking}>
                Disconnect
              </Button>
              <Button size="small" onClick={connectTracking}>
                Connect
              </Button>
            </>
          }
        />
      </FormGroup>

      <Section
        title="Detected external agents"
        description="Your own processes, read-only. A running process does not prove an active turn. Desktop apps and existing terminal sessions keep their own lifecycle."
      >
        {external.length ? (
          <List label="External agents">
            {external.map((a) => (
              <ListRow
                key={a.id}
                leading={
                  <StatusDot
                    tone={
                      a.status === "working"
                        ? "accent"
                        : a.status === "blocked"
                          ? "warning"
                          : "neutral"
                    }
                    label={a.status}
                  />
                }
                title={a.name}
                subtitle={<span className="ui-mono">{a.cwd || "Directory unavailable"}</span>}
                meta={
                  <span className="agent-external-meta">
                    <span className="ui-num">PID {a.pid}</span>
                    <Badge>{a.status}</Badge>
                  </span>
                }
              />
            ))}
          </List>
        ) : (
          <div className="agent-external-empty">
            <EmptyState size="compact" title="No external agents">
              No supported agent process is running outside Veronica.
            </EmptyState>
          </div>
        )}
      </Section>

      <Dialog
        open={editing}
        onClose={() => setEditing(false)}
        title={editingId ? "Edit provider" : "Add provider"}
        description="Profiles are stored on this device. Use the CLI's own login flow; never put API keys in saved arguments."
        footer={
          <>
            <Button onClick={() => setEditing(false)}>Cancel</Button>
            <Button variant="primary" onClick={() => void save()}>
              Save provider
            </Button>
          </>
        }
      >
        <div className="agent-profile-form">
          {formError && (
            <Notice tone="critical" compact>
              {formError}
            </Notice>
          )}
          <Field label="Name">
            {(id) => (
              <TextField
                id={id}
                value={name}
                onChange={(e) => setName(e.target.value)}
                maxLength={80}
                data-autofocus
              />
            )}
          </Field>
          <Field label="Executable path or command">
            {(id) => (
              <TextField
                id={id}
                mono
                value={exe}
                onChange={(e) => setExe(e.target.value)}
                placeholder="/path/to/agent"
              />
            )}
          </Field>
          <Field
            label="Arguments"
            hint="A JSON array of strings. To resume a CLI chat, add its documented resume arguments to a new profile."
          >
            {(id) => (
              <TextField
                id={id}
                mono
                value={args}
                onChange={(e) => setArgs(e.target.value)}
                maxLength={16000}
              />
            )}
          </Field>
        </div>
      </Dialog>

      <ConfirmDialog
        open={confirmation !== null}
        onClose={() => setConfirmation(null)}
        onConfirm={() => {
          const current = confirmation;
          setConfirmation(null);
          current?.run();
        }}
        title={confirmation?.title ?? ""}
        message={confirmation?.message}
        confirmLabel={confirmation?.confirmLabel ?? "Continue"}
        destructive={confirmation?.destructive}
      />
    </>
  );
}
