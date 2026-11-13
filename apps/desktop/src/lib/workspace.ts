export interface DoctorReport { project: string; root: string; checks: { name: string; status: string; detail: string }[] }
export interface GraftConnection { endpoint: string; token: string }
export interface GraftTool { name: string; description?: string; inputSchema?: unknown; outputSchema?: unknown }
export interface GraftDiscovery { server: { name?: string; version?: string }; tools: GraftTool[]; installCommands: unknown }
export function methodArguments(text: string): Record<string, unknown> {
  if (new TextEncoder().encode(text).length > 16384) throw new Error("Arguments exceed 16 KiB.");
  const value: unknown = JSON.parse(text);
  if (!value || Array.isArray(value) || typeof value !== "object") throw new Error("Arguments must be a JSON object.");
  return value as Record<string, unknown>;
}
