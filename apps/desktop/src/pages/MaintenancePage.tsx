import { useCallback, useEffect, useMemo, useState } from "react";

import { ipc } from "../lib/ipc";
import { bytes } from "../lib/format";
import type { PackageInstalled, PackageSource, PackageUpdate, RemovalPlan } from "../lib/types";

/**
 * App Maintenance.
 *
 * Edith unifies Homebrew, the Mac App Store and Sparkle into one update
 * inventory; on Ubuntu the three sources are apt, snap and flatpak. The rules
 * carry over: discovery installs nothing, every write is confirmed first, and a
 * removal shows what else goes with it before anything is removed.
 *
 * Nothing here is wrapped in sudo. An apt or snap change runs through pkexec,
 * so the desktop's own dialog asks the user to authenticate — which is why the
 * button says so rather than appearing to act silently.
 */
type Tab = "updates" | "installed";

export function MaintenancePage() {
  const [tab, setTab] = useState<Tab>("updates");
  const [sources, setSources] = useState<PackageSource[]>([]);
  const [updates, setUpdates] = useState<PackageUpdate[] | null>(null);
  const [installed, setInstalled] = useState<PackageInstalled[] | null>(null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState("");
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [plan, setPlan] = useState<RemovalPlan | null>(null);

  const loadUpdates = useCallback(async () => {
    setBusy("Looking for updates…"); setError(""); setNotice("");
    try { setUpdates(await ipc.packagesUpdates()); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(""); }
  }, []);

  useEffect(() => {
    void ipc.packagesSources().then(setSources).catch((reason) => setError(String(reason)));
    void loadUpdates();
  }, [loadUpdates]);

  useEffect(() => {
    if (tab !== "installed" || installed !== null) return;
    setBusy("Reading what is installed…");
    void ipc
      .packagesInventory()
      .then(setInstalled)
      .catch((reason) => setError(String(reason)))
      .finally(() => setBusy(""));
  }, [tab, installed]);

  // Grouped by source, because the three update by different commands and a
  // single "update everything" button would hide that.
  const bySource = useMemo(() => {
    const groups = new Map<string, PackageUpdate[]>();
    for (const update of updates ?? []) {
      const list = groups.get(update.source) ?? [];
      list.push(update);
      groups.set(update.source, list);
    }
    return groups;
  }, [updates]);

  const update = async (source: string) => {
    const count = bySource.get(source)?.length ?? 0;
    const needsRoot = sources.find((entry) => entry.source === source)?.needsRoot ?? true;
    if (!window.confirm(
      `Update ${count} ${source} package${count === 1 ? "" : "s"}?\n\n` +
      (needsRoot
        ? "Your desktop will ask you to authenticate. Veronica runs the change through pkexec and never uses sudo on your behalf."
        : "This is a user-scope flatpak update and needs no authentication.")
    )) return;

    setBusy(`Updating ${source}…`); setError(""); setNotice("");
    try {
      const result = await ipc.packagesUpdate(source, []);
      setNotice(result.succeeded ? `${source} is up to date.` : `${source} update did not finish.`);
      if (!result.succeeded) setError(result.output);
      await loadUpdates();
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
  const visibleInstalled = (installed ?? []).filter((entry) =>
    !needle || entry.name.toLowerCase().includes(needle));

  return <div className="page maintenance-page">
    <div className="page-head edith-head">
      <div><h1>App Maintenance</h1><div className="page-sub">Packages and updates across apt, snap and flatpak</div></div>
      <div className="head-actions">
        <button className="button" disabled={Boolean(busy)} onClick={() => void loadUpdates()}>Check for updates</button>
      </div>
    </div>

    <div className="pill-tabs">
      <button className={tab === "updates" ? "active" : ""} onClick={() => setTab("updates")}>Updates</button>
      <button className={tab === "installed" ? "active" : ""} onClick={() => setTab("installed")}>Installed</button>
    </div>

    {error && <div className="banner error">{error}</div>}
    {notice && <div className="banner">{notice}</div>}
    {busy && <div className="banner">{busy}</div>}

    <div className="source-strip">
      {sources.map((entry) => (
        <span key={entry.source} className={`pill ${entry.available ? "good" : ""}`}>
          {entry.source}{entry.available ? "" : " · not installed"}
        </span>
      ))}
      <span className="card-note">Checking never installs anything.</span>
    </div>

    {tab === "updates" && (updates === null
      ? null
      : updates.length === 0
        ? <div className="empty"><div className="empty-glyph">✓</div><h3>Everything is up to date</h3><p>Nothing on apt, snap or flatpak has a newer version.</p></div>
        : [...bySource.entries()].map(([source, rows]) => (
          <section className="card" key={source}>
            <div className="card-head">
              <h2>{source}</h2>
              <button className="button primary" disabled={Boolean(busy)} onClick={() => void update(source)}>
                Update {rows.length}
              </button>
            </div>
            <div className="list package-list">
              {rows.map((row) => (
                <div className="list-row" key={`${row.source}-${row.name}`}>
                  <div><strong>{row.name}</strong><small>{row.origin ?? ""}</small></div>
                  <span className="package-versions">
                    {row.installedVersion ? <><code>{row.installedVersion}</code> → </> : null}
                    <code className="new">{row.availableVersion}</code>
                  </span>
                </div>
              ))}
            </div>
          </section>
        )))}

    {tab === "installed" && <section className="card">
      <div className="card-head">
        <h2>Installed</h2>
        <input className="search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search packages" />
      </div>
      {visibleInstalled.length === 0
        ? <p className="quiet">{installed === null ? "Reading…" : "Nothing matches."}</p>
        : <div className="list package-list">
            {visibleInstalled.map((entry) => (
              <div className="list-row" key={`${entry.source}-${entry.name}`}>
                <div><strong>{entry.name}</strong><small>{entry.source} · {entry.version}</small></div>
                <span className="package-versions">
                  {entry.sizeBytes !== null && <code>{bytes(entry.sizeBytes)}</code>}
                  {entry.source === "apt" && <button className="button" disabled={Boolean(busy)} onClick={() => void review(entry.name)}>Review removal</button>}
                </span>
              </div>
            ))}
          </div>}
      <p className="card-note">
        apt shows what you asked for, not the dependencies pulled in with it. Removal is reviewed
        here and applied with <code>vr maintenance remove &lt;package&gt; --yes</code>.
      </p>
    </section>}

    {plan && <section className="card removal-plan">
      <div className="card-head"><h2>Removing {plan.package}</h2><button className="button" onClick={() => setPlan(null)}>Close</button></div>
      {plan.removesMoreThanAsked && <div className="banner warn">This takes more than the package you named.</div>}
      <div className="tile-label">Would be removed</div>
      <ul>{plan.removed.map((name) => <li key={name}>{name}</li>)}</ul>
      {plan.nowUnused.length > 0 && <>
        <div className="tile-label">Left installed but no longer needed</div>
        <ul>{plan.nowUnused.map((name) => <li key={name}>{name}</li>)}</ul>
      </>}
      <p className="card-note">Nothing has been removed. Run it yourself with <code>{plan.command}</code>, or <code>vr maintenance remove {plan.package} --yes</code>.</p>
    </section>}
  </div>;
}
