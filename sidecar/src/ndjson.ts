/**
 * Newline-delimited JSON framing, split on LF only.
 *
 * This is the one framing rule the sidecar cannot get wrong: Node's `readline` also splits on
 * U+2028/U+2029, which are legal unescaped inside JSON strings, so a `readline`-based reader tears a
 * valid record in half and turns a prompt into a parse error. pi documents LF-only framing for its
 * own RPC mode, and Grove Sidecar Protocol v1 uses the same rule on both sides of the process
 * boundary. A record may end with CRLF; the CR is dropped.
 */

export interface LineSplitter {
  /** Feed one decoded chunk. Complete lines are emitted immediately. */
  push(chunk: string): void;
  /** No more input: emit a final unterminated line, if it carries anything. */
  end(): void;
}

export function createLineSplitter(onLine: (line: string) => void): LineSplitter {
  let buffer = "";

  const flush = (line: string) => {
    const record = line.endsWith("\r") ? line.slice(0, -1) : line;
    if (record.trim().length > 0) onLine(record);
  };

  return {
    push(chunk) {
      buffer += chunk;
      let index = buffer.indexOf("\n");
      while (index >= 0) {
        flush(buffer.slice(0, index));
        buffer = buffer.slice(index + 1);
        index = buffer.indexOf("\n");
      }
    },
    end() {
      const tail = buffer;
      buffer = "";
      if (tail.length > 0) flush(tail);
    },
  };
}
