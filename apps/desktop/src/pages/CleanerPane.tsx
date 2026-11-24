import { ScanSearch, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { bytes } from "../lib/format";
import { ipc } from "../lib/ipc";
import type { CleanerCategory, CleanerScan } from "../lib/types";
import {
  Button,
  Card,
  Checkbox,
  ConfirmDialog,
  EmptyState,
  Notice,
  Section,
  ListSkeleton,
  Toolbar,
} from "../ui";
import "./CleanerPane.css";

const STROKE = 1.75;

function items(count: number) {
  return `${count} item${count === 1 ? "" : "s"}`;
}

export function CleanerPane() {
  const [categories, setCategories] = useState<CleanerCategory[] | null>(null);
  const [categoriesError, setCategoriesError] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [scan, setScan] = useState<CleanerScan | null>(null);
  const [busy, setBusy] = useState<"" | "scan" | "clean">("");
  const [confirming, setConfirming] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

  const loadCategories = useCallback(() => {
    setCategoriesError("");
    void ipc
      .cleanerCategories()
      .then((all) => {
        const caches = all.filter((entry) => entry.family === "cache");
        setCategories(caches);
        setSelected(new Set(caches.filter((entry) => entry.onByDefault).map((entry) => entry.id)));
      })
      .catch((reason) => setCategoriesError(String(reason)));
  }, []);

  useEffect(loadCategories, [loadCategories]);

  const measure = useCallback(async (ids: Set<string>) => {
    setBusy("scan");
    setError("");
    setNotice("");
    try {
      setScan(await ipc.cleanerScan([...ids]));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy("");
    }
  }, []);

  const toggle = (id: string) => {
    const next = new Set(selected);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    setSelected(next);
    setScan(null);
  };

  const clean = async () => {
    if (!scan || scan.items.length === 0) {
      return;
    }
    setBusy("clean");
    setError("");
    try {
      const report = await ipc.cleanerClean(scan.items);
      setNotice(
        `Moved ${items(report.trashed.length)} to the Trash, ${bytes(report.bytesReclaimed)}. Empty the Trash to actually free the space.`,
      );
      if (report.failed.length > 0) {
        setError(report.failed.map(([path, why]) => `${path}: ${why}`).join("\n"));
      }
      setScan(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy("");
      setConfirming(false);
    }
  };

  const summary = scan
    ? scan.items.length === 0
      ? "Nothing to reclaim in the selected caches."
      : `${items(scan.items.length)}, ${bytes(scan.totalBytes)} reclaimable. Moving to the Trash does not free the space until you empty it.`
    : "Scanning only reads. Sweep project folders such as node_modules with vr cleaner scan --root <folder>.";

  return (
    <Section
      title="Developer caches"
      description="Cleaning moves what the scan found to the Trash, so it stays recoverable from Files."
    >
      {categoriesError ? (
        <Notice
          tone="critical"
          title="Cache categories could not be read"
          actions={
            <Button size="small" onClick={loadCategories}>
              Retry
            </Button>
          }
        >
          {categoriesError}
        </Notice>
      ) : (
        <Card padding="none">
          {categories === null ? (
            <ListSkeleton rows={4} inset={false} leading={0} meta={64} trailing={72} label="Reading cache categories" />
          ) : categories.length === 0 ? (
            <EmptyState size="compact" title="No caches to clean">
              This system reports no developer caches Veronica knows how to sweep.
            </EmptyState>
          ) : (
            <ul className="clean-list" aria-label="Cache categories">
              {categories.map((entry) => (
                <li
                  className="clean-row"
                  key={entry.id}
                  data-checked={selected.has(entry.id) || undefined}
                >
                  <Checkbox
                    checked={selected.has(entry.id)}
                    onChange={() => toggle(entry.id)}
                    label={entry.title}
                    description={entry.cost}
                  />
                  {scan?.totals[entry.id] !== undefined && (
                    <span className="clean-size ui-num">{bytes(scan.totals[entry.id])}</span>
                  )}
                </li>
              ))}
            </ul>
          )}
          {categories !== null && categories.length > 0 && (
            <div className="clean-footer">
              <Toolbar
                leading={<span className="clean-summary">{summary}</span>}
                trailing={
                  <>
                    <Button
                      icon={<ScanSearch strokeWidth={STROKE} />}
                      busy={busy === "scan"}
                      disabled={busy !== "" || selected.size === 0}
                      onClick={() => void measure(selected)}
                    >
                      {busy === "scan" ? "Scanning…" : scan ? "Scan again" : "Scan"}
                    </Button>
                    <Button
                      variant="destructive"
                      icon={<Trash2 strokeWidth={STROKE} />}
                      disabled={busy !== "" || !scan || scan.items.length === 0}
                      onClick={() => setConfirming(true)}
                    >
                      Move to Trash
                    </Button>
                  </>
                }
              />
            </div>
          )}
        </Card>
      )}

      {error && (
        <Notice tone="critical" title="Some items could not be cleaned">
          <span className="clean-error">{error}</span>
        </Notice>
      )}
      {notice && <Notice tone="good">{notice}</Notice>}

      <ConfirmDialog
        open={confirming}
        onClose={() => {
          if (busy !== "clean") {
            setConfirming(false);
          }
        }}
        onConfirm={() => void clean()}
        title={scan ? `Move ${items(scan.items.length)} to the Trash?` : "Move to the Trash?"}
        message={`${scan ? bytes(scan.totalBytes) : "Nothing"} goes to the Trash. Nothing is deleted in place, so it stays recoverable from Files, but the Trash keeps occupying the disk until you empty it.`}
        confirmLabel="Move to Trash"
        destructive
        busy={busy === "clean"}
      />
    </Section>
  );
}
