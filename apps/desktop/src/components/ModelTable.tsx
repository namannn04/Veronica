import { useMemo, useState } from "react";

import { money, modelLabel, tokens } from "../lib/format";
import type { ColorScale } from "./charts";
import type { NamedAmount } from "../lib/types";

type Column = {
  key: keyof NamedAmount | "name";
  label: string;
  numeric: boolean;
  render: (row: NamedAmount) => string;
};

const COLUMNS: Column[] = [
  { key: "name", label: "Model", numeric: false, render: (r) => modelLabel(r.name) },
  { key: "cost", label: "Cost", numeric: true, render: (r) => money(r.cost) },
  { key: "tokens", label: "Tokens", numeric: true, render: (r) => tokens(r.tokens) },
  { key: "inputTokens", label: "Input", numeric: true, render: (r) => tokens(r.inputTokens) },
  { key: "outputTokens", label: "Output", numeric: true, render: (r) => tokens(r.outputTokens) },
  {
    key: "cacheCreationTokens",
    label: "Cache write",
    numeric: true,
    render: (r) => tokens(r.cacheCreationTokens),
  },
  {
    key: "cacheReadTokens",
    label: "Cache read",
    numeric: true,
    render: (r) => tokens(r.cacheReadTokens),
  },
];

/**
 * The table view of the model breakdown.
 *
 * It doubles as the accessible alternative to the charts: every figure the
 * ranked bars encode as length is available here as a number, which is what
 * makes the light-mode series colours legitimate despite two of them sitting
 * below 3:1 against the light surface.
 */
export function ModelTable({
  rows,
  colorOf,
}: {
  rows: NamedAmount[];
  colorOf: ColorScale;
}) {
  const [sortKey, setSortKey] = useState<Column["key"]>("cost");
  const [ascending, setAscending] = useState(false);

  const sorted = useMemo(() => {
    const copy = [...rows];
    copy.sort((a, b) => {
      if (sortKey === "name") return a.name.localeCompare(b.name);
      const left = a[sortKey] as number;
      const right = b[sortKey] as number;
      return left - right;
    });
    return ascending ? copy : copy.reverse();
  }, [rows, sortKey, ascending]);

  const toggle = (key: Column["key"]) => {
    if (key === sortKey) {
      setAscending((value) => !value);
    } else {
      setSortKey(key);
      setAscending(key === "name");
    }
  };

  if (rows.length === 0) return <p className="card-note">No models in this window.</p>;

  return (
    <div style={{ overflowX: "auto" }}>
      <table>
        <thead>
          <tr>
            {COLUMNS.map((column) => (
              <th
                key={column.key}
                className={`sortable${column.numeric ? " num" : ""}`}
                onClick={() => toggle(column.key)}
                aria-sort={
                  sortKey === column.key ? (ascending ? "ascending" : "descending") : "none"
                }
              >
                {column.label}
                {sortKey === column.key ? (ascending ? " ↑" : " ↓") : ""}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sorted.map((row) => (
            <tr key={row.name}>
              {COLUMNS.map((column) => (
                <td key={column.key} className={column.numeric ? "num" : ""}>
                  {column.key === "name" ? (
                    <span className="cell-name">
                      <span
                        className="swatch"
                        style={{ background: colorOf(row.name) }}
                      />
                      <span className="truncate" title={row.name}>
                        {column.render(row)}
                      </span>
                    </span>
                  ) : (
                    column.render(row)
                  )}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
