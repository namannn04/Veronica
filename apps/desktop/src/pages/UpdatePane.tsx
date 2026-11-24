import { Download, RefreshCw, RotateCcw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { ipc } from "../lib/ipc";
import type { UpdateInfo } from "../lib/types";
import { Badge, Button, FormGroup, FormRow, Notice, Stack } from "../ui";

const INSTALL_COMMAND = "curl -fsSL https://namannn04.github.io/veronica/install | bash";

export function UpdatePane({ current }: { current: string }) {
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [installed, setInstalled] = useState(false);
  const [error, setError] = useState("");

  const check = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      setUpdate(await ipc.updateCheck());
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void check();
  }, [check]);

  const install = async () => {
    if (!update) {
      return;
    }
    setInstalling(true);
    setError("");
    try {
      await ipc.updateInstall(update.latestVersion);
      setInstalled(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setInstalling(false);
    }
  };

  const available = Boolean(update?.updateAvailable) && !installed;

  const status = installed
    ? `Version ${update?.latestVersion} is installed. Restart Veronica to use it.`
    : update
      ? update.updateAvailable
        ? `Version ${update.latestVersion} is available.`
        : "You are up to date."
      : busy
        ? "Checking releases…"
        : "Check whenever you like. Nothing runs in the background.";

  const how =
    update?.installKind === "flatpak"
      ? "Updates through Flatpak."
      : update?.installKind === "snap"
        ? "Updates through Snap; snapd also refreshes it on its own."
        : update?.installKind === "appimage"
          ? "An AppImage is replaced by downloading the new file."
          : "Downloads the release and verifies its checksum, then Ubuntu asks for your password before installing.";

  const action = installed ? (
    <Button
      variant="primary"
      icon={<RotateCcw />}
      onClick={() => void ipc.appRestart().catch((reason) => setError(String(reason)))}
    >
      Restart Veronica
    </Button>
  ) : update?.updateAvailable && update.canInstall ? (
    <Button variant="primary" icon={<Download />} busy={installing} onClick={() => void install()}>
      {installing ? "Installing…" : `Update to ${update.latestVersion}`}
    </Button>
  ) : update?.updateAvailable ? (
    <Button
      variant="primary"
      icon={<Download />}
      onClick={() => void ipc.openExternal(update.packageUrl ?? update.releaseUrl)}
    >
      Download update
    </Button>
  ) : (
    <Button icon={<RefreshCw />} busy={busy} onClick={() => void check()}>
      {busy ? "Checking…" : "Check for updates"}
    </Button>
  );

  return (
    <Stack>
      {error && (
        <Notice
          tone="critical"
          title="The update check did not finish"
          actions={
            <Button size="small" onClick={() => void check()}>
              Retry
            </Button>
          }
        >
          {error}
        </Notice>
      )}
      <FormGroup
        title="Software updates"
        description="Checks Veronica's official releases. Nothing installs until you click."
      >
        <FormRow
          label="Installed version"
          description={status}
          control={
            current ? <Badge tone={available ? "warning" : "good"}>v{current}</Badge> : undefined
          }
        />
        <FormRow
          label={
            installed
              ? "Finish updating"
              : available
                ? `Veronica ${update?.latestVersion}`
                : "Latest release"
          }
          description={
            available
              ? `${update?.publishedAt ? `Published ${new Date(update.publishedAt).toLocaleDateString()}. ` : ""}${how}`
              : installed
                ? "Your settings and history stay in your home directory."
                : update
                  ? `Latest is ${update.latestVersion}.`
                  : "Uses a 12 second timeout and reports network errors."
          }
          control={action}
        />
        {available && update?.notes && (
          <FormRow label="What's new" stacked>
            <p className="settings-notes ui-selectable">{update.notes}</p>
          </FormRow>
        )}
        {available && update?.installKind !== "flatpak" && update?.installKind !== "snap" && (
          <FormRow
            label="Or update from a terminal"
            description="Installs the latest release in one step."
            stacked
          >
            <pre className="settings-code ui-mono ui-selectable">{INSTALL_COMMAND}</pre>
          </FormRow>
        )}
      </FormGroup>
    </Stack>
  );
}
