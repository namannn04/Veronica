import { frameClient } from "@veronica/ui/frame";
import type { DatabaseConnection, DatabaseObject, DatabaseOperation, DatabasePage } from "./types";

const SESSION_CALL_MS = 120_000;

function call<T>(method: string, params: unknown = {}, timeoutMs?: number): Promise<T> {
  return frameClient().backend.call<T>(method, params, timeoutMs ? { timeoutMs } : {});
}

export const ipc = {
  databaseConnections: () => call<DatabaseConnection[]>("connections"),
  databaseTest: (connection: string) => call<unknown>("test", { connection }, SESSION_CALL_MS),
  databaseBrowse: (connection: string, path: string[]) =>
    call<DatabaseObject[]>("browse", { connection, path }, SESSION_CALL_MS),
  databaseQuery: (connection: string, statement: string, limit: number, offset: number) =>
    call<DatabasePage>("query", { connection, statement, limit, offset }, SESSION_CALL_MS),
  databaseRead: (connection: string, path: string[], limit: number, offset: number) =>
    call<DatabasePage>("read", { connection, path, limit, offset }, SESSION_CALL_MS),
  databaseCapabilities: (connection: string) =>
    call<unknown>("capabilities", { connection }, SESSION_CALL_MS),
  databaseOperations: (limit: number) => call<DatabaseOperation[]>("operations", { limit }),
};

export function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
