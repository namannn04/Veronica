import {
  Badge,
  Button,
  Card,
  CardHeader,
  List,
  ListRow,
  ProgressBar,
  type Tone,
} from "@veronica/ui";
import { PartyPopper } from "lucide-react";
import { useEffect, useState } from "react";
import { useInstalledExtensions } from "../extensions/useInstalledExtensions";
import { invoke, listenEvent } from "../lib/ipc";
import { watchJobs } from "../lib/library";
import { type StarterState, starterStatus } from "../lib/onboarding";
import type { ExtensionJob } from "../lib/types";

const STATE_LABELS: Record<StarterState, { label: string; tone: Tone }> = {
  pending: { label: "Not installed", tone: "neutral" },
  installing: { label: "Installing", tone: "accent" },
  installed: { label: "Ready", tone: "good" },
  failed: { label: "Failed", tone: "critical" },
};

export function WelcomeCard({
  starters,
  onDismiss,
  onOpenLibrary,
}: {
  starters: string[];
  onDismiss: () => void;
  onOpenLibrary: () => void;
}) {
  const installed = useInstalledExtensions() ?? [];
  const [jobs, setJobs] = useState<Map<string, ExtensionJob>>(new Map());
  const [retrying, setRetrying] = useState<string | null>(null);

  useEffect(() => watchJobs(invoke, listenEvent, (next) => setJobs(next)), []);

  const retry = async (id: string) => {
    setRetrying(id);
    try {
      await invoke("ext_install", { id });
    } catch {
      return;
    } finally {
      setRetrying(null);
    }
  };

  const titleOf = (id: string) =>
    installed.find((row) => row.id === id)?.manifest?.title ??
    id.charAt(0).toUpperCase() + id.slice(1);
  const statuses = starters.map((id) => ({ id, ...starterStatus(id, installed, jobs.get(id)) }));
  const ready = statuses.filter((status) => status.state === "installed").length;

  return (
    <Card as="section" className="home-span-3 welcome-card" aria-label="Welcome">
      <CardHeader
        icon={<PartyPopper />}
        title="Welcome to Veronica"
        subtitle={`${ready} of ${starters.length} starter extensions ready`}
        actions={
          <>
            <Button size="small" onClick={onOpenLibrary}>
              Open Library
            </Button>
            <Button size="small" variant="plain" onClick={onDismiss}>
              Dismiss
            </Button>
          </>
        }
      />
      <List label="Starter extensions">
        {statuses.map((status) => (
          <ListRow
            key={status.id}
            title={titleOf(status.id)}
            subtitle={
              status.state === "installing" ? (
                <span className="welcome-progress">
                  <span>{status.message ?? "Waiting for another install"}</span>
                  <ProgressBar value={null} size="thin" label={`${titleOf(status.id)} progress`} />
                </span>
              ) : (
                (status.error ?? undefined)
              )
            }
            trailing={
              status.state === "failed" || status.state === "pending" ? (
                <Button
                  size="small"
                  busy={retrying === status.id}
                  aria-label={`${status.state === "failed" ? "Retry" : "Install"} ${titleOf(status.id)}`}
                  onClick={() => void retry(status.id)}
                >
                  {status.state === "failed" ? "Retry" : "Install"}
                </Button>
              ) : (
                <Badge tone={STATE_LABELS[status.state].tone}>
                  {STATE_LABELS[status.state].label}
                </Badge>
              )
            }
          />
        ))}
      </List>
    </Card>
  );
}
