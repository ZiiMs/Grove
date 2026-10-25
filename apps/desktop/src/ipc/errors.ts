import type { ErrorCode, IpcError } from "@grove/contract";

/**
 * One way to read a rejected command.
 *
 * Every command rejection crosses the bridge as the wire shape `{ code, message, detail? }` — not as
 * an `Error` — but a bridge can also throw a plain `Error` (a missing handler, a serialisation
 * failure) or something else entirely. Four hand-rolled copies of this logic disagreed: two dropped
 * the code, one rendered `"undefined"` for a `null` rejection, and only Git kept the code. This is the
 * one place that decides.
 */

/** The codes the contract allows; anything else is a code we cannot switch on. */
const ERROR_CODES: ReadonlySet<string> = new Set<ErrorCode>([
  "conflict",
  "not_found",
  "invalid",
  "unavailable",
  "io",
]);

/** Shown when a rejection carries no usable message at all. Never the string `"undefined"`. */
export const UNKNOWN_ERROR = "the command failed without a message";

/** A rejection, normalised: a code when the core supplied one, a message that is never empty. */
export interface CommandFailure {
  code: ErrorCode | null;
  message: string;
  detail: string | null;
}

function asCode(value: unknown): ErrorCode | null {
  return typeof value === "string" && ERROR_CODES.has(value) ? (value as ErrorCode) : null;
}

function asText(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const text = value.trim();
  return text.length > 0 ? text : null;
}

/**
 * Normalise anything a command can reject with: the wire `{ code, message, detail? }`, an `Error`, a
 * bare string, or a value with none of those (`null`, `undefined`, a number, a plain object).
 */
export function toCommandFailure(error: unknown): CommandFailure {
  if (typeof error === "string") {
    return { code: null, message: asText(error) ?? UNKNOWN_ERROR, detail: null };
  }
  if (error instanceof Error) {
    return { code: asCode((error as { code?: unknown }).code), message: asText(error.message) ?? UNKNOWN_ERROR, detail: null };
  }
  if (typeof error === "object" && error !== null) {
    const candidate = error as { code?: unknown; message?: unknown; detail?: unknown };
    const message =
      asText(candidate.message) ??
      // A JSON-RPC-ish rejection often carries the text in `error`; a bare `{}` carries nothing.
      asText((candidate as { error?: unknown }).error) ??
      UNKNOWN_ERROR;
    const detail = asText(candidate.detail);
    return { code: asCode(candidate.code), message, detail };
  }
  return { code: null, message: UNKNOWN_ERROR, detail: null };
}

/**
 * One line for a panel: the message, plus the wire code when the core sent one, so a `conflict` reads
 * differently from an `io` failure without every panel formatting it itself.
 */
export function errorText(error: unknown): string {
  const failure = toCommandFailure(error);
  return failure.code ? `${failure.message} (${failure.code})` : failure.message;
}

/**
 * The wire error a panel renders in its `{ code, message, detail }` shape. The code is `io` for a
 * rejection that did not carry one, which is what a transport failure is.
 */
export function toIpcError(error: unknown): IpcError {
  const failure = toCommandFailure(error);
  return {
    code: failure.code ?? "io",
    message: failure.message,
    ...(failure.detail ? { detail: failure.detail } : {}),
  };
}
