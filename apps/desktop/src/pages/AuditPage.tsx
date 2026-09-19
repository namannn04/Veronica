import { useState } from "react";

import { ipc } from "../lib/ipc";
import type { AuditReport, AuditSeverity } from "../lib/types";

/**
 * Site Audit.
 *
 * Edith's rules exactly — the same eleven checks, the same thresholds — so a
 * page that passes on macOS passes here.
 *
 * Every run stays local: Veronica fetches the pages being audited and nothing
 * else. No result is uploaded and no third-party service is consulted, which is
 * the whole reason to run an audit from your own machine.
 */
const SEVERITIES: { id: AuditSeverity; label: string; pill: string }[] = [
  { id: "error", label: "Errors", pill: "critical" },
  { id: "warning", label: "Warnings", pill: "warn" },
  { id: "notice", label: "Notices", pill: "" },
];

export function AuditPage() {
  const [site, setSite] = useState("");
  const [limit, setLimit] = useState(50);
  const [report, setReport] = useState<AuditReport | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState("");
  const [open, setOpen] = useState<string | null>(null);

  const run = async () => {
    if (!site.trim()) return;
    setRunning(true); setError(""); setReport(null); setOpen(null);
    try { setReport(await ipc.auditSite(site.trim(), limit, 4)); }
    catch (reason) { setError(String(reason)); }
    finally { setRunning(false); }
  };

  const counts = report
    ? { error: report.errors, warning: report.warnings, notice: report.notices }
    : null;

  return <div className="page audit-page">
    <div className="page-head edith-head">
      <div><h1>Site Audit</h1><div className="page-sub">Sitemap crawl and page metadata · every run stays on this computer</div></div>
    </div>

    <section className="card">
      <div className="backup-path">
        <input
          className="button"
          value={site}
          placeholder="example.com, or a link to a sitemap"
          aria-label="Site to audit"
          onChange={(event) => setSite(event.target.value)}
          onKeyDown={(event) => { if (event.key === "Enter") void run(); }}
        />
        <select value={limit} onChange={(event) => setLimit(Number(event.target.value))} aria-label="Page limit">
          {[10, 25, 50, 100, 250].map((value) => <option key={value} value={value}>{value} pages</option>)}
        </select>
        <button className="button primary" disabled={running || !site.trim()} onClick={() => void run()}>
          {running ? "Crawling…" : "Audit"}
        </button>
      </div>
      <p className="card-note">
        Veronica fetches the pages being audited and nothing else — four at a time, so an audit
        does not read as a load test against someone else&apos;s server.
      </p>
    </section>

    {error && <div className="banner error">{error}</div>}

    {report && <>
      <div className="metric-grid">
        {SEVERITIES.map((entry) => (
          <div className="metric-card" key={entry.id}>
            <span>{entry.label}</span>
            <strong>{counts?.[entry.id] ?? 0}</strong>
          </div>
        ))}
        <div className="metric-card"><span>Pages</span><strong>{report.pages.length}</strong></div>
      </div>

      {report.pages.length > 0 && report.errors + report.warnings + report.notices === 0
        ? <div className="empty"><div className="empty-glyph">✓</div><h3>Nothing to fix</h3><p>Every page crawled passes all eleven checks.</p></div>
        : <>
          <section className="card">
            <div className="card-head"><h2>Across the site</h2><span className="card-note">one page is a page&apos;s problem; forty is the template&apos;s</span></div>
            <div className="list">
              {report.byCode.map(([code, severity, count]) => (
                <div className="list-row" key={code}>
                  <div><strong>{code}</strong></div>
                  <span className={`pill ${SEVERITIES.find((entry) => entry.id === severity)?.pill ?? ""}`}>
                    {count} page{count === 1 ? "" : "s"}
                  </span>
                </div>
              ))}
            </div>
          </section>

          <section className="card">
            <div className="card-head"><h2>Pages</h2><span className="card-note">worst first</span></div>
            <div className="list">
              {[...report.pages]
                .sort((a, b) => b.issues.length - a.issues.length)
                .map((page) => (
                  <div key={page.url}>
                    <button className="list-row audit-page-row" onClick={() => setOpen(open === page.url ? null : page.url)}>
                      <div>
                        <strong>{page.url}</strong>
                        <small>{page.error ?? `HTTP ${page.statusCode ?? "—"} · ${page.responseMillis ?? 0} ms · ${page.metadata.wordCount} words`}</small>
                      </div>
                      <span className={`pill ${page.issues.some((issue) => issue.severity === "error") ? "critical" : page.issues.length ? "warn" : "good"}`}>
                        {page.issues.length === 0 ? "Clean" : `${page.issues.length} issue${page.issues.length === 1 ? "" : "s"}`}
                      </span>
                    </button>
                    {open === page.url && <div className="audit-detail">
                      {page.issues.map((issue) => (
                        <div className="audit-issue" key={issue.code}>
                          <span className={`pill ${SEVERITIES.find((entry) => entry.id === issue.severity)?.pill ?? ""}`}>{issue.severity}</span>
                          <div><strong>{issue.title}</strong><small>{issue.detail}</small></div>
                        </div>
                      ))}
                      <dl className="hardware-list">
                        <div><dt>Title</dt><dd>{page.metadata.title ?? "—"}</dd></div>
                        <div><dt>Description</dt><dd>{page.metadata.description ?? "—"}</dd></div>
                        <div><dt>Canonical</dt><dd>{page.metadata.canonicalUrl ?? "—"}</dd></div>
                        <div><dt>Language</dt><dd>{page.metadata.language ?? "—"}</dd></div>
                        <div><dt>H1</dt><dd>{page.metadata.heading ?? "—"}</dd></div>
                        <div><dt>og:image</dt><dd>{page.metadata.openGraphImageUrl ?? "—"}</dd></div>
                      </dl>
                    </div>}
                  </div>
                ))}
            </div>
          </section>
        </>}
    </>}
  </div>;
}
