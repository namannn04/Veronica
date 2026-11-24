import { Archive, FileSearch, RotateCcw } from "lucide-react";
import { useState } from "react";

import { bytes } from "../lib/format";
import { ipc } from "../lib/ipc";
import type { BackupSummary } from "../lib/types";
import {
  Button,
  ConfirmDialog,
  FormGroup,
  FormRow,
  KeyValueList,
  Notice,
  Stack,
  TextField,
} from "../ui";

export function BackupPane() {
  const [path, setPath] = useState("");
  const [summary, setSummary] = useState<BackupSummary | null>(null);
  const [busy, setBusy] = useState<"export" | "inspect" | "restore" | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [exported, setExported] = useState("");
  const [restored, setRestored] = useState("");
  const [error, setError] = useState("");

  const begin = (step: "export" | "inspect" | "restore") => {
    setBusy(step);
    setError("");
    setExported("");
    setRestored("");
  };

  const exportBackup = async () => {
    begin("export");
    try {
      const result = await ipc.backupExport(null);
      setSummary(result);
      setPath(result.path);
      setExported(`Saved to ${result.path}`);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  const inspect = async () => {
    if (!path.trim()) {
      setError("Enter the path to a .veronica-backup file.");
      return;
    }
    begin("inspect");
    try {
      setSummary(await ipc.backupInspect(path.trim()));
    } catch (reason) {
      setSummary(null);
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  const askRestore = () => {
    if (!summary || summary.path !== path.trim()) {
      setError("Inspect this archive first, then restore it.");
      return;
    }
    setConfirming(true);
  };

  const restore = async () => {
    begin("restore");
    try {
      const report = await ipc.backupImport(path.trim(), true);
      setRestored(
        `Restored ${report.filesRestored} files (${bytes(report.bytesRestored)}). Settings are live now.`,
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
      setConfirming(false);
    }
  };

  const working = busy !== null;
  const inspected = summary !== null && summary.path === path.trim();

  return (
    <Stack>
      {error && <Notice tone="critical">{error}</Notice>}

      <FormGroup
        title="Export"
        description="Settings, usage, machines, clipboard, swatches and notifier state. Cache and runtime files are left out."
        footer={exported || undefined}
      >
        <FormRow
          label="Create a local backup"
          description="Saved to Downloads with the current date and time. Nothing is uploaded."
          control={
            <Button
              variant="primary"
              icon={<Archive />}
              busy={busy === "export"}
              disabled={working}
              onClick={() => void exportBackup()}
            >
              {busy === "export" ? "Exporting…" : "Export backup"}
            </Button>
          }
        />
      </FormGroup>

      <FormGroup
        title="Inspect or restore"
        description="Every path, byte count and SHA-256 digest is checked before any live file is replaced."
        footer={
          restored || (
            <>
              From a terminal: <code className="ui-mono">vr backup export</code>,{" "}
              <code className="ui-mono">vr backup inspect &lt;file&gt;</code> and{" "}
              <code className="ui-mono">vr backup import &lt;file&gt; --yes</code>.
            </>
          )
        }
      >
        <FormRow
          label="Backup archive"
          description="The full path to a .veronica-backup file."
          stacked
        >
          <div className="settings-inline">
            <TextField
              mono
              className="settings-grow"
              value={path}
              onChange={(event) => {
                setPath(event.target.value);
                setSummary(null);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  void inspect();
                }
              }}
              placeholder="/home/you/Downloads/Veronica-backup.veronica-backup"
              aria-label="Backup archive path"
            />
            <Button
              icon={<FileSearch />}
              busy={busy === "inspect"}
              disabled={working}
              onClick={() => void inspect()}
            >
              Inspect
            </Button>
          </div>
        </FormRow>
        {inspected && summary && (
          <FormRow
            label={`Veronica ${summary.appVersion} backup`}
            control={
              <Button size="small" icon={<RotateCcw />} disabled={working} onClick={askRestore}>
                Restore backup
              </Button>
            }
          >
            <KeyValueList
              items={[
                { label: "Files", value: `${summary.files} files, ${bytes(summary.decodedBytes)}` },
                { label: "Created", value: new Date(summary.createdAt).toLocaleString() },
                { label: "Path", value: summary.path, mono: true },
              ]}
            />
          </FormRow>
        )}
      </FormGroup>

      <ConfirmDialog
        open={confirming}
        onClose={() => setConfirming(false)}
        onConfirm={() => void restore()}
        busy={busy === "restore"}
        destructive
        title="Restore this backup?"
        message={
          summary
            ? `${summary.files} files from Veronica ${summary.appVersion}. Matching settings and data are replaced. Other files stay untouched.`
            : undefined
        }
        confirmLabel="Restore"
      />
    </Stack>
  );
}
