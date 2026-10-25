import { describe, expect, it } from "vitest";
import { errorText, toCommandFailure, toIpcError, UNKNOWN_ERROR } from "./errors";

describe("command rejections", () => {
  it("keeps the wire code, message and detail of an ipc error", () => {
    expect(toCommandFailure({ code: "conflict", message: "plan changed", detail: "v3" })).toEqual({
      code: "conflict",
      message: "plan changed",
      detail: "v3",
    });
    expect(errorText({ code: "conflict", message: "plan changed" })).toBe("plan changed (conflict)");
    expect(toIpcError({ code: "not_found", message: "no session", detail: "id" })).toEqual({
      code: "not_found",
      message: "no session",
      detail: "id",
    });
  });

  it("reads a plain Error as a message with no code", () => {
    expect(toCommandFailure(new Error("bridge offline"))).toEqual({
      code: null,
      message: "bridge offline",
      detail: null,
    });
    expect(errorText(new Error("bridge offline"))).toBe("bridge offline");
  });

  it("never renders an unknown value as 'undefined'", () => {
    for (const value of [undefined, null, 42, {}, [], true]) {
      expect(errorText(value)).toBe(UNKNOWN_ERROR);
      expect(toCommandFailure(value).message).toBe(UNKNOWN_ERROR);
    }
    expect(errorText("   ")).toBe(UNKNOWN_ERROR);
    expect(errorText("plain string")).toBe("plain string");
  });

  it("ignores a code the contract does not define", () => {
    expect(toCommandFailure({ code: "exploded", message: "boom" }).code).toBeNull();
    expect(toIpcError({ code: "exploded", message: "boom" }).code).toBe("io");
  });
});
