import { describe, expect, it } from "vitest";
import { fileKind } from "./FileTypeIcon";

describe("file type icons", () => {
  it("maps the languages the transcript sees most often", () => {
    expect(fileKind("apps/desktop/src/App.tsx")).toBe("react");
    expect(fileKind("src/widget.jsx")).toBe("react");
    expect(fileKind("crates/grove-core/src/lib.rs")).toBe("rust");
    expect(fileKind("src/index.ts")).toBe("typescript");
    expect(fileKind("src/index.mjs")).toBe("javascript");
    expect(fileKind("package.json")).toBe("json");
    expect(fileKind("docs/readme.md")).toBe("markdown");
    expect(fileKind("styles/design.scss")).toBe("css");
    expect(fileKind("index.html")).toBe("html");
    expect(fileKind("tools/script.py")).toBe("python");
    expect(fileKind("cmd/main.go")).toBe("go");
    expect(fileKind("scripts/setup.ps1")).toBe("shell");
    expect(fileKind("Cargo.toml")).toBe("config");
    expect(fileKind("Dockerfile")).toBe("config");
  });

  it("falls back to a generic file and ignores query strings", () => {
    expect(fileKind("LICENSE")).toBe("file");
    expect(fileKind("data.bin")).toBe("file");
    expect(fileKind("src/App.tsx?raw")).toBe("react");
    expect(fileKind("src/App.tsx#L10")).toBe("react");
  });
});
