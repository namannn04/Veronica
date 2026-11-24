import { useEffect, useRef, useState } from "react";
import { AlertTriangle, CheckCircle2, Stethoscope } from "lucide-react";

import { GraftPlayground } from "../components/GraftPlayground";
import { ipc } from "../lib/ipc";
import type { DoctorReport } from "../lib/workspace";
import { Badge, Button, Card, CardHeader, EmptyState, Field, List, ListRow, Notice, Page, PageHeader, SkeletonRows, Tabs, TextField } from "../ui";
import "./WorkspacePage.css";

type Tool = "doctor" | "graft";

const ICON = { strokeWidth: 1.75 };
const TOOLS = [
  { id: "doctor", label: "Project Doctor" },
  { id: "graft", label: "Graftcode Playground" },
] as const;
const SUBTITLES: Record<Tool, string> = {
  doctor: "Read-only checks for one project, only when you ask",
  graft: "Browse and call the methods a local Graftcode Gateway exposes",
};

export function WorkspacePage() {
  const [tab, setTab] = useState<Tool>("doctor");
  const [root, setRoot] = useState("");
  const [ports, setPorts] = useState("");
  const [containers, setContainers] = useState("");
  const [report, setReport] = useState<DoctorReport | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  const run = async () => {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError("");
    setReport(null);
    try {
      const selected = ports.trim()
        ? ports.split(",").map((p) => {
            if (!/^\s*\d+\s*$/.test(p)) throw new Error("Use comma-separated port numbers.");
            return Number(p);
          })
        : [];
      const result = await ipc.workspaceDoctor({ root: root.trim(), ports: selected, containers: containers.trim() ? containers.split(",").map((s) => s.trim()) : [] });
      if (live.current) setReport(result);
    } catch (e) {
      if (live.current) setError(String(e));
    } finally {
      running.current = false;
      if (live.current) setBusy(false);
    }
  };

  const passed = report?.checks.filter((c) => c.status === "pass").length ?? 0;
  const flagged = (report?.checks.length ?? 0) - passed;

  return (
    <Page
      fill
      className="ws-page"
      header={
        <PageHeader
          title="Developer Workspace"
          subtitle={SUBTITLES[tab]}
          accessory={<Tabs items={TOOLS} value={tab} onChange={(next) => !busy && setTab(next)} label="Workspace tools" />}
        />
      }
    >
      {tab === "graft" ? (
        <GraftPlayground />
      ) : (
        <div className="ws-panes">
          <Card className="ws-pane ws-pane-form">
            <CardHeader title="Project" subtitle="Project scripts and .env files are never opened or run." />
            <form
              className="ws-form"
              onSubmit={(event) => {
                event.preventDefault();
                if (root.trim()) void run();
              }}
            >
              <Field label="Project directory">
                {(id) => (
                  <TextField
                    id={id}
                    mono
                    value={root}
                    onChange={(e) => {
                      setRoot(e.target.value);
                      setReport(null);
                    }}
                    placeholder="/home/you/projects/my-app"
                    disabled={busy}
                  />
                )}
              </Field>
              <Field label="Expected TCP ports" hint="Optional. Checked on IPv4 localhost.">
                {(id) => (
                  <TextField
                    id={id}
                    mono
                    value={ports}
                    onChange={(e) => {
                      setPorts(e.target.value);
                      setReport(null);
                    }}
                    placeholder="3000, 5432"
                    disabled={busy}
                  />
                )}
              </Field>
              <Field label="Expected Docker containers" hint="Optional. Exact container names.">
                {(id) => (
                  <TextField
                    id={id}
                    mono
                    value={containers}
                    onChange={(e) => {
                      setContainers(e.target.value);
                      setReport(null);
                    }}
                    placeholder="my-app-db, my-app-cache"
                    disabled={busy}
                  />
                )}
              </Field>
              <div className="ws-form-actions">
                <Button type="submit" variant="primary" busy={busy} disabled={!root.trim()}>
                  {busy ? "Checking project…" : "Check project"}
                </Button>
              </div>
            </form>
            <p className="ws-footnote">Up to 16 ports and 16 containers. An open port confirms a TCP listener, not application health. Nothing runs in the background.</p>
          </Card>

          <Card className="ws-pane ws-pane-result" padding="none" aria-live="polite">
            {busy ? (
              <SkeletonRows rows={5} label="Checking project" />
            ) : error ? (
              <div className="ws-pane-pad">
                <Notice tone="critical" title="The check did not finish" actions={<Button size="small" disabled={!root.trim()} onClick={() => void run()}>Retry</Button>}>
                  {error}
                </Notice>
              </div>
            ) : report ? (
              <>
                <CardHeader
                  title={report.project || "Project"}
                  subtitle={<span className="ui-mono">{report.root}</span>}
                  actions={
                    <span className="ws-summary">
                      <Badge tone="good">{`${passed} ready`}</Badge>
                      {flagged > 0 && <Badge tone="warning">{`${flagged} to check`}</Badge>}
                    </span>
                  }
                />
                <List inset={false} label="Checks" className="ws-checks">
                  {report.checks.map((check, index) => {
                    const pass = check.status === "pass";
                    return (
                      <ListRow
                        key={index}
                        className={pass ? "ws-check-pass" : "ws-check-warn"}
                        leading={pass ? <CheckCircle2 {...ICON} aria-hidden="true" /> : <AlertTriangle {...ICON} aria-hidden="true" />}
                        title={check.name}
                        subtitle={check.detail}
                        trailing={<Badge tone={pass ? "good" : "warning"}>{pass ? "Ready" : "Check needed"}</Badge>}
                      />
                    );
                  })}
                </List>
              </>
            ) : (
              <div className="ws-pane-empty">
                <EmptyState icon={<Stethoscope {...ICON} />} title="No checks yet">
                  Choose a project directory and run the checks to see what is ready.
                </EmptyState>
              </div>
            )}
          </Card>
        </div>
      )}
    </Page>
  );
}
