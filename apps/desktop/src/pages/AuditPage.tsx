import { ChevronDown, ChevronRight, CircleCheck, Globe, ScanSearch } from "lucide-react";
import { Fragment, useState, useSyncExternalStore } from "react";

import { ipc } from "../lib/ipc";
import type { AuditPage as AuditedPage, AuditReport, AuditSeverity } from "../lib/types";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  KeyValueList,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
  ProgressBar,
  Section,
  Select,
  Stat,
  StatGrid,
  TextField,
  type Tone,
} from "../ui";
import "./AuditPage.css";

const SEVERITIES: { id: AuditSeverity; label: string; singular: string; tone: Tone }[] = [
  { id: "error", label: "Errors", singular: "Error", tone: "critical" },
  { id: "warning", label: "Warnings", singular: "Warning", tone: "warning" },
  { id: "notice", label: "Notices", singular: "Notice", tone: "neutral" },
];

const LIMITS = [10, 25, 50, 100, 250];

type AuditRun = {
  site: string;
  limit: number;
  report: AuditReport | null;
  running: boolean;
  error: string;
};
let current: AuditRun = { site: "", limit: 50, report: null, running: false, error: "" };
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
const update = (next: Partial<AuditRun>) => {
  current = { ...current, ...next };
  listeners.forEach((listener) => listener());
};

async function startAudit(site: string, limit: number) {
  update({ site, limit, running: true, error: "", report: null });
  try {
    update({ report: await ipc.auditSite(site, limit, 4) });
  } catch (reason) {
    update({ error: String(reason) });
  } finally {
    update({ running: false });
  }
}

export function AuditPage() {
  const {
    report,
    running,
    error,
    site: lastSite,
    limit: lastLimit,
  } = useSyncExternalStore(subscribe, () => current);
  const [site, setSite] = useState(lastSite);
  const [limit, setLimit] = useState(lastLimit);
  const [open, setOpen] = useState<string | null>(null);

  const run = async () => {
    if (!site.trim() || running) {
      return;
    }
    setOpen(null);
    await startAudit(site.trim(), limit);
  };

  return (
    <Page
      header={
        <PageHeader
          title="Site Audit"
          subtitle="Checks every page in a sitemap, from this computer"
        />
      }
    >
      <Card>
        <form
          className="audit-form"
          onSubmit={(event) => {
            event.preventDefault();
            void run();
          }}
        >
          <TextField
            className="audit-site"
            leading={<Globe aria-hidden="true" />}
            value={site}
            placeholder="example.com, or a link to a sitemap"
            aria-label="Site to audit"
            onChange={(event) => setSite(event.target.value)}
          />
          <Select
            value={limit}
            onChange={(event) => setLimit(Number(event.target.value))}
            aria-label="Page limit"
          >
            {LIMITS.map((value) => (
              <option key={value} value={value}>
                Up to {value} pages
              </option>
            ))}
          </Select>
          <Button type="submit" variant="primary" busy={running} disabled={!site.trim()}>
            Run audit
          </Button>
        </form>
        <p className="audit-hint">
          Veronica fetches only the pages being audited, four at a time, so an audit never reads as
          a load test against someone else's server.
        </p>
      </Card>

      {running && (
        <Card className="audit-progress" aria-live="polite">
          <div className="audit-progress-text">
            <span className="audit-progress-label">Crawling {lastSite}…</span>
            <span className="audit-progress-note">
              Reading the sitemap and up to {lastLimit} pages
            </span>
          </div>
          <ProgressBar value={null} label={`Crawling ${lastSite}`} />
        </Card>
      )}

      {error && !running && (
        <Notice
          tone="critical"
          title={`Could not audit ${lastSite}`}
          actions={
            <Button size="small" onClick={() => void startAudit(lastSite, lastLimit)}>
              Retry
            </Button>
          }
        >
          {error}
        </Notice>
      )}

      {!report && !running && !error && (
        <EmptyState icon={<ScanSearch />} title="No audit yet">
          Enter a site or a sitemap link above. Each page is checked for titles, descriptions,
          headings, canonical links and social previews.
        </EmptyState>
      )}

      {report && (
        <Results
          report={report}
          open={open}
          onToggle={(url) => setOpen(open === url ? null : url)}
        />
      )}
    </Page>
  );
}

function Results({
  report,
  open,
  onToggle,
}: {
  report: AuditReport;
  open: string | null;
  onToggle: (url: string) => void;
}) {
  const counts: Record<AuditSeverity, number> = {
    error: report.errors,
    warning: report.warnings,
    notice: report.notices,
  };
  const total = report.errors + report.warnings + report.notices;
  const titles = new Map(
    report.pages.flatMap((page) => page.issues.map((issue) => [issue.code, issue.title] as const)),
  );
  const pages = [...report.pages].sort((a, b) => b.issues.length - a.issues.length);

  return (
    <>
      <StatGrid min={140}>
        <Stat label="Pages crawled" value={report.pages.length} />
        {SEVERITIES.map((entry) => (
          <Stat
            key={entry.id}
            label={entry.label}
            value={counts[entry.id]}
            tone={counts[entry.id] > 0 && entry.tone !== "neutral" ? entry.tone : "neutral"}
          />
        ))}
      </StatGrid>

      {report.pages.length > 0 && total === 0 ? (
        <EmptyState icon={<CircleCheck />} title="Nothing to fix">
          Every page crawled passes all eleven checks.
        </EmptyState>
      ) : (
        SEVERITIES.map((severity) => {
          const issues = report.byCode.filter(([, level]) => level === severity.id);
          if (issues.length === 0) {
            return null;
          }
          return (
            <Section key={severity.id} title={severity.label}>
              <List label={severity.label}>
                {issues.map(([code, , count]) => (
                  <ListRow
                    key={code}
                    title={titles.get(code) ?? code}
                    subtitle={<span className="ui-mono">{code}</span>}
                    trailing={
                      <Badge tone={severity.tone}>
                        {count} page{count === 1 ? "" : "s"}
                      </Badge>
                    }
                  />
                ))}
              </List>
            </Section>
          );
        })
      )}

      {pages.length > 0 && (
        <Section
          title="Pages"
          description="Worst first. Open a page to see its issues and metadata."
        >
          <List label="Audited pages">
            {pages.map((page) => (
              <Fragment key={page.url}>
                <ListRow
                  leading={open === page.url ? <ChevronDown /> : <ChevronRight />}
                  title={page.url}
                  mono
                  subtitle={
                    page.error ??
                    `HTTP ${page.statusCode ?? "unknown"} · ${page.responseMillis ?? 0} ms · ${page.metadata.wordCount} words`
                  }
                  trailing={<PageBadge page={page} />}
                  selected={open === page.url}
                  onClick={() => onToggle(page.url)}
                />
                {open === page.url && <PageDetail page={page} />}
              </Fragment>
            ))}
          </List>
        </Section>
      )}
    </>
  );
}

function PageBadge({ page }: { page: AuditedPage }) {
  if (page.error) {
    return <Badge tone="critical">Failed</Badge>;
  }
  if (page.issues.length === 0) {
    return <Badge tone="good">Clean</Badge>;
  }
  const tone: Tone = page.issues.some((issue) => issue.severity === "error")
    ? "critical"
    : page.issues.some((issue) => issue.severity === "warning")
      ? "warning"
      : "neutral";
  return (
    <Badge tone={tone}>
      {page.issues.length} issue{page.issues.length === 1 ? "" : "s"}
    </Badge>
  );
}

function PageDetail({ page }: { page: AuditedPage }) {
  const none = "None";
  return (
    <div className="audit-detail">
      {page.issues.length > 0 && (
        <ul className="audit-issues">
          {page.issues.map((issue) => {
            const severity = SEVERITIES.find((entry) => entry.id === issue.severity);
            return (
              <li key={issue.code} className="audit-issue">
                <Badge tone={severity?.tone ?? "neutral"}>
                  {severity?.singular ?? issue.severity}
                </Badge>
                <div className="audit-issue-text">
                  <p className="audit-issue-title">{issue.title}</p>
                  <p className="audit-issue-detail">{issue.detail}</p>
                </div>
              </li>
            );
          })}
        </ul>
      )}
      <KeyValueList
        items={[
          { label: "Title", value: page.metadata.title ?? none },
          { label: "Description", value: page.metadata.description ?? none },
          {
            label: "Canonical",
            value: page.metadata.canonicalUrl ?? none,
            mono: Boolean(page.metadata.canonicalUrl),
          },
          { label: "Language", value: page.metadata.language ?? none },
          { label: "H1", value: page.metadata.heading ?? none },
          {
            label: "og:image",
            value: page.metadata.openGraphImageUrl ?? none,
            mono: Boolean(page.metadata.openGraphImageUrl),
          },
        ]}
      />
    </div>
  );
}
