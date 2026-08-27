import { useState } from "react";

import { bytes } from "../lib/format";
import { ipc } from "../lib/ipc";
import type { BackupSummary } from "../lib/types";

/** Explicit local backup: no silently chosen cloud account or upload. */
export function BackupPane() {
  const [path, setPath] = useState("");
  const [summary, setSummary] = useState<BackupSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

  const exportBackup = async () => {
    setBusy(true); setError(""); setNotice("");
    try {
      const result = await ipc.backupExport(null);
      setSummary(result); setPath(result.path);
      setNotice(`Backup exported to ${result.path}`);
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  };

  const inspect = async () => {
    if (!path.trim()) { setError("Enter the path to a .veronica-backup file."); return; }
    setBusy(true); setError(""); setNotice("");
    try { setSummary(await ipc.backupInspect(path.trim())); }
    catch (reason) { setSummary(null); setError(String(reason)); }
    finally { setBusy(false); }
  };

  const restore = async () => {
    if (!summary || summary.path !== path.trim()) {
      setError("Inspect this archive first, then restore it."); return;
    }
    if (!window.confirm(`Restore ${summary.files} files from Veronica ${summary.appVersion}?\n\nMatching settings and data are replaced. Other files remain untouched.`)) return;
    setBusy(true); setError(""); setNotice("");
    try {
      const report = await ipc.backupImport(path.trim(), true);
      setNotice(`Restored ${report.filesRestored} files (${bytes(report.bytesRestored)}). Settings are live now.`);
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  };

  return <div className="settings-stack backup-pane">
    {error && <div className="banner error">{error}</div>}
    {notice && !error && <div className="banner">{notice}</div>}
    <section className="settings-group"><div><h2>Export</h2><p>Settings, usage, machines, clipboard, swatches and notifier state. Cache and runtime files are excluded.</p></div><div className="settings-box"><div className="setting-row"><div><strong>Create a local backup</strong><small>Saved to Downloads with the current date and time. Nothing is uploaded.</small></div><button className="button primary" disabled={busy} onClick={() => void exportBackup()}>{busy ? "Working…" : "Export backup"}</button></div></div></section>
    <section className="settings-group"><div><h2>Inspect or restore</h2><p>Every path, byte count and SHA-256 digest is checked before any live file is replaced.</p></div><div className="settings-box">
      <div className="backup-path"><input className="button" value={path} onChange={(event) => { setPath(event.target.value); setSummary(null); }} placeholder="/home/you/Downloads/Veronica-backup.veronica-backup" aria-label="Backup archive path"/><button className="button" disabled={busy} onClick={() => void inspect()}>Inspect</button></div>
      {summary && <div className="backup-summary"><strong>Veronica {summary.appVersion}</strong><span>{summary.files} files · {bytes(summary.decodedBytes)}</span><span>{new Date(summary.createdAt).toLocaleString()}</span><button className="button" disabled={busy} onClick={() => void restore()}>Restore this backup</button></div>}
    </div></section>
    <p className="quiet">CLI equivalents: <code>vr backup export</code>, <code>vr backup inspect &lt;file&gt;</code>, and <code>vr backup import &lt;file&gt; --confirm</code>.</p>
  </div>;
}
