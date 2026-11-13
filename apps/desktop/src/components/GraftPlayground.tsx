import { useEffect, useRef, useState } from "react";
import { ipc } from "../lib/ipc";
import { methodArguments, type GraftDiscovery } from "../lib/workspace";

export function GraftPlayground() {
  const [endpoint, setEndpoint] = useState("http://127.0.0.1:18772/mcp"), [token, setToken] = useState("");
  const [discovery, setDiscovery] = useState<GraftDiscovery | null>(null);
  const [name, setName] = useState(""), [argumentsText, setArgumentsText] = useState("{}");
  const [result, setResult] = useState<unknown>(undefined), [error, setError] = useState("");
  const [busy, setBusy] = useState(false), [pending, setPending] = useState<Record<string, unknown> | null>(null);
  const [copied, setCopied] = useState(false);
  const running = useRef(false), live = useRef(true);
  useEffect(() => { live.current = true; return () => { live.current = false; }; }, []);
  const reset = () => { setDiscovery(null); setName(""); setResult(undefined); setPending(null); setError(""); setCopied(false); };
  const execute = async (call: boolean) => {
    if (running.current) return;
    running.current = true; setBusy(true); setError(""); setCopied(false);
    try {
      const connection = { endpoint: endpoint.trim(), token };
      if (call) {
        if (!pending) return;
        setResult(undefined);
        const value = await ipc.graftCall(connection, name, pending, true);
        if (live.current) { setResult(value); setPending(null); }
      } else {
        reset();
        const value = await ipc.graftDiscover(connection);
        if (live.current) { setDiscovery(value); setName(value.tools[0]?.name || ""); setArgumentsText("{}"); }
      }
    } catch (e) { if (live.current) { setError(String(e)); setPending(null); } }
    finally { running.current = false; if (live.current) setBusy(false); }
  };
  const prepare = () => {
    setError(""); setResult(undefined);
    try { setPending(methodArguments(argumentsText)); } catch (e) { setError(String(e)); }
  };
  const tool = discovery?.tools.find(t => t.name === name);
  return <>
    <section className="card workspace-form"><h2>Graftcode Playground</h2><p className="muted">Connect a trusted local Graftcode Gateway to browse and call its methods. Veronica does not install packages, start a Gateway or execute project files automatically.</p>
      <label>Gateway MCP endpoint<input value={endpoint} onChange={e => { setEndpoint(e.target.value); reset(); }} disabled={busy} spellCheck={false} maxLength={512} /></label>
      <label>Session access token <span className="muted">if required; sent as bearer header and declared sessionToken argument; kept only here</span><input type="password" autoComplete="off" value={token} onChange={e => { setToken(e.target.value); reset(); }} disabled={busy} maxLength={2048} /></label>
      <div className="button-row"><button className="button" disabled={busy || !endpoint.trim()} onClick={() => void execute(false)}>{busy ? "Waiting for Gateway…" : "Connect & discover"}</button><button className="button" disabled={busy} onClick={() => { reset(); setToken(""); }}>Clear session</button></div>
      <small className="muted">Loopback only · no polling · up to 64 tools · 256 KiB response limit. Calls can change your project; inspect the method before confirming.</small>
    </section>
    {error && <div className="banner error" role="alert">{error}</div>}
    {discovery && <section className="card workspace-form"><h2>{discovery.server?.name || "Connected Gateway"}</h2><p className="muted">{discovery.tools.length} callable methods{discovery.server?.version ? ` · ${discovery.server.version}` : ""}</p>
      {discovery.tools.length === 0 ? <p>No callable methods. Check the Gateway's module and method filters.</p> : <>
        <label>Method<select value={name} disabled={busy || !!pending} onChange={e => { setName(e.target.value); setArgumentsText("{}"); setResult(undefined); setCopied(false); }}><option value="">Choose a method</option>{discovery.tools.map(t => <option key={t.name} value={t.name}>{t.name}</option>)}</select></label>
        {tool?.description && <p className="muted">{tool.description}</p>}
        <details><summary>Input schema</summary><pre>{JSON.stringify(tool?.inputSchema || {}, null, 2)}</pre></details>
        <label>Arguments (JSON)<textarea value={argumentsText} onChange={e => { setArgumentsText(e.target.value); setResult(undefined); }} disabled={busy || !!pending} maxLength={16384} spellCheck={false} /></label>
        <div><button className="button" disabled={busy || !name || !!pending} onClick={prepare}>Review call</button></div>
      </>}
      {pending && <div className="workspace-confirm" role="region" aria-label="Confirm method call"><h3>Run {name}?</h3><p className="muted">This invokes code in {endpoint}. A timeout does not guarantee that the method stopped. Calls are never retried automatically.</p><pre>{JSON.stringify(pending,null,2)}</pre><div className="button-row"><button className="button" disabled={busy} onClick={() => setPending(null)}>Cancel</button><button className="button danger" disabled={busy} onClick={() => void execute(true)}>Confirm & run</button></div></div>}
    </section>}
    {result !== undefined && <section className="card" aria-live="polite"><h2>{(result as { isError?: boolean } | null)?.isError ? "Method returned an error" : "Method result"}</h2><pre>{JSON.stringify(result,null,2)}</pre><button className="button" onClick={() => { void navigator.clipboard.writeText(JSON.stringify(result,null,2)).then(() => setCopied(true)).catch(() => setError("Clipboard unavailable. Select and copy the result manually.")); }}>{copied ? "Copied" : "Copy result"}</button></section>}
    {discovery && <section className="card"><h2>Generated Graft packages</h2><p className="muted">Gateway-advertised install commands, shown as text only. Review their registry and package before using them in your own project. Metadata upload may be required for package generation; the private Doctor launcher disables it by default.</p><pre>{JSON.stringify(discovery.installCommands,null,2)}</pre></section>}
    <section className="card"><h2>Connect your project</h2><p className="muted">Use Graftcode Gateway with your supported Node.js, Python, .NET, Java, PHP or Ruby module. The optional Veronica Project Doctor receiver exposes only read-only checks for one chosen workspace. Setup instructions are in docs/GRAFTCODE.md in the repository.</p><p className="muted">Gateway ownership stays with you: clearing this page does not stop a separately started Gateway. No methods run during discovery.</p></section>
  </>;
}
