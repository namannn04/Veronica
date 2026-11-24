import type { ReactNode } from "react";

import { Card, CardHeader } from "./Card";
import { Skeleton } from "./Feedback";
import { List, ListRow } from "./List";
import { Stat } from "./Stat";
import { Table } from "./Toolbar";

import "./Skeletons.css";

const LINE_WIDTHS = ["62%", "48%", "56%", "40%", "52%", "44%", "58%", "36%"];

function widthAt(index: number, offset = 0) {
  return LINE_WIDTHS[(index + offset) % LINE_WIDTHS.length];
}

function indices(count: number) {
  return Array.from({ length: count }, (_, index) => index);
}

export function SkeletonText({
  width = "60%",
  className,
}: {
  width?: number | string;
  className?: string;
}) {
  return (
    <span
      className={["ui-skeleton-text", className].filter(Boolean).join(" ")}
      style={{ width }}
      aria-hidden="true"
    >
      {"​"}
    </span>
  );
}

export function StatSkeleton({
  note = true,
  meter = false,
}: {
  note?: boolean;
  meter?: boolean;
}) {
  return (
    <Stat
      className="ui-stat-skeleton"
      label={<SkeletonText width={72} />}
      value={<SkeletonText width={88} />}
      note={
        note || meter ? (
          <>
            {meter && <span className="ui-skeleton-meter" aria-hidden="true" />}
            {note && <SkeletonText width="56%" />}
          </>
        ) : undefined
      }
    />
  );
}

export function ListSkeleton({
  rows = 4,
  leading = 28,
  subtitle = true,
  meta = 0,
  trailing = 0,
  inset = true,
  label = "Loading",
  className,
}: {
  rows?: number;
  leading?: number;
  subtitle?: boolean;
  meta?: number;
  trailing?: number;
  inset?: boolean;
  label?: string;
  className?: string;
}) {
  return (
    <div className="ui-skeleton-block" role="status" aria-label={label}>
      <List inset={inset} className={className}>
        {indices(rows).map((index) => (
          <ListRow
            key={index}
            leading={
              leading > 0 ? (
                <Skeleton width={leading} height={leading} radius={leading > 20 ? 8 : 4} />
              ) : undefined
            }
            title={<SkeletonText width={widthAt(index)} />}
            subtitle={subtitle ? <SkeletonText width={widthAt(index, 3)} /> : undefined}
            meta={meta > 0 ? <SkeletonText width={meta} /> : undefined}
            trailing={
              trailing > 0 ? <Skeleton width={trailing} height={26} radius={6} /> : undefined
            }
          />
        ))}
      </List>
    </div>
  );
}

export function BarListSkeleton({ rows = 5, label = "Loading" }: { rows?: number; label?: string }) {
  return (
    <ul className="ui-bar-list" role="status" aria-label={label}>
      {indices(rows).map((index) => (
        <li className="ui-bar-row" key={index}>
          <span className="ui-bar-label">
            <SkeletonText width={widthAt(index)} />
          </span>
          <span className="ui-bar-track" aria-hidden="true" />
          <span className="ui-bar-value">
            <SkeletonText width={40} />
          </span>
        </li>
      ))}
    </ul>
  );
}

export function TableSkeleton({
  columns = 4,
  rows = 6,
  twoLine = false,
  label = "Loading",
  className,
}: {
  columns?: number;
  rows?: number;
  twoLine?: boolean;
  label?: string;
  className?: string;
}) {
  return (
    <div className="ui-skeleton-block" role="status" aria-label={label}>
      <Table className={className}>
        <thead>
          <tr>
            {indices(columns).map((column) => (
              <th key={column}>
                <SkeletonText width={48 + ((column * 17) % 32)} />
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {indices(rows).map((row) => (
            <tr key={row}>
              {indices(columns).map((column) => (
                <td key={column}>
                  {twoLine && column === 0 ? (
                    <span className="ui-skeleton-cell-stack">
                      <SkeletonText width={widthAt(row)} />
                      <SkeletonText width={widthAt(row, 3)} className="ui-skeleton-caption" />
                    </span>
                  ) : (
                    <SkeletonText width={widthAt(row, column * 2)} />
                  )}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </Table>
    </div>
  );
}

export function CardSkeleton({
  lines = 3,
  icon = false,
  subtitle = false,
  children,
  className,
  label = "Loading",
}: {
  lines?: number;
  icon?: boolean;
  subtitle?: boolean;
  children?: ReactNode;
  className?: string;
  label?: string;
}) {
  return (
    <Card className={className} role="status" aria-label={label}>
      <CardHeader
        icon={icon ? <Skeleton width={15} height={15} radius={4} /> : undefined}
        title={<SkeletonText width="38%" />}
        subtitle={subtitle ? <SkeletonText width="52%" /> : undefined}
      />
      {children ??
        (lines > 0 && (
          <div className="ui-skeleton-lines-block">
            {indices(lines).map((index) => (
              <SkeletonText key={index} width={widthAt(index, 1)} />
            ))}
          </div>
        ))}
    </Card>
  );
}
