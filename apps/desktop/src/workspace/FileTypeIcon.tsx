import type { ReactNode } from "react";
import { Icon } from "./Icon";

/**
 * A compact, monochrome file-type mark for the transcript's tool rows. The shape carries the
 * language (React atom, Rust cog, TS/JS monograms); the tint keeps them recognisable at 12–14px.
 * Unknown types fall back to the generic file glyph, so no extension is ever a broken row.
 */
export type FileKind =
  | "react"
  | "rust"
  | "typescript"
  | "javascript"
  | "json"
  | "markdown"
  | "css"
  | "html"
  | "python"
  | "go"
  | "shell"
  | "config"
  | "file";

export function fileKind(path: string): FileKind {
  const clean = path.split(/[?#]/)[0] ?? path;
  const name = clean.replace(/\\/g, "/").split("/").pop()?.toLowerCase() ?? "";
  if (/^(dockerfile|makefile)$/.test(name)) return "config";
  const dot = name.lastIndexOf(".");
  const extension = dot >= 0 ? name.slice(dot + 1) : "";
  switch (extension) {
    case "tsx":
    case "jsx":
      return "react";
    case "rs":
      return "rust";
    case "ts":
    case "mts":
    case "cts":
      return "typescript";
    case "js":
    case "mjs":
    case "cjs":
      return "javascript";
    case "json":
    case "jsonc":
      return "json";
    case "md":
    case "mdx":
      return "markdown";
    case "css":
    case "scss":
    case "sass":
    case "less":
      return "css";
    case "html":
    case "htm":
      return "html";
    case "py":
      return "python";
    case "go":
      return "go";
    case "sh":
    case "bash":
    case "zsh":
    case "ps1":
      return "shell";
    case "toml":
    case "yaml":
    case "yml":
    case "ini":
      return "config";
    default:
      return "file";
  }
}

const TINTS: Record<FileKind, string> = {
  react: "#61dafb",
  rust: "#dea584",
  typescript: "#6ea8ff",
  javascript: "#d9c04a",
  json: "#c9b458",
  markdown: "#8ab4f8",
  css: "#a78bfa",
  html: "#e07a5f",
  python: "#5b9bd5",
  go: "#38bdf8",
  shell: "#8ec07c",
  config: "#b08ce0",
  file: "var(--fg-3)",
};

export function FileTypeIcon({
  path,
  size = "sm",
}: {
  path: string;
  size?: "xs" | "sm";
}) {
  const kind = fileKind(path);
  if (kind === "file") return <Icon name="file" size={size} />;
  const dimension = size === "xs" ? 12 : 14;
  return (
    <svg
      className="file-type-icon"
      width={dimension}
      height={dimension}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.25"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      style={{ color: TINTS[kind] }}
    >
      {mark(kind)}
    </svg>
  );
}

function mark(kind: FileKind): ReactNode {
  switch (kind) {
    case "react":
      return (
        <>
          <ellipse cx="8" cy="8" rx="6" ry="2.4" />
          <ellipse cx="8" cy="8" rx="6" ry="2.4" transform="rotate(60 8 8)" />
          <ellipse cx="8" cy="8" rx="6" ry="2.4" transform="rotate(120 8 8)" />
          <circle cx="8" cy="8" r="1.1" fill="currentColor" stroke="none" />
        </>
      );
    case "rust":
      return (
        <>
          <circle cx="8" cy="8" r="2.6" />
          <path d="M8 1.6v1.8M8 12.6v1.8M1.6 8h1.8M12.6 8h1.8M3.4 3.4l1.3 1.3M11.3 11.3l1.3 1.3M12.6 3.4l-1.3 1.3M4.7 11.3l-1.3 1.3" />
        </>
      );
    case "typescript":
      return monogram("TS");
    case "javascript":
      return monogram("JS");
    case "json":
      return monogram("{}");
    case "markdown":
      return (
        <>
          <path d="M3 2.5h6.3L12.8 6v7.5H3z" />
          <path d="M9.3 2.5V6h3.5" />
          <path d="M5 9.2h4M5 11.3h3" />
        </>
      );
    case "css":
      return monogram("#");
    case "html":
      return <path d="M5.4 4.4 2.8 8l2.6 3.6M10.6 4.4 13.2 8l-2.6 3.6" />;
    case "python":
      return monogram("Py");
    case "go":
      return monogram("Go");
    case "shell":
      return <path d="M3 4.4 6 8l-3 3.6M8.2 11.6h4.4" />;
    case "config":
      return <path d="M4 5 2.5 8 4 11M12 5l1.5 3L12 11M8 3.5v9" />;
    case "file":
      return <path d="M4 2.5h5.3L12.5 5.8v7.7H4z" />;
  }
}

function monogram(text: string): ReactNode {
  return (
    <text
      x="8"
      y="11.3"
      textAnchor="middle"
      fontSize={text.length > 1 ? 6.2 : 9}
      fontWeight="700"
      fontFamily="ui-monospace, SFMono-Regular, Menlo, monospace"
      fill="currentColor"
      stroke="none"
    >
      {text}
    </text>
  );
}
