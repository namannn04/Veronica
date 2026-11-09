import { expect, test } from "bun:test";
import { endpointLabel, matchesContainer, matchesPort } from "../src/lib/services";
import type { DockerContainer, ListeningPort } from "../src/lib/types";
const port: ListeningPort = { address: "::1", port: 3000, protocol: "TCP", inode: "123", owners: [{ name: "node", pid: 456, uid: 1000, startedAt: "789", canStop: true }] };
test("numeric searches match an exact port and names ignore case", () => {
  expect(matchesPort(port, "3000")).toBe(true);
  expect(matchesPort(port, "300")).toBe(false);
  expect(matchesPort(port, "NODE")).toBe(true);
  expect(matchesPort(port, "")).toBe(true);
  expect(endpointLabel(port)).toBe("[::1]:3000");
});
const container: DockerContainer = { id: "abc", name: "web", image: "nginx", state: "running", status: "Up", ports: "127.0.0.1:3000->80/tcp, [::]:4000-4005->90-95/tcp, 443/tcp" };
test("container port search handles IPv6 and ranges without matching IP octets", () => {
  for (const query of ["3000", "80", "4003", "92", "443", "NGINX"]) expect(matchesContainer(container, query)).toBe(true);
  for (const query of ["300", "127", "4006", "postgres"]) expect(matchesContainer(container, query)).toBe(false);
});
