import {
  Button,
  EmptyState,
  IconButton,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
} from "@veronica/ui";
import { useClient } from "@veronica/ui/frame";
import { FolderOpen, Trash } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

interface ShelfEntry {
  id: string;
  name: string;
  path: string;
}

export function ShelfPage() {
  const client = useClient();
  const [entries, setEntries] = useState<ShelfEntry[] | null>(null);
  const [error, setError] = useState("");

  const run = useCallback(
    async (method: "list" | "remove" | "clear", params: Record<string, unknown> = {}) => {
      try {
        const result = await client.backend.call<{ entries: ShelfEntry[] }>(method, params);
        setEntries(result.entries);
        setError("");
      } catch (reason) {
        setError(String(reason));
      }
    },
    [client],
  );

  useEffect(() => {
    void run("list");
  }, [run]);

  useEffect(
    () =>
      client.on("visibility.changed", ({ visible }) => {
        if (visible) {
          void run("list");
        }
      }),
    [client, run],
  );

  const clear = async () => {
    const confirmed = await client.ui.confirm({
      title: "Clear the shelf?",
      body: "Every copy on the shelf is deleted. The original files are not touched.",
      confirmLabel: "Clear",
      destructive: true,
    });
    if (confirmed) {
      await run("clear");
    }
  };

  return (
    <Page
      header={
        <PageHeader
          title="File Shelf"
          subtitle="Copies shared by the notch's Files tab and the drop shelf"
          actions={
            entries && entries.length > 0 ? (
              <Button variant="destructive-plain" onClick={() => void clear()}>
                Clear
              </Button>
            ) : undefined
          }
        />
      }
    >
      {error && <Notice tone="critical">{error}</Notice>}
      {entries?.length === 0 && (
        <EmptyState icon={<FolderOpen />} title="Nothing on the shelf">
          Drop files on the notch's Files tab, or run `vr shelf add`, to keep a copy here.
        </EmptyState>
      )}
      {entries && entries.length > 0 && (
        <List label="Files on the shelf">
          {entries.map((entry) => (
            <ListRow
              key={entry.id}
              title={entry.name}
              subtitle={<span className="ui-mono">{entry.path}</span>}
              trailing={
                <IconButton
                  label={`Remove ${entry.name}`}
                  icon={<Trash />}
                  onClick={() => void run("remove", { id: entry.id })}
                />
              }
            />
          ))}
        </List>
      )}
    </Page>
  );
}
