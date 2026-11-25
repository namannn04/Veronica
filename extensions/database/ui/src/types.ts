export interface DatabaseConnection {
  id: string;
  displayName: string;
  productHint: string;
  environment: { kind: string; label: string; protection: string };
  readOnlyPolicy: "disabled" | "preferred" | "required";
  productionPolicy: "standard" | "requireMutationPreview" | "prohibitMutations";
  location:
    | { kind: "network"; endpoints: { host: string; port: number; role: string }[] }
    | { kind: "sqlite"; sqlite: { path: string; accessMode: string } }
    | { kind: "memory"; name: string | null };
}

export interface DatabaseObject {
  kind: string;
  path: string[];
  nativeIdentifier: string | null;
}

export interface DatabaseValue {
  kind: string;
  value?: unknown;
}

export interface DatabasePage {
  columns: { name: string; typeName: string | null }[];
  rows: DatabaseValue[][];
  hasMore: boolean;
  nextOffset: number | null;
  elapsedMillis: number;
}

export interface DatabaseOperation {
  id: string;
  connectionId: string;
  connectionName: string;
  kind: string;
  summary: string;
  outcome: "succeeded" | "failed" | "abandoned";
  affectedRecords: number | null;
  elapsedMillis: number;
  error: string | null;
  at: string;
}
