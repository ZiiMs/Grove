import { describe, expect, it } from "vitest";
import { createLineSplitter } from "../src/ndjson";

function collect() {
  const lines: string[] = [];
  const splitter = createLineSplitter((line) => lines.push(line));
  return { lines, splitter };
}

describe("ndjson framing", () => {
  it("splits on LF only and tolerates a preceding CR", () => {
    const { lines, splitter } = collect();
    splitter.push('{"a":1}\r\n{"a":2}\n');
    expect(lines).toEqual(['{"a":1}', '{"a":2}']);
  });

  it("keeps U+2028 and U+2029 inside a record", () => {
    // Node's `readline` also splits on these, which silently truncates JSON strings. pi's RPC mode
    // documents LF-only framing for exactly this reason.
    const { lines, splitter } = collect();
    const record = JSON.stringify({ text: "one\u2028two\u2029three" });
    splitter.push(`${record}\n`);
    expect(lines).toHaveLength(1);
    expect(JSON.parse(lines[0]).text).toBe("one\u2028two\u2029three");
  });

  it("reassembles a record split across chunks", () => {
    const { lines, splitter } = collect();
    splitter.push('{"half":');
    expect(lines).toHaveLength(0);
    splitter.push('"done"}\n');
    expect(lines).toEqual(['{"half":"done"}']);
  });

  it("drops blank lines and does not emit an unterminated tail", () => {
    const { lines, splitter } = collect();
    splitter.push("\n   \n\t\n");
    splitter.push("no newline yet");
    expect(lines).toEqual([]);
  });

  it("flushes a final unterminated record on end", () => {
    const { lines, splitter } = collect();
    splitter.push("last");
    splitter.end();
    expect(lines).toEqual(["last"]);
  });
});
