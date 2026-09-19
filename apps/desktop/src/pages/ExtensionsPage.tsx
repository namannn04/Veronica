import { useEffect, useMemo, useState } from "react";

import { ipc } from "../lib/ipc";
import type {
  Diagnostics,
  ExtensionReport,
  ExtensionGroup,
  ToolReport,
  ToolSurvey,
} from "../lib/types";

const GROUPS: { id: ExtensionGroup | "all"; label: string }[] = [
  { id: "all", label: "All" },
  { id: "agent", label: "Agent" },
  { id: "system", label: "System" },
  { id: "media", label: "Media" },
  { id: "utilities", label: "Utilities" },
];

/** Human wording for a capability id, e.g. windowDimming -> Window dimming. */
function capabilityLabel(id: string): string {
  const spaced = id.replace(/([A-Z])/g, " $1").toLowerCase().trim();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

export function ExtensionsPage({
  diagnostics,
  onChanged,
}: {
  diagnostics: Diagnostics | null;
  onChanged: () => void;
}) {
  const [group, setGroup] = useState<ExtensionGroup | "all">("all");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [survey, setSurvey] = useState<ToolSurvey | null>(null);

  // Probing runs five processes, so it is its own request rather than part of
  // diagnostics, and its absence leaves the rest of the page usable: a page
  // that says nothing about tools is the state Veronica shipped with.
  useEffect(() => {
    void ipc.toolsReadiness().then(setSurvey).catch(() => setSurvey(null));
  }, []);

  const toolsById = useMemo(() => {
    const index = new Map<string, ToolReport>();
    for (const tool of survey?.tools ?? []) index.set(tool.id, tool);
    return index;
  }, [survey]);

  const shown = useMemo(() => {
    const entries = diagnostics?.extensions ?? [];
    const needle = query.trim().toLowerCase();
    return entries.filter((entry) => {
      const matchesGroup = group === "all" || entry.group === group;
      const matchesQuery =
        needle === "" ||
        entry.title.toLowerCase().includes(needle) ||
        entry.subtitle.toLowerCase().includes(needle);
      return matchesGroup && matchesQuery;
    });
  }, [diagnostics, group, query]);

  const toggle = async (entry: ExtensionReport) => {
    // The key comes from the catalogue rather than a copy here, so the two
    // cannot drift as extensions are added.
    setBusy(entry.id);
    try {
      await ipc.settingsSet(entry.defaultsKey, !entry.enabled);
      onChanged();
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Extensions</h1>
          <div className="page-sub">
            Every feature is in the app. Turning one off stops its timers and background work.
          </div>
        </div>
      </div>

      <div className="toolbar">
        <div className="segmented" role="group" aria-label="Category">
          {GROUPS.map((entry) => (
            <button
              key={entry.id}
              aria-pressed={group === entry.id}
              onClick={() => setGroup(entry.id)}
            >
              {entry.label}
            </button>
          ))}
        </div>
        <input
          className="button"
          style={{ minWidth: 180 }}
          placeholder="Search extensions"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          aria-label="Search extensions"
        />
      </div>

      <section className="card">
        {/* An empty list has three quite different causes, and blaming the
            search box for all of them is wrong twice: the catalogue arrives
            with diagnostics, which may not have answered yet or at all. */}
        {shown.length === 0 && (
          <p className="card-note">
            {diagnostics === null
              ? "Reading the extension catalogue…"
              : query.trim() || group !== "all"
                ? "Nothing here matches. Clear the search, or choose All."
                : "The extension catalogue is empty, which should not happen. Run `vr diagnose` to see what Veronica found."}
          </p>
        )}
        {shown.map((entry) => {
          const unavailable = entry.availability === "unavailable";
          const unmet = (survey?.unmet[entry.id] ?? [])
            .map((id) => toolsById.get(id))
            .filter((tool): tool is ToolReport => tool !== undefined);
          return (
            <div className="ext-row" key={entry.id}>
              <div className="ext-body">
                <div className="ext-title">
                  {entry.title}
                  <AvailabilityPill entry={entry} unmetTools={unmet.length > 0} />
                </div>
                <div className="ext-sub">{entry.subtitle}</div>
                {entry.missing && entry.missing.length > 0 && (
                  <div className="ext-sub" style={{ marginTop: 3 }}>
                    Needs {entry.missing.map(capabilityLabel).join(", ")}.
                    {" "}
                    {reasonFor(diagnostics, entry.missing[0])}
                  </div>
                )}
                {unmet.length > 0 && (
                  <div className="ext-sub" style={{ marginTop: 3 }}>
                    {/* "or" because Agent Usage takes either provider, and the
                        Rust rule has already decided which case this is: a
                        list of one reads the same either way. */}
                    Needs {unmet.map((tool) => tool.displayName).join(" or ")}.{" "}
                    {toolNote(unmet[0])}
                  </div>
                )}
              </div>
              <button
                className="switch"
                role="switch"
                aria-checked={entry.enabled}
                aria-label={`${entry.enabled ? "Disable" : "Enable"} ${entry.title}`}
                disabled={unavailable || busy === entry.id}
                onClick={() => void toggle(entry)}
              >
                <span className="knob" />
              </button>
            </div>
          );
        })}
      </section>
    </>
  );
}

/**
 * A missing tool is not the same as a missing capability.
 *
 * The session either supports something or it does not, and nothing the user
 * types changes it; a tool they can install in one command. So an extension
 * short of a tool reads as "Needs setup" rather than Unavailable, and its
 * switch stays usable: turning Herdr on before installing Herdr is a perfectly
 * reasonable order to do things in.
 */
function AvailabilityPill({
  entry,
  unmetTools,
}: {
  entry: ExtensionReport;
  unmetTools: boolean;
}) {
  if (entry.availability === "unavailable") {
    return <span className="pill critical">Unavailable</span>;
  }
  if (unmetTools) {
    return <span className="pill warn">Needs setup</span>;
  }
  if (entry.availability === "degraded") {
    return <span className="pill warn">Partial</span>;
  }
  return <span className="pill good">Ready</span>;
}

/** What to do about one unmet tool: the diagnosis, or the install line. */
function toolNote(tool: ToolReport): string {
  return tool.state === "error" ? tool.detail : `${tool.why} ${tool.instruction}`;
}

/** The reason text the capability resolver attached, when there is one. */
function reasonFor(diagnostics: Diagnostics | null, capability: string): string {
  const state = diagnostics?.capabilities.states[capability];
  if (!state) return "";
  return "reason" in state ? state.reason : "";
}
