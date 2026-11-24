import { useMemo, useState } from "react";
import { ArrowDown, ArrowUp } from "lucide-react";

import { Table } from "../ui";
import { money, modelLabel, tokens } from "../lib/format";
import type { ColorScale } from "./charts";
import type { NamedAmount } from "../lib/types";
import "./charts.css";
import "./ModelTable.css";

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
  { key: "cacheCreationTokens", label: "Cache write", numeric: true, render: (r) => tokens(r.cacheCreationTokens) },
  { key: "cacheReadTokens", label: "Cache read", numeric: true, render: (r) => tokens(r.cacheReadTokens) },
];

export function ModelTable({ rows, colorOf }: { rows: NamedAmount[]; colorOf: ColorScale }) {
  const [sortKey, setSortKey] = useState<Column["key"]>("cost");
  const [ascending, setAscending] = useState(false);

  const sorted = useMemo(() => {
    const copy = [...rows];
    copy.sort((a, b) => {
      if (sortKey === "name") return a.name.localeCompare(b.name);
      return (a[sortKey] as number) - (b[sortKey] as number);
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

  if (rows.length === 0) return <p className="models-empty">No models in this window.</p>;

  return (
    <Table label="Usage by model" className="models-table">
      <thead>
        <tr>
          {COLUMNS.map((column) => {
            const active = sortKey === column.key;
            return (
              <th key={column.key} className={column.numeric ? "num" : undefined} aria-sort={active ? (ascending ? "ascending" : "descending") : "none"}>
                <button type="button" className="models-sort" data-active={active || undefined} onClick={() => toggle(column.key)}>
                  {column.label}
                  <span className="models-sort-icon" aria-hidden="true">
                    {active ? ascending ? <ArrowUp /> : <ArrowDown /> : null}
                  </span>
                </button>
              </th>
            );
          })}
        </tr>
      </thead>
      <tbody>
        {sorted.map((row) => (
          <tr key={row.name}>
            {COLUMNS.map((column) => (
              <td key={column.key} className={column.numeric ? "num" : undefined}>
                {column.key === "name" ? (
                  <span className="models-name" title={row.name}>
                    <span className="chart-swatch" style={{ background: colorOf(row.name) }} aria-hidden="true" />
                    <span>{column.render(row)}</span>
                  </span>
                ) : (
                  column.render(row)
                )}
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </Table>
  );
}
