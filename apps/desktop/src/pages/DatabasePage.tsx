import { useCallback, useEffect, useState } from "react";

import { ipc } from "../lib/ipc";
import type { DatabaseConnection, DatabaseObject, DatabasePage, DatabaseOperation } from "../lib/types";

/**
 * The Database page.
 *
 * Reads only. Browsing, reading a table and running a statement all go through
 * the same adapters the CLI uses, and a statement that would write is refused
 * there rather than here.
 *
 * A change is deliberately not startable from this page. It goes through
 * `vr database mutations preview`, which prints what it would do, the warnings
 * it earns and the exact text to type back — a flow that reads badly as a
 * dialog and reads well as something you have to look at. The page says so
 * rather than offering a button that would open a wizard nobody should rush.
 */
type Tab = "browse" | "query" | "history";

export function DatabasePage() {
  const [connections, setConnections] = useState<DatabaseConnection[]>([]);
  const [selected, setSelected] = useState<string>("");
  const [tab, setTab] = useState<Tab>("browse");
  const [objects, setObjects] = useState<DatabaseObject[]>([]);
  const [path, setPath] = useState<string[]>([]);
  const [page, setPage] = useState<DatabasePage | null>(null);
  const [statement, setStatement] = useState("");
  const [history, setHistory] = useState<DatabaseOperation[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    void ipc
      .databaseConnections()
      .then((all) => {
        setConnections(all);
        if (all.length > 0) setSelected((current) => current || all[0].id);
      })
      .catch((reason) => setError(String(reason)));
  }, []);

  const browse = useCallback(async (connection: string, into: string[]) => {
    setBusy(true); setError(""); setPage(null);
    try {
      setObjects(await ipc.databaseBrowse(connection, into));
      setPath(into);
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }, []);

  useEffect(() => {
    if (!selected || tab !== "browse") return;
    void browse(selected, []);
  }, [selected, tab, browse]);

  useEffect(() => {
    if (tab !== "history") return;
    void ipc.databaseOperations(50).then(setHistory).catch((reason) => setError(String(reason)));
  }, [tab]);

  const open = async (object: DatabaseObject) => {
    // A column has nothing inside it; the row it belongs to is what to show.
    if (object.kind === "column") return;
    setBusy(true); setError("");
    try {
      setPage(await ipc.databaseRead(selected, object.path, 100, 0));
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  };

  const run = async () => {
    if (!statement.trim()) return;
    setBusy(true); setError(""); setPage(null);
    try {
      setPage(await ipc.databaseQuery(selected, statement, 200, 0));
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  };

  const connection = connections.find((entry) => entry.id === selected);
  // Derived here rather than expected from the backend: the definition carries
  // the policy switches, and what they mean for this page is a front-end
  // question. `ConnectionDefinition::mutation_prohibition` is the same rule.
  const readOnly = connection !== undefined
    && (connection.readOnlyPolicy === "required"
      || connection.environment.protection === "readOnly"
      || connection.productionPolicy === "prohibitMutations");
  const summary = connection ? describeLocation(connection) : "";

  if (connections.length === 0) {
    return <div className="page">
      <div className="page-head edith-head"><div><h1>Database</h1><div className="page-sub">Explore databases and run guarded changes</div></div></div>
      <section className="settings-info">
        <div className="info-glyph">◇</div>
        <h2>No saved connections</h2>
        <p>
          Add one from the command line, where a password can be given on stdin rather than
          ending up in a shell history:
        </p>
        <pre>vr database connections add local postgresql 127.0.0.1:5432 \
  --username me --database app --password-stdin</pre>
        <p>Passwords go to the desktop keyring. A connection definition never holds one.</p>
      </section>
    </div>;
  }

  return <div className="page database-page">
    <div className="page-head edith-head">
      <div><h1>Database</h1><div className="page-sub">{summary || "Reads only — changes go through vr"}</div></div>
      <div className="head-actions">
        <select value={selected} onChange={(event) => setSelected(event.target.value)} aria-label="Connection">
          {connections.map((entry) => <option key={entry.id} value={entry.id}>{entry.displayName}</option>)}
        </select>
      </div>
    </div>

    <div className="pill-tabs">
      {(["browse", "query", "history"] as Tab[]).map((item) => (
        <button key={item} className={tab === item ? "active" : ""} onClick={() => setTab(item)}>
          {item[0].toUpperCase() + item.slice(1)}
        </button>
      ))}
    </div>

    {error && <div className="banner error">{error}</div>}
    {readOnly && <div className="banner">This connection is read-only. No change can be made through it.</div>}

    {tab === "browse" && <>
      <section className="card">
        <div className="card-head">
          <h2>{path.length > 0 ? path.join(" › ") : "Objects"}</h2>
          {path.length > 0 && <button className="button" onClick={() => void browse(selected, path.slice(0, -1))}>Up</button>}
        </div>
        {objects.length === 0
          ? <p className="quiet">{busy ? "Reading…" : "Nothing here."}</p>
          : <div className="list">
              {objects.map((object) => (
                <div className="list-row" key={object.path.join(".")}>
                  <button className="quinjet-name" onClick={() => void browse(selected, object.path)} disabled={object.kind === "column"}>
                    <strong>{object.path[object.path.length - 1]}</strong>
                    <small>{object.kind}{object.nativeIdentifier ? ` · ${object.nativeIdentifier}` : ""}</small>
                  </button>
                  {object.kind !== "column" && <button className="button" onClick={() => void open(object)}>Read</button>}
                </div>
              ))}
            </div>}
      </section>
      {page && <ResultGrid page={page} />}
    </>}

    {tab === "query" && <>
      <section className="card">
        <div className="card-head"><h2>Statement</h2><span className="card-note">reads only — a write is refused</span></div>
        <textarea
          className="companion-body"
          value={statement}
          onChange={(event) => setStatement(event.target.value)}
          placeholder={connection?.productHint === "redis" || connection?.productHint === "valkey" ? "session:*" : "SELECT * FROM orders LIMIT 20"}
          spellCheck={false}
        />
        <div className="cleaner-actions">
          <button className="button primary" disabled={busy || !statement.trim()} onClick={() => void run()}>
            {busy ? "Running…" : "Run"}
          </button>
          <span className="card-note">
            To change data: <code>vr database mutations preview</code>, read what it says, then apply it
            with the token and the exact confirmation text.
          </span>
        </div>
      </section>
      {page && <ResultGrid page={page} />}
    </>}

    {tab === "history" && <section className="card">
      <div className="card-head"><h2>What Veronica has done</h2><span className="card-note">newest first</span></div>
      {history.length === 0
        ? <p className="quiet">Nothing yet.</p>
        : <div className="list">
            {history.map((record) => (
              <div className="list-row" key={record.id}>
                <div>
                  <strong>{record.summary}</strong>
                  <small>{record.connectionName} · {new Date(record.at).toLocaleString()}</small>
                </div>
                <span className={`pill ${record.outcome === "succeeded" ? "good" : record.outcome === "failed" ? "critical" : ""}`}>
                  {record.outcome}{record.affectedRecords !== null ? ` · ${record.affectedRecords}` : ""}
                </span>
              </div>
            ))}
          </div>}
    </section>}
  </div>;
}

/** A page of records. Wide results scroll inside the card rather than widening it. */
function ResultGrid({ page }: { page: DatabasePage }) {
  if (page.rows.length === 0) {
    return <section className="card"><p className="quiet">No rows.</p></section>;
  }
  return <section className="card">
    <div className="card-head">
      <h2>{page.rows.length} row{page.rows.length === 1 ? "" : "s"}</h2>
      <span className="card-note">{page.elapsedMillis} ms{page.hasMore ? " · more available" : ""}</span>
    </div>
    <div className="result-scroll">
      <table className="result-grid">
        <thead><tr>{page.columns.map((column) => <th key={column.name}>{column.name}</th>)}</tr></thead>
        <tbody>
          {page.rows.map((row, index) => (
            <tr key={index}>
              {row.map((value, cell) => <td key={cell} className={value.kind === "null" || value.kind === "missing" ? "quiet" : ""}>{renderValue(value)}</td>)}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  </section>;
}

/** The same rule `Value::render` applies in the core. */
function renderValue(value: { kind: string; value?: unknown }): string {
  switch (value.kind) {
    case "missing": return "—";
    case "null": return "NULL";
    case "array": return `[${(value.value as unknown[])?.length ?? 0} items]`;
    case "object": return `{${(value.value as unknown[])?.length ?? 0} fields}`;
    case "binary": return "binary";
    // Decimal is serialized transparently as its complete exact string. It is
    // not a tuple: indexing it would show only the first character of 19.99.
    case "decimal": return typeof value.value === "string" ? value.value : String(value.value ?? "");
    case "productSpecific": return String((value.value as { rendered?: string })?.rendered ?? "");
    default: return String(value.value ?? "");
  }
}

/** The same one-line summary `Location::summary` produces in the core. */
function describeLocation(connection: DatabaseConnection): string {
  const where = connection.location;
  const place = where.kind === "network"
    ? where.endpoints.length > 1
      ? `${where.endpoints[0].host}:${where.endpoints[0].port} +${where.endpoints.length - 1}`
      : where.endpoints.length === 1
        ? `${where.endpoints[0].host}:${where.endpoints[0].port}`
        : "no endpoint"
    : where.kind === "sqlite"
      ? where.sqlite.path
      : where.name
        ? `in memory (${where.name})`
        : "in memory";
  return `${connection.productHint} · ${connection.environment.label} · ${place}`;
}
