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
/** Unwrap MCP's structured value for a readable result; keep raw protocol separately. */
export function methodResult(result: unknown): unknown {
  if (!result || typeof result !== "object") return result;
  const value = result as { structuredContent?: unknown; content?: { type?: string; text?: string }[]; isError?: boolean };
  if (value.isError) return result;
  let display: unknown = value.structuredContent;
  if (display && typeof display === "object" && "value" in display) display = (display as { value: unknown }).value;
  if (display === undefined && Array.isArray(value.content) && value.content.length === 1 && value.content[0].type === "text") display = value.content[0].text;
  if (display === undefined) return result;
  for (let i=0; i<2 && typeof display === "string"; i++) {
    try { display = JSON.parse(display); } catch { break; }
  }
  return display;
}
