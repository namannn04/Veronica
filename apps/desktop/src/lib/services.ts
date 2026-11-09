import type { DockerContainer, ListeningPort } from "./types";

export function matchesPort(row: ListeningPort, query: string): boolean {
  const term = query.trim().toLowerCase();
  if (!term) return true;
  if (/^\d+$/.test(term)) return row.port === Number(term);
  return [row.address, row.protocol, ...row.owners.map((owner) => `${owner.name} ${owner.pid}`)]
    .some((value) => value.toLowerCase().includes(term));
}
export function matchesContainer(row: DockerContainer, query: string): boolean {
  const term = query.trim().toLowerCase();
  if (!term) return true;
  if (!/^\d+$/.test(term)) return [row.name, row.image, row.id, row.state, row.ports].some((value) => value.toLowerCase().includes(term));
  const port = Number(term);
  return row.ports.split(",").some((mapping) => {
    const [host, target] = mapping.trim().split("->");
    const ranges = target ? [host.slice(host.lastIndexOf(":") + 1), target.split("/")[0]] : [host.split("/")[0]];
    return ranges.some((range) => {
      if (!/^\d+(?:-\d+)?$/.test(range)) return false;
      const [start, end] = range.split("-").map(Number);
      return port >= start && port <= (end ?? start);
    });
  });
}
export function endpointLabel(row: ListeningPort): string {
  return `${row.address.includes(":") ? `[${row.address}]` : row.address}:${row.port}`;
}
