import { useCallback, useEffect, useState, type ReactNode } from "react";
import { ArrowUp, ChevronLeft, ChevronRight, Database, Play, Table2 } from "lucide-react";

import { ipc } from "../lib/ipc";
import type {
  DatabaseConnection,
  DatabaseObject,
  DatabasePage as ResultPage,
  DatabaseOperation,
} from "../lib/types";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  IconButton,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
  SkeletonRows,
  Table,
  Tabs,
  TextArea,
  Toolbar,
} from "../ui";
import "./DatabasePage.css";

const STROKE = 1.75;

type Tab = "browse" | "query" | "history";

type PageSource =
  | { kind: "read"; path: string[]; size: number }
  | { kind: "query"; statement: string; size: number };

type Paging = { offset: number; onPrevious?: () => void; onNext?: () => void; busy: boolean };

const READABLE = new Set(["table", "view", "materializedView", "collection", "key"]);

const TABS: { id: Tab; label: string }[] = [
  { id: "browse", label: "Browse" },
  { id: "query", label: "Query" },
  { id: "history", label: "History" },
];

function isReadOnly(connection: DatabaseConnection) {
  return (
    connection.readOnlyPolicy === "required" ||
    connection.environment.protection === "readOnly" ||
    connection.productionPolicy === "prohibitMutations"
  );
}

export function DatabasePage() {
  const [connections, setConnections] = useState<DatabaseConnection[] | null>(null);
  const [connectionsError, setConnectionsError] = useState("");
  const [selected, setSelected] = useState<string>("");
  const [tab, setTab] = useState<Tab>("browse");
  const [objects, setObjects] = useState<DatabaseObject[]>([]);
  const [objectsRead, setObjectsRead] = useState(false);
  const [path, setPath] = useState<string[]>([]);
  const [page, setPage] = useState<ResultPage | null>(null);
  const [statement, setStatement] = useState("");
  const [history, setHistory] = useState<DatabaseOperation[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const loadConnections = useCallback(() => {
    setConnectionsError("");
    void ipc
      .databaseConnections()
      .then((all) => {
        setConnections(all);
        if (all.length > 0) setSelected((current) => current || all[0].id);
      })
      .catch((reason) => setConnectionsError(String(reason)));
  }, []);

  useEffect(loadConnections, [loadConnections]);

  const browse = useCallback(async (connection: string, into: string[]) => {
    setBusy(true);
    setError("");
    setPage(null);
    try {
      setObjects(await ipc.databaseBrowse(connection, into));
      setPath(into);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
      setObjectsRead(true);
    }
  }, []);

  useEffect(() => {
    if (!selected || tab !== "browse") return;
    void browse(selected, []);
  }, [selected, tab, browse]);

  useEffect(() => {
    if (tab !== "history") return;
    void ipc
      .databaseOperations(50)
      .then(setHistory)
      .catch((reason) => setError(String(reason)));
  }, [tab]);

  const [source, setSource] = useState<PageSource | null>(null);
  const [offset, setOffset] = useState(0);

  const fetchPage = async (from: PageSource, at: number) => {
    setBusy(true);
    setError("");
    try {
      setPage(
        from.kind === "read"
          ? await ipc.databaseRead(selected, from.path, from.size, at)
          : await ipc.databaseQuery(selected, from.statement, from.size, at),
      );
      setSource(from);
      setOffset(at);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    setPage(null);
    setSource(null);
    setOffset(0);
  }, [selected]);

  const open = async (object: DatabaseObject) => {
    if (object.kind === "column") return;
    await fetchPage({ kind: "read", path: object.path, size: 100 }, 0);
  };

  const run = async () => {
    if (!statement.trim()) return;
    setPage(null);
    await fetchPage({ kind: "query", statement, size: 200 }, 0);
  };

  const paging =
    page && source
      ? {
          offset,
          onPrevious:
            offset > 0
              ? () => void fetchPage(source, Math.max(0, offset - source.size))
              : undefined,
          onNext:
            page.hasMore && page.nextOffset !== null
              ? () => void fetchPage(source, page.nextOffset!)
              : undefined,
          busy,
        }
      : undefined;

  const connection = connections?.find((entry) => entry.id === selected);
  const readOnly = connection !== undefined && isReadOnly(connection);
  const summary = connection ? describeLocation(connection) : "";

  if (connections === null || connections.length === 0) {
    return (
      <Page
        header={
          <PageHeader title="Database" subtitle="Explore databases and run guarded changes" />
        }
      >
        {connectionsError ? (
          <Notice
            tone="critical"
            title="Saved connections could not be read"
            actions={
              <Button size="small" onClick={loadConnections}>
                Retry
              </Button>
            }
          >
            {connectionsError}
          </Notice>
        ) : connections === null ? (
          <div className="db-layout db-layout-static">
            <List label="Connections">
              <div className="db-pad">
                <SkeletonRows rows={3} label="Reading saved connections" />
              </div>
            </List>
            <Card>
              <SkeletonRows rows={5} />
            </Card>
          </div>
        ) : (
          <Card>
            <EmptyState icon={<Database strokeWidth={STROKE} />} title="No saved connections">
              Add one from the command line, where a password can be given on stdin rather than
              ending up in a shell history.
            </EmptyState>
            <pre className="db-command ui-mono ui-selectable">
              {
                "vr database connections add local postgresql 127.0.0.1:5432 \\\n  --username me --database app --password-stdin"
              }
            </pre>
            <p className="db-footnote db-center">
              Passwords go to the desktop keyring. A connection definition never holds one.
            </p>
          </Card>
        )}
      </Page>
    );
  }

  const placeholder =
    connection?.productHint === "redis" || connection?.productHint === "valkey"
      ? "session:*"
      : "SELECT * FROM orders LIMIT 20";

  return (
    <Page
      width="fluid"
      fill
      className="db-page"
      header={
        <PageHeader
          title="Database"
          subtitle={
            <span className="ui-mono">{summary || "Reads only. Changes go through vr."}</span>
          }
          accessory={<Tabs label="Database views" items={TABS} value={tab} onChange={setTab} />}
        />
      }
    >
      <div className="db-layout">
        <div className="db-connections">
          <List label="Connections">
            {connections.map((entry) => (
              <ListRow
                key={entry.id}
                leading={<Database strokeWidth={STROKE} />}
                title={entry.displayName}
                subtitle={`${entry.productHint} · ${entry.environment.label}`}
                trailing={isReadOnly(entry) ? <Badge>Read-only</Badge> : undefined}
                selected={entry.id === selected}
                onClick={() => setSelected(entry.id)}
              />
            ))}
          </List>
        </div>

        <div className="db-workspace">
          {readOnly && (
            <Notice compact>This connection is read-only. No change can be made through it.</Notice>
          )}
          {error && (
            <Notice
              tone="critical"
              compact
              actions={
                <Button size="small" variant="plain" onClick={() => setError("")}>
                  Dismiss
                </Button>
              }
            >
              {error}
            </Notice>
          )}

          {tab === "browse" && (
            <div className="db-browse">
              <Card padding="none" className="db-objects">
                <div className="db-objects-head">
                  <IconButton
                    size="small"
                    label="Go up one level"
                    icon={<ArrowUp strokeWidth={STROKE} />}
                    disabled={path.length === 0 || busy}
                    onClick={() => void browse(selected, path.slice(0, -1))}
                  />
                  <span className="db-path ui-mono ui-selectable" title={path.join(" › ")}>
                    {path.length > 0 ? path.join(" › ") : "Objects"}
                  </span>
                </div>
                <div className="db-scroll">
                  {!objectsRead || (busy && objects.length === 0) ? (
                    <div className="db-pad">
                      <SkeletonRows rows={5} label="Reading objects" />
                    </div>
                  ) : objects.length === 0 ? (
                    <EmptyState size="compact" title="Nothing here">
                      This level holds no objects.
                    </EmptyState>
                  ) : (
                    <List label="Objects" inset={false}>
                      {objects.map((object) => (
                        <ListRow
                          key={object.path.join(".")}
                          title={object.path[object.path.length - 1]}
                          mono
                          subtitle={`${object.kind}${object.nativeIdentifier ? ` · ${object.nativeIdentifier}` : ""}`}
                          onClick={
                            object.kind === "column"
                              ? undefined
                              : () => {
                                  if (!busy) void browse(selected, object.path);
                                }
                          }
                          trailing={
                            READABLE.has(object.kind) ? (
                              <Button
                                size="small"
                                variant="plain"
                                disabled={busy}
                                onClick={() => void open(object)}
                              >
                                Read
                              </Button>
                            ) : undefined
                          }
                        />
                      ))}
                    </List>
                  )}
                </div>
              </Card>
              <ResultPane
                page={page}
                paging={paging}
                busy={busy && source !== null}
                empty={
                  <EmptyState
                    size="compact"
                    icon={<Table2 strokeWidth={STROKE} />}
                    title="No rows yet"
                  >
                    Choose Read next to a table or view to see its rows.
                  </EmptyState>
                }
              />
            </div>
          )}

          {tab === "query" && (
            <div className="db-query">
              <Card className="db-editor">
                <TextArea
                  mono
                  className="db-statement"
                  value={statement}
                  onChange={(event) => setStatement(event.target.value)}
                  placeholder={placeholder}
                  spellCheck={false}
                  aria-label="Statement"
                />
                <Toolbar
                  leading={
                    <span className="db-footnote">
                      Reads only, a write is refused. To change data run{" "}
                      <span className="ui-mono">vr database mutations preview</span>, read what it
                      says, then apply it with the token and the exact confirmation text.
                    </span>
                  }
                  trailing={
                    <Button
                      variant="primary"
                      icon={<Play strokeWidth={STROKE} />}
                      busy={busy}
                      disabled={!statement.trim()}
                      onClick={() => void run()}
                    >
                      {busy ? "Running…" : "Run"}
                    </Button>
                  }
                />
              </Card>
              <ResultPane
                page={page}
                paging={paging}
                busy={busy}
                empty={
                  <EmptyState
                    size="compact"
                    icon={<Table2 strokeWidth={STROKE} />}
                    title="No results yet"
                  >
                    Write a statement and run it to see rows here.
                  </EmptyState>
                }
              />
            </div>
          )}

          {tab === "history" && (
            <Card padding="none" className="db-history">
              <div className="db-history-head">
                <span className="db-pane-title">What Veronica has done</span>
                <span className="db-footnote">Newest first</span>
              </div>
              <div className="db-scroll">
                {history === null ? (
                  <div className="db-pad">
                    <SkeletonRows rows={5} label="Reading history" />
                  </div>
                ) : history.length === 0 ? (
                  <EmptyState size="compact" title="Nothing yet">
                    Reads and queries from Veronica and vr appear here.
                  </EmptyState>
                ) : (
                  <List label="Operations" inset={false}>
                    {history.map((record) => (
                      <ListRow
                        key={record.id}
                        title={record.summary}
                        mono
                        subtitle={`${record.connectionName} · ${new Date(record.at).toLocaleString()}${record.error ? ` · ${record.error}` : ""}`}
                        meta={
                          record.affectedRecords !== null
                            ? `${record.affectedRecords} record${record.affectedRecords === 1 ? "" : "s"}`
                            : undefined
                        }
                        trailing={
                          <Badge
                            tone={
                              record.outcome === "succeeded"
                                ? "good"
                                : record.outcome === "failed"
                                  ? "critical"
                                  : "neutral"
                            }
                          >
                            {record.outcome === "succeeded"
                              ? "Succeeded"
                              : record.outcome === "failed"
                                ? "Failed"
                                : "Abandoned"}
                          </Badge>
                        }
                      />
                    ))}
                  </List>
                )}
              </div>
            </Card>
          )}
        </div>
      </div>
    </Page>
  );
}

function ResultPane({
  page,
  paging,
  busy,
  empty,
}: {
  page: ResultPage | null;
  paging?: Paging;
  busy: boolean;
  empty: ReactNode;
}) {
  if (!page) {
    return (
      <Card padding="none" className="db-results">
        {busy ? (
          <div className="db-pad">
            <SkeletonRows rows={6} label="Reading rows" />
          </div>
        ) : (
          empty
        )}
      </Card>
    );
  }
  const first = (paging?.offset ?? 0) + 1;
  const title =
    paging && paging.offset > 0
      ? `Rows ${first} to ${first + page.rows.length - 1}`
      : `${page.rows.length} row${page.rows.length === 1 ? "" : "s"}`;
  return (
    <Card padding="none" className="db-results">
      <div className="db-results-head">
        <div className="db-results-titles">
          <span className="db-pane-title ui-num">{title}</span>
          <span className="db-footnote ui-num">
            {page.elapsedMillis} ms{page.hasMore ? " · more available" : ""}
          </span>
        </div>
        {(paging?.onPrevious || paging?.onNext) && (
          <div className="db-results-paging">
            <IconButton
              size="small"
              label="Previous page"
              icon={<ChevronLeft strokeWidth={STROKE} />}
              disabled={!paging?.onPrevious || paging.busy}
              onClick={paging?.onPrevious}
            />
            <IconButton
              size="small"
              label="Next page"
              icon={<ChevronRight strokeWidth={STROKE} />}
              disabled={!paging?.onNext || paging.busy}
              onClick={paging?.onNext}
            />
          </div>
        )}
      </div>
      <div className="db-scroll db-grid">
        {page.rows.length === 0 ? (
          <EmptyState size="compact" title="No rows">
            The statement returned nothing.
          </EmptyState>
        ) : (
          <Table label="Results">
            <thead>
              <tr>
                {page.columns.map((column) => (
                  <th key={column.name} title={column.typeName ?? undefined}>
                    {column.name}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {page.rows.map((row, index) => (
                <tr key={index}>
                  {row.map((value, cell) => (
                    <td
                      key={cell}
                      className={
                        value.kind === "null" || value.kind === "missing" ? "muted" : undefined
                      }
                    >
                      {renderValue(value)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </div>
    </Card>
  );
}

function renderValue(value: { kind: string; value?: unknown }): string {
  switch (value.kind) {
    case "missing":
      return "–";
    case "null":
      return "NULL";
    case "array":
      return `[${(value.value as unknown[])?.length ?? 0} items]`;
    case "object":
      return `{${(value.value as unknown[])?.length ?? 0} fields}`;
    case "binary":
      return "binary";
    case "decimal":
      return typeof value.value === "string" ? value.value : String(value.value ?? "");
    case "productSpecific":
      return String((value.value as { rendered?: string })?.rendered ?? "");
    default:
      return String(value.value ?? "");
  }
}

function describeLocation(connection: DatabaseConnection): string {
  const where = connection.location;
  const place =
    where.kind === "network"
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
