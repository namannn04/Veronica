import { useEffect, useMemo, useRef, useState } from "react";
import { Check, Copy, PlugZap } from "lucide-react";

import { ipc } from "../lib/ipc";
import { methodArguments, methodResult, type GraftDiscovery } from "../lib/workspace";
import { Badge, Button, ButtonGroup, Card, CardHeader, EmptyState, Field, IconButton, Notice, Select, TextArea, TextField } from "../ui";

const ICON = { strokeWidth: 1.75 };

export function GraftPlayground() {
  const [endpoint, setEndpoint] = useState("http://127.0.0.1:18772/mcp");
  const [token, setToken] = useState("");
  const [discovery, setDiscovery] = useState<GraftDiscovery | null>(null);
  const [name, setName] = useState("");
  const [argumentsText, setArgumentsText] = useState("{}");
  const [result, setResult] = useState<unknown>(undefined);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<Record<string, unknown> | null>(null);
  const [copied, setCopied] = useState(false);
  const [drafts, setDrafts] = useState<{ name: string; text: string }[]>([]);
  const [showSchema, setShowSchema] = useState(false);
  const [showProtocol, setShowProtocol] = useState(false);
  const [showInstall, setShowInstall] = useState(false);
  const running = useRef(false);
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  const reset = () => {
    setDiscovery(null);
    setName("");
    setResult(undefined);
    setPending(null);
    setError("");
    setCopied(false);
    setDrafts([]);
  };

  const execute = async (call: boolean) => {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError("");
    setCopied(false);
    try {
      const connection = { endpoint: endpoint.trim(), token };
      if (call) {
        if (!pending) return;
        setResult(undefined);
        const value = await ipc.graftCall(connection, name, pending, true);
        if (live.current) {
          setResult(value);
          setPending(null);
        }
      } else {
        reset();
        const value = await ipc.graftDiscover(connection);
        if (live.current) {
          setDiscovery(value);
          setName(value.tools[0]?.name || "");
          setArgumentsText("{}");
        }
      }
    } catch (e) {
      if (live.current) {
        setError(String(e));
        setPending(null);
      }
    } finally {
      running.current = false;
      if (live.current) setBusy(false);
    }
  };

  const prepare = () => {
    setError("");
    setResult(undefined);
    try {
      setPending(methodArguments(argumentsText));
    } catch (e) {
      setError(String(e));
    }
  };

  const tool = discovery?.tools.find((t) => t.name === name);
  const displayResult = useMemo(() => methodResult(result), [result]);
  const failed = Boolean((result as { isError?: boolean } | null)?.isError || (displayResult as { error?: unknown } | null)?.error);

  const keepDraft = () => {
    try {
      const value = methodArguments(argumentsText);
      if ("sessionToken" in value) throw new Error("Put the session token in the access-token field before keeping a call.");
      setDrafts((items) => [...items.filter((item) => item.name !== name || item.text !== argumentsText), { name, text: argumentsText }].slice(-8));
    } catch (e) {
      setError(String(e));
    }
  };

  const copyResult = () => {
    void navigator.clipboard
      .writeText(JSON.stringify(displayResult, null, 2))
      .then(() => setCopied(true))
      .catch(() => setError("Clipboard unavailable. Select and copy the result manually."));
  };

  return (
    <div className="ws-panes">
      <div className="ws-pane ws-pane-stack">
        <Card>
          <CardHeader title="Gateway" subtitle="Veronica never installs packages, starts a Gateway or runs project files on its own." />
          <form
            className="ws-form"
            onSubmit={(event) => {
              event.preventDefault();
              if (endpoint.trim()) void execute(false);
            }}
          >
            <Field label="Gateway MCP endpoint">
              {(id) => (
                <TextField
                  id={id}
                  mono
                  value={endpoint}
                  onChange={(e) => {
                    setEndpoint(e.target.value);
                    reset();
                  }}
                  disabled={busy}
                  maxLength={512}
                />
              )}
            </Field>
            <Field label="Session access token" hint="Only if required. Sent as a bearer header and as a declared sessionToken argument; kept on this page only.">
              {(id) => (
                <TextField
                  id={id}
                  type="password"
                  autoComplete="off"
                  value={token}
                  onChange={(e) => {
                    setToken(e.target.value);
                    reset();
                  }}
                  disabled={busy}
                  maxLength={2048}
                />
              )}
            </Field>
            <div className="ws-form-actions">
              <Button
                variant="plain"
                disabled={busy}
                onClick={() => {
                  reset();
                  setToken("");
                }}
              >
                Clear session
              </Button>
              <Button type="submit" variant="primary" icon={<PlugZap {...ICON} />} busy={busy && !discovery} disabled={busy || !endpoint.trim()}>
                {busy && !discovery ? "Waiting for Gateway…" : "Connect and discover"}
              </Button>
            </div>
          </form>
          <p className="ws-footnote">Loopback only, no polling, up to 64 tools and a 256 KiB response limit. Calls can change your project; inspect a method before confirming.</p>
        </Card>

        {error && (
          <Notice tone="critical" title="The Gateway reported a problem">
            {error}
          </Notice>
        )}

        {discovery && (
          <Card>
            <CardHeader
              title={discovery.server?.name || "Connected Gateway"}
              subtitle={`${discovery.tools.length} callable method${discovery.tools.length === 1 ? "" : "s"}${discovery.server?.version ? `, version ${discovery.server.version}` : ""}`}
            />
            {discovery.tools.length === 0 ? (
              <EmptyState size="compact" title="No callable methods">
                Check the Gateway's module and method filters.
              </EmptyState>
            ) : (
              <div className="ws-form">
                <Field label="Method" hint={tool?.description}>
                  {(id) => (
                    <Select
                      id={id}
                      value={name}
                      disabled={busy || !!pending}
                      onChange={(e) => {
                        setName(e.target.value);
                        setArgumentsText("{}");
                        setResult(undefined);
                        setCopied(false);
                      }}
                    >
                      <option value="">Choose a method</option>
                      {discovery.tools.map((t) => (
                        <option key={t.name} value={t.name}>
                          {t.name}
                        </option>
                      ))}
                    </Select>
                  )}
                </Field>
                <div className="ws-disclosure">
                  <Button size="small" variant="plain" aria-expanded={showSchema} onClick={() => setShowSchema(!showSchema)}>
                    {showSchema ? "Hide input schema" : "Show input schema"}
                  </Button>
                  {showSchema && <pre className="ws-code ui-mono ui-selectable">{JSON.stringify(tool?.inputSchema || {}, null, 2)}</pre>}
                </div>
                <Field label="Arguments" hint="A JSON object, up to 16 KiB.">
                  {(id) => (
                    <TextArea
                      id={id}
                      mono
                      rows={5}
                      value={argumentsText}
                      onChange={(e) => {
                        setArgumentsText(e.target.value);
                        setResult(undefined);
                      }}
                      disabled={busy || !!pending}
                      maxLength={16384}
                      spellCheck={false}
                    />
                  )}
                </Field>
                {drafts.length > 0 && (
                  <Field label="Saved calls" hint="Up to eight, cleared when this session changes.">
                    {(id) => (
                      <Select
                        id={id}
                        value=""
                        disabled={busy || !!pending}
                        onChange={(e) => {
                          const draft = drafts[Number(e.target.value)];
                          if (draft) {
                            setName(draft.name);
                            setArgumentsText(draft.text);
                            setResult(undefined);
                          }
                        }}
                      >
                        <option value="">Load a saved call</option>
                        {drafts.map((draft, i) => (
                          <option key={i} value={i}>
                            {i + 1}. {draft.name}
                          </option>
                        ))}
                      </Select>
                    )}
                  </Field>
                )}
                {pending ? (
                  <Notice
                    tone="warning"
                    title={`Run ${name}?`}
                    actions={
                      <ButtonGroup>
                        <Button size="small" disabled={busy} onClick={() => setPending(null)}>
                          Cancel
                        </Button>
                        <Button size="small" variant="destructive" busy={busy} onClick={() => void execute(true)}>
                          Confirm and run
                        </Button>
                      </ButtonGroup>
                    }
                  >
                    <p>This runs code behind {endpoint}. A timeout does not guarantee the method stopped, and calls are never retried.</p>
                    <pre className="ws-code ui-mono ui-selectable">{JSON.stringify(pending, null, 2)}</pre>
                  </Notice>
                ) : (
                  <div className="ws-form-actions">
                    <Button variant="plain" disabled={busy || !name} onClick={keepDraft}>
                      Keep call for this session
                    </Button>
                    <Button disabled={busy || !name} onClick={prepare}>
                      Review call
                    </Button>
                  </div>
                )}
              </div>
            )}
          </Card>
        )}
      </div>

      <div className="ws-pane ws-pane-stack">
        <Card className="ws-response" aria-live="polite">
          <CardHeader
            title="Response"
            subtitle={result === undefined ? "The method result appears here after you confirm a call." : undefined}
            actions={
              result !== undefined && (
                <>
                  <Badge tone={failed ? "critical" : "good"}>{failed ? "Method returned an error" : "Method result"}</Badge>
                  <IconButton size="small" label={copied ? "Copied" : "Copy result"} icon={copied ? <Check {...ICON} /> : <Copy {...ICON} />} onClick={copyResult} />
                </>
              )
            }
          />
          {result === undefined ? (
            <pre className="ws-code ws-code-placeholder ui-mono">{"{ }"}</pre>
          ) : (
            <>
              <pre className="ws-code ui-mono ui-selectable">{JSON.stringify(displayResult, null, 2)}</pre>
              <div className="ws-disclosure">
                <Button size="small" variant="plain" aria-expanded={showProtocol} onClick={() => setShowProtocol(!showProtocol)}>
                  {showProtocol ? "Hide protocol response" : "Show protocol response"}
                </Button>
                {showProtocol && <pre className="ws-code ui-mono ui-selectable">{JSON.stringify(result, null, 2)}</pre>}
              </div>
            </>
          )}
        </Card>

        {discovery && (
          <Card>
            <CardHeader
              title="Generated Graft packages"
              subtitle="Install commands the Gateway advertises, shown as text only. Review their registry and package before using them. Package generation may need a metadata upload; the private Doctor launcher turns it off by default."
            />
            <div className="ws-disclosure">
              <Button size="small" variant="plain" aria-expanded={showInstall} onClick={() => setShowInstall(!showInstall)}>
                {showInstall ? "Hide install commands" : "Show install commands"}
              </Button>
              {showInstall && <pre className="ws-code ui-mono ui-selectable">{JSON.stringify(discovery.installCommands, null, 2)}</pre>}
            </div>
          </Card>
        )}

        <Card tone="inset">
          <CardHeader title="Connect your project" />
          <p className="ws-text">
            Use Graftcode Gateway with a supported Node.js, Python, .NET, Java, PHP or Ruby module. The optional Veronica Project Doctor receiver exposes only read-only checks
            for one chosen workspace. Setup steps are in <span className="ui-mono">docs/GRAFTCODE.md</span> in the repository.
          </p>
          <p className="ws-text">You own the Gateway: clearing this page does not stop a Gateway you started. No methods run during discovery.</p>
        </Card>
      </div>
    </div>
  );
}
