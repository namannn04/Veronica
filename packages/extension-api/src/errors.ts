export const ERROR_CODES = [
  "E_PERMISSION",
  "E_UNAVAILABLE",
  "E_TIMEOUT",
  "E_INVALID",
  "E_BACKEND",
  "E_VERSION",
] as const;

export type ErrorCode = (typeof ERROR_CODES)[number];

export interface WireError {
  code: ErrorCode;
  message: string;
  data?: unknown;
}

export function isErrorCode(value: unknown): value is ErrorCode {
  return typeof value === "string" && (ERROR_CODES as readonly string[]).includes(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export class ExtensionError extends Error {
  readonly code: ErrorCode;
  readonly data: unknown;

  constructor(code: ErrorCode, message: string, data?: unknown) {
    super(message);
    this.name = "ExtensionError";
    this.code = code;
    this.data = data;
  }

  static from(value: unknown): ExtensionError {
    if (value instanceof ExtensionError) {
      return value;
    }
    if (isRecord(value) && isErrorCode(value.code)) {
      const message = typeof value.message === "string" ? value.message : value.code;
      return new ExtensionError(value.code, message, value.data);
    }
    if (value instanceof Error) {
      return new ExtensionError("E_UNAVAILABLE", value.message);
    }
    return new ExtensionError("E_UNAVAILABLE", String(value));
  }

  toWire(): WireError {
    return this.data === undefined
      ? { code: this.code, message: this.message }
      : { code: this.code, message: this.message, data: this.data };
  }
}

export function isExtensionError(value: unknown, code?: ErrorCode): value is ExtensionError {
  return value instanceof ExtensionError && (code === undefined || value.code === code);
}
