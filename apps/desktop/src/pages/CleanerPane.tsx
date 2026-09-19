import { useCallback, useEffect, useState } from "react";

import { ipc } from "../lib/ipc";
import { bytes } from "../lib/format";
import type { CleanerCategory, CleanerScan } from "../lib/types";

/**
 * The disk cleaner.
 *
 * Two things are true of every clean and are repeated wherever it can be
 * started: it moves what the scan found to the Trash rather than deleting it,
 * and the Trash keeps occupying the disk until it is emptied. The paths sent to
 * the clean are the ones on screen, not a fresh scan, so nothing the user never
 * saw can be swept along with what they agreed to.
 *
 * Project directories — node_modules, target, .venv — are swept from a folder
 * you point at, which the CLI does with `vr cleaner scan --root`. This card
 * covers the fixed caches, where there is nothing to choose but the categories.
 */
export function CleanerPane() {
  const [categories, setCategories] = useState<CleanerCategory[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [scan, setScan] = useState<CleanerScan | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    void ipc
      .cleanerCategories()
      .then((all) => {
        const caches = all.filter((entry) => entry.family === "cache");
        setCategories(caches);
        setSelected(new Set(caches.filter((entry) => entry.onByDefault).map((entry) => entry.id)));
      })
      .catch((reason) => setError(String(reason)));
  }, []);

  const measure = useCallback(async (ids: Set<string>) => {
    setBusy(true); setError(""); setNotice("");
    try { setScan(await ipc.cleanerScan([...ids])); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }, []);

  const toggle = (id: string) => {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id); else next.add(id);
    setSelected(next);
    // A stale total under a changed set of ticks would be a lie, so it goes.
    setScan(null);
  };

  const clean = async () => {
    if (!scan || scan.items.length === 0) return;
    if (!window.confirm(
      `Move ${scan.items.length} item${scan.items.length === 1 ? "" : "s"} (${bytes(scan.totalBytes)}) to the Trash?\n\n` +
      "Nothing is deleted in place, so this is recoverable from Files — but the Trash keeps occupying the disk until you empty it."
    )) return;
    setBusy(true); setError("");
    try {
      const report = await ipc.cleanerClean(scan.items);
      setNotice(
        `Moved ${report.trashed.length} item${report.trashed.length === 1 ? "" : "s"} to the Trash, ${bytes(report.bytesReclaimed)}. ` +
        "Empty the Trash to actually free the space."
      );
      if (report.failed.length > 0) {
        setError(report.failed.map(([path, why]) => `${path}: ${why}`).join("\n"));
      }
      setScan(null);
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  };

  return <section className="card cleaner-card">
    <div className="card-head">
      <h2>Disk cleaner</h2>
      <span className="card-note">{scan ? `${bytes(scan.totalBytes)} reclaimable` : "developer caches"}</span>
    </div>

    <div className="cleaner-categories">
      {categories.map((entry) => (
        <label key={entry.id} className={selected.has(entry.id) ? "on" : ""} title={entry.cost}>
          <input type="checkbox" checked={selected.has(entry.id)} onChange={() => toggle(entry.id)} />
          <span><strong>{entry.title}</strong><small>{entry.cost}</small></span>
          {scan?.totals[entry.id] !== undefined && <code>{bytes(scan.totals[entry.id])}</code>}
        </label>
      ))}
    </div>

    {error && <div className="banner error">{error}</div>}
    {notice && <div className="banner">{notice}</div>}

    <div className="cleaner-actions">
      <button className="button" disabled={busy || selected.size === 0} onClick={() => void measure(selected)}>
        {busy ? "Working…" : "Scan"}
      </button>
      <button className="button danger" disabled={busy || !scan || scan.items.length === 0} onClick={() => void clean()}>
        Move to Trash
      </button>
      <span className="card-note">
        {scan
          ? scan.items.length === 0
            ? "Nothing to reclaim."
            : `${scan.items.length} item${scan.items.length === 1 ? "" : "s"} · moving to the Trash does not free the space until you empty it`
          : "Scanning reads only. Project folders such as node_modules are swept with vr cleaner --root."}
      </span>
    </div>
  </section>;
}
