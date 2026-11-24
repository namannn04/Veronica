import { useCallback, useEffect, useMemo, useState } from "react";
import { CheckCircle2, Download, Package, RefreshCw } from "lucide-react";

import { ipc } from "../lib/ipc";
import { bytes } from "../lib/format";
import type { PackageInstalled, PackageSource, PackageUpdate, RemovalPlan } from "../lib/types";
import {
  Badge,
  Button,
  ConfirmDialog,
  Dialog,
  EmptyState,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
  SearchField,
  Section,
  SkeletonRows,
  Spinner,
  Tabs,
  Toolbar,
} from "../ui";
import "./MaintenancePage.css";

const STROKE = 1.75;

type Tab = "updates" | "installed";
type Pending = { kind: "source"; source: string } | { kind: "all" };

function packages(count: number) {
  return `${count} package${count === 1 ? "" : "s"}`;
}

export function MaintenancePage() {
  const [tab, setTab] = useState<Tab>("updates");
  const [sources, setSources] = useState<PackageSource[]>([]);
  const [updates, setUpdates] = useState<PackageUpdate[] | null>(null);
  const [updatesError, setUpdatesError] = useState("");
  const [installed, setInstalled] = useState<PackageInstalled[] | null>(null);
  const [installedError, setInstalledError] = useState("");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState("");
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [plan, setPlan] = useState<RemovalPlan | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);

  const loadUpdates = useCallback(async () => {
    setBusy("Looking for updates…"); setError(""); setNotice(""); setUpdatesError("");
    try { setUpdates(await ipc.packagesUpdates()); }
    catch (reason) { setUpdatesError(String(reason)); }
    finally { setBusy(""); }
  }, []);

  useEffect(() => {
    void ipc.packagesSources().then(setSources).catch((reason) => setError(String(reason)));
    void loadUpdates();
  }, [loadUpdates]);

  const loadInstalled = useCallback(() => {
    setBusy("Reading what is installed…"); setInstalledError("");
    void ipc
      .packagesInventory()
      .then(setInstalled)
      .catch((reason) => setInstalledError(String(reason)))
      .finally(() => setBusy(""));
  }, []);

  useEffect(() => {
    if (tab !== "installed" || installed !== null || installedError) return;
    loadInstalled();
  }, [tab, installed, installedError, loadInstalled]);

  const bySource = useMemo(() => {
    const groups = new Map<string, PackageUpdate[]>();
    for (const update of updates ?? []) {
      const list = groups.get(update.source) ?? [];
      list.push(update);
      groups.set(update.source, list);
    }
    return groups;
  }, [updates]);

  const needsRoot = (source: string) => sources.find((entry) => entry.source === source)?.needsRoot ?? true;

  const run = async (targets: string[]) => {
    setPending(null);
    setError(""); setNotice("");
    const outcomes: string[] = [];
    const failures: string[] = [];
    try {
      const results: { source: string; succeeded: boolean; output: string }[] = [];
      for (const source of targets) {
        setBusy(`Updating ${source}…`);
        const result = await ipc.packagesUpdate(source, []);
        results.push({ source, succeeded: result.succeeded, output: result.output });
      }
      setBusy("Looking for updates…");
      const remaining = await ipc.packagesUpdates().then((next) => { setUpdates(next); return next; }).catch(() => null);
      for (const { source, succeeded, output } of results) {
        const left = remaining?.filter((entry) => entry.source === source).length ?? 0;
        if (!succeeded) {
          outcomes.push(`${source} update did not finish.`);
          failures.push(output);
        } else if (left > 0) {
          outcomes.push(`${source} updated, but ${left} package${left === 1 ? " was" : "s were"} held back and still need${left === 1 ? "s" : ""} updating.`);
        } else {
          outcomes.push(`${source} is up to date.`);
        }
      }
      setNotice(outcomes.join(" "));
      if (failures.length > 0) setError(failures.join("\n\n"));
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(""); }
  };

  const review = async (name: string) => {
    setBusy(`Checking what removing ${name} would do…`); setError(""); setPlan(null);
    try { setPlan(await ipc.packagesRemovalPlan(name)); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(""); }
  };

  const needle = query.trim().toLowerCase();
  const visibleInstalled = (installed ?? []).filter((entry) => !needle || entry.name.toLowerCase().includes(needle));
  const total = updates?.length ?? 0;
  const groups = [...bySource.entries()];

  const pendingSources = pending?.kind === "all" ? groups.map(([source]) => source) : pending ? [pending.source] : [];
  const pendingCount = pendingSources.reduce((sum, source) => sum + (bySource.get(source)?.length ?? 0), 0);
  const pendingRoot = pendingSources.filter(needsRoot);
  const pendingMessage = pendingRoot.length === 0
    ? "This is a user-scope flatpak update and needs no authentication."
    : pendingRoot.length === pendingSources.length && pendingSources.length === 1
      ? "Your desktop will ask you to authenticate. Veronica runs the change through pkexec and never uses sudo on your behalf."
      : `Your desktop will ask you to authenticate for ${pendingRoot.join(" and ")}. Veronica runs those changes through pkexec and never uses sudo on your behalf.`;

  return (
    <Page
      className="mnt-page"
      header={
        <PageHeader
          title="App Maintenance"
          subtitle={tab === "updates" ? "Updates across apt, snap and flatpak" : "What apt, snap and flatpak have installed"}
          actions={
            <>
              <Button icon={<RefreshCw strokeWidth={STROKE} />} disabled={Boolean(busy)} onClick={() => void loadUpdates()}>
                Check for updates
              </Button>
              {groups.length > 0 && (
                <Button variant="primary" icon={<Download strokeWidth={STROKE} />} disabled={Boolean(busy)} onClick={() => setPending({ kind: "all" })}>
                  Update all
                </Button>
              )}
            </>
          }
          accessory={
            <Tabs
              label="Maintenance views"
              value={tab}
              onChange={setTab}
              items={[
                { id: "updates", label: "Updates", badge: total > 0 ? total : undefined },
                { id: "installed", label: "Installed" },
              ]}
            />
          }
        />
      }
    >
      {busy && (updates !== null || tab === "installed") && (
        <div className="mnt-busy" role="status">
          <Spinner size={14} />
          <span>{busy}</span>
        </div>
      )}
      {error && (
        <Notice tone="critical" title={notice || "That did not work"} actions={<Button size="small" variant="plain" onClick={() => setError("")}>Dismiss</Button>}>
          <span className="mnt-output ui-mono">{error}</span>
        </Notice>
      )}
      {notice && !error && <Notice tone="good" compact actions={<Button size="small" variant="plain" onClick={() => setNotice("")}>Dismiss</Button>}>{notice}</Notice>}

      {tab === "updates" && (
        <>
          {sources.length > 0 && (
            <div className="mnt-sources" aria-label="Package sources">
              {sources.map((entry) => (
                <Badge key={entry.source} tone={entry.available ? "good" : "neutral"} dot={entry.available}>
                  {entry.source}{entry.available ? "" : " not installed"}
                </Badge>
              ))}
              <span className="mnt-sources-note">Checking never installs anything.</span>
            </div>
          )}

          {updatesError ? (
            <Notice tone="critical" title="Updates could not be checked" actions={<Button size="small" disabled={Boolean(busy)} onClick={() => void loadUpdates()}>Retry</Button>}>
              {updatesError}
            </Notice>
          ) : updates === null ? (
            <List label="Updates"><div className="mnt-pad"><SkeletonRows rows={5} label="Looking for updates" /></div></List>
          ) : updates.length === 0 ? (
            <List label="Updates">
              <EmptyState
                icon={<CheckCircle2 strokeWidth={STROKE} />}
                title="Everything is up to date"
                action={<Button disabled={Boolean(busy)} onClick={() => void loadUpdates()}>Check again</Button>}
              >
                Nothing on apt, snap or flatpak has a newer version.
              </EmptyState>
            </List>
          ) : (
            groups.map(([source, rows]) => (
              <Section
                key={source}
                title={source}
                description={`${packages(rows.length)} · ${needsRoot(source) ? "asks you to authenticate" : "no authentication needed"}`}
                actions={
                  <Button size="small" disabled={Boolean(busy)} onClick={() => setPending({ kind: "source", source })}>
                    Update {rows.length}
                  </Button>
                }
              >
                <List label={`${source} updates`}>
                  {rows.map((row) => (
                    <ListRow
                      key={`${row.source}-${row.name}`}
                      className="mnt-update-row"
                      title={row.name}
                      subtitle={row.origin ?? undefined}
                      trailing={
                        <span className="mnt-versions ui-mono ui-selectable">
                          {row.installedVersion && (
                            <>
                              <span className="mnt-version-from" title={row.installedVersion}>{row.installedVersion}</span>
                              <span className="mnt-version-arrow" aria-label="to">→</span>
                            </>
                          )}
                          <span className="mnt-version-to" title={row.availableVersion}>{row.availableVersion}</span>
                        </span>
                      }
                    />
                  ))}
                </List>
              </Section>
            ))
          )}
        </>
      )}

      {tab === "installed" && (
        <Section>
          <Toolbar
            leading={<SearchField className="mnt-search" value={query} onChange={setQuery} placeholder="Search packages" />}
            trailing={installed && <span className="mnt-count ui-num">{visibleInstalled.length === installed.length ? packages(installed.length) : `${visibleInstalled.length} of ${installed.length}`}</span>}
          />
          {installedError ? (
            <Notice tone="critical" title="Installed packages could not be read" actions={<Button size="small" onClick={loadInstalled}>Retry</Button>}>
              {installedError}
            </Notice>
          ) : installed === null ? (
            <List label="Installed packages"><div className="mnt-pad"><SkeletonRows rows={6} label="Reading what is installed" /></div></List>
          ) : visibleInstalled.length === 0 ? (
            <List label="Installed packages">
              {needle
                ? <EmptyState size="compact" title={`Nothing matches “${query.trim()}”`} action={<Button size="small" onClick={() => setQuery("")}>Clear search</Button>}>Search by package name.</EmptyState>
                : <EmptyState size="compact" icon={<Package strokeWidth={STROKE} />} title="No packages reported">None of apt, snap or flatpak listed anything installed.</EmptyState>}
            </List>
          ) : (
            <List label="Installed packages">
              {visibleInstalled.map((entry) => (
                <ListRow
                  key={`${entry.source}-${entry.name}`}
                  title={entry.name}
                  subtitle={<>{entry.source} · <span className="ui-mono">{entry.version}</span></>}
                  meta={entry.sizeBytes !== null ? bytes(entry.sizeBytes) : undefined}
                  trailing={entry.source === "apt" ? (
                    <Button size="small" variant="plain" disabled={Boolean(busy)} onClick={() => void review(entry.name)}>Review removal</Button>
                  ) : undefined}
                />
              ))}
            </List>
          )}
          <p className="mnt-footnote">
            apt shows what you asked for, not the dependencies pulled in with it. Removal is reviewed here and applied with{" "}
            <span className="ui-mono">vr maintenance remove &lt;package&gt; --yes</span>.
          </p>
        </Section>
      )}

      <ConfirmDialog
        open={pending !== null}
        onClose={() => setPending(null)}
        onConfirm={() => void run(pendingSources)}
        title={pending?.kind === "all" ? `Update ${packages(pendingCount)}?` : `Update ${pendingCount} ${pendingSources[0] ?? ""} package${pendingCount === 1 ? "" : "s"}?`}
        message={pendingMessage}
        confirmLabel={pending?.kind === "all" ? "Update all" : "Update"}
      >
        {pending?.kind === "all" && pendingSources.length > 1 && (
          <p className="mnt-dialog-note">Sources update one after another: {pendingSources.map((source) => `${source} (${bySource.get(source)?.length ?? 0})`).join(", ")}.</p>
        )}
      </ConfirmDialog>

      <Dialog
        open={plan !== null}
        onClose={() => setPlan(null)}
        title={plan ? `Removing ${plan.package}` : "Removal"}
        description="Nothing has been removed. This is what the change would do."
        width={520}
        footer={<Button onClick={() => setPlan(null)}>Close</Button>}
      >
        {plan && (
          <div className="mnt-plan">
            {plan.removesMoreThanAsked && <Notice tone="warning" compact>This takes more than the package you named.</Notice>}
            <div className="mnt-plan-group">
              <span className="mnt-plan-label">Would be removed</span>
              <ul className="mnt-plan-list ui-mono">{plan.removed.map((name) => <li key={name}>{name}</li>)}</ul>
            </div>
            {plan.nowUnused.length > 0 && (
              <div className="mnt-plan-group">
                <span className="mnt-plan-label">Left installed but no longer needed</span>
                <ul className="mnt-plan-list ui-mono">{plan.nowUnused.map((name) => <li key={name}>{name}</li>)}</ul>
              </div>
            )}
            <div className="mnt-plan-group">
              <span className="mnt-plan-label">Run it yourself</span>
              <code className="mnt-command ui-mono ui-selectable">{plan.command}</code>
              <code className="mnt-command ui-mono ui-selectable">vr maintenance remove {plan.package} --yes</code>
            </div>
          </div>
        )}
      </Dialog>
    </Page>
  );
}
