import type { ReactNode } from "react";

/**
 * The one icon set: 16px strokes traced from the design export. Keeping them here means a tool card,
 * a tree row and a dialog button all draw the same glyph.
 */
const PATHS: Record<string, ReactNode> = {
  chevron: <path d="M4 6.5 8 10.5 12 6.5" />,
  chevronRight: <path d="M6.5 4 10.5 8 6.5 12" />,
  chevronLeft: <path d="M9.5 4 5.5 8 9.5 12" />,
  plus: <path d="M8 3.5v9M3.5 8h9" />,
  minimize: <path d="M3.5 8.5h9" />,
  maximize: <rect x="3.5" y="3.5" width="9" height="9" />,
  restore: (
    <>
      <path d="M5.4 5.4h7.2v7.2H5.4z" />
      <path d="M3.4 3.4h7.2v7.2" />
    </>
  ),
  x: <path d="M4.6 4.6l6.8 6.8M11.4 4.6l-6.8 6.8" />,
  terminal: (
    <>
      <rect x="2.4" y="3.4" width="11.2" height="9.2" rx="1.6" />
      <path d="M5.4 6.6 7.4 8.2 5.4 9.8M9.2 9.8h2" />
    </>
  ),
  file: (
    <>
      <path d="M4.2 2.6h4.3L12 5.9v7.5H4.2z" />
      <path d="M8.4 2.6v3.4H12" />
    </>
  ),
  edit: (
    <>
      <path d="M10.4 2.9 13.1 5.6 6.1 12.6 3 13.1l.5-3.1z" />
      <path d="M9.3 4.1l2.6 2.6" />
    </>
  ),
  checkCircle: (
    <>
      <circle cx="8" cy="8" r="5.6" />
      <path d="M5.7 8.1l1.6 1.6 3-3.2" />
    </>
  ),
  circle: <circle cx="8" cy="8" r="4.6" />,
  panelRight: (
    <>
      <rect x="2.5" y="3" width="11" height="10" rx="1.6" />
      <path d="M9.5 3v10" />
    </>
  ),
  filter: <path d="M2.5 3.5h11L9.6 8.3v4.2l-3.2-1.7V8.3z" />,
  sort: <path d="M4 3.5v9M2 10.5l2 2 2-2M9 4.5h5M9 8h3.6M9 11.5h2.2" />,
  list: <path d="M3 4.5h10M3 8h10M3 11.5h10" />,
  collapseAll: <path d="M4 6.5 8 2.5l4 4M4 9.5l4 4 4-4" />,
  pin: (
    <>
      <path d="M9.8 2.2l4 4M10.6 3l-5 5-.8 3.8 3.8-.8 5-5z" />
      <path d="M5.6 10.4 2.5 13.5" />
    </>
  ),
  shield: <path d="M8 2.2 12.8 3.9v3.6c0 2.9-2 4.9-4.8 5.9-2.8-1-4.8-3-4.8-5.9V3.9z" />,
  shieldCheck: (
    <>
      <path d="M8 2.2 12.8 3.9v3.6c0 2.9-2 4.9-4.8 5.9-2.8-1-4.8-3-4.8-5.9V3.9z" />
      <path d="M5.9 7.7 7.4 9.2 10.2 6.3" />
    </>
  ),
  alert: (
    <>
      <path d="M8 2.6 13.7 12.6H2.3z" />
      <path d="M8 6.1v3.1" />
      <path d="M8 10.7v.4" />
    </>
  ),
  clock: (
    <>
      <circle cx="8" cy="8" r="5.6" />
      <path d="M8 4.9v3.3l2.1 1.3" />
    </>
  ),
  branch: (
    <>
      <circle cx="4.9" cy="3.9" r="1.4" />
      <circle cx="4.9" cy="12.1" r="1.4" />
      <circle cx="11.1" cy="6" r="1.4" />
      <path d="M4.9 5.4v5.2" />
      <path d="M11.1 7.5c0 1.7-1.6 2.6-3.4 2.6H4.9" />
    </>
  ),
  folder: <path d="M2.6 4.4h3.6l1.2 1.5h5.9v6.9H2.6z" />,
  folderPlus: (
    <>
      <path d="M2.6 4.4h3.6l1.2 1.5h5.9v6.9H2.6z" />
      <path d="M8 9V6M6.5 7.5h3" />
    </>
  ),
  message: <path d="M2.8 3.8h10.4v7.1H8.4L5.2 13V10.9H2.8z" />,
  layers: (
    <>
      <path d="M8 2.4 13.1 5 8 7.6 2.9 5z" />
      <path d="M3.2 8.2 8 10.6l4.8-2.4" />
    </>
  ),
  download: (
    <>
      <path d="M8 3.4v6.2M5.4 7.2 8 9.8l2.6-2.6" />
      <path d="M3.6 12.6h8.8" />
    </>
  ),
  link: (
    <>
      <path d="M6.6 9.4 9.4 6.6" />
      <path d="M7.3 5.2 8.6 3.9a2.3 2.3 0 0 1 3.3 3.3l-1.3 1.3" />
      <path d="M8.7 10.8 7.4 12.1a2.3 2.3 0 0 1-3.3-3.3l1.3-1.3" />
    </>
  ),
  trash: (
    <>
      <path d="M3.4 5.2h9.2" />
      <path d="M6.4 5.2V3.8h3.2v1.4" />
      <path d="M5.1 5.2l.6 7.3h4.6l.6-7.3" />
    </>
  ),
  archive: (
    <>
      <path d="M2.9 4.4h10.2v2.2H2.9z" />
      <path d="M3.8 6.6v5.6h8.4V6.6" />
      <path d="M6.4 8.9h3.2" />
    </>
  ),
  check: <path d="M3.6 8.4 6.4 11.2 12.4 4.8" />,
  more: (
    <>
      <circle cx="3.6" cy="8" r="0.9" fill="currentColor" stroke="none" />
      <circle cx="8" cy="8" r="0.9" fill="currentColor" stroke="none" />
      <circle cx="12.4" cy="8" r="0.9" fill="currentColor" stroke="none" />
    </>
  ),
  copy: (
    <>
      <rect x="5.4" y="5.4" width="7.2" height="7.2" rx="1.2" />
      <path d="M10.2 5.4V4.2A1.2 1.2 0 0 0 9 3H4.2A1.2 1.2 0 0 0 3 4.2V9a1.2 1.2 0 0 0 1.2 1.2h1.2" />
    </>
  ),
  diff: (
    <>
      <path d="M3.2 4.8h9.6M3.2 11.2h9.6" />
      <circle cx="6" cy="4.8" r="1.3" />
      <circle cx="10" cy="11.2" r="1.3" />
    </>
  ),
  play: <path d="M6.2 4.1 11.4 8l-5.2 3.9z" />,
  send: <path d="M8 12.6V3.8M4.4 7.2 8 3.6l3.6 3.6" />,
  search: (
    <>
      <circle cx="7.1" cy="7.1" r="3.8" />
      <path d="M9.9 9.9 13 13" />
    </>
  ),
  book: (
    <>
      <path d="M2.6 4.1h4.1c.8 0 1.3.5 1.3 1.3v7.1c0-.8-.5-1.3-1.3-1.3H2.6z" />
      <path d="M13.4 4.1H9.3c-.8 0-1.3.5-1.3 1.3v7.1c0-.8.5-1.3 1.3-1.3h4.1z" />
    </>
  ),
  settings: (
    <>
      <path d="M2.4 4.6h4.2M10.4 4.6h3.2M2.4 11.4h5.4M11.6 11.4h2" />
      <circle cx="8.4" cy="4.6" r="1.6" />
      <circle cx="9.7" cy="11.4" r="1.6" />
    </>
  ),
  refresh: (
    <>
      <path d="M13.1 8a5.1 5.1 0 1 1-1.5-3.6" />
      <path d="M13.2 3.2v2.8h-2.8" />
    </>
  ),
  stop: <rect x="5" y="5" width="6" height="6" rx="1.2" />,
  arrowRight: <path d="M2.6 8h8.2M7.6 4.6 11 8l-3.4 3.4" />,
  sparkle: (
    <>
      <path d="M8 2.2l1.3 3.2 3.2 1.3-3.2 1.3L8 11.2 6.7 8 3.5 6.7 6.7 5.4z" />
      <path d="M12.6 10.8l.5 1.2 1.2.5-1.2.5-.5 1.2-.5-1.2-1.2-.5 1.2-.5z" />
    </>
  ),
  brain: (
    <>
      <path d="M8 3.4C6.6 2.5 4.9 3.2 4.8 4.6c-1.3.2-2.2 1.4-1.8 2.6-1 .7-1 2.3 0 3.1 0 1.3 1.2 2.2 2.5 2.1.4.8 1.3 1.3 2.3 1.1" />
      <path
        d="M8 3.4C6.6 2.5 4.9 3.2 4.8 4.6c-1.3.2-2.2 1.4-1.8 2.6-1 .7-1 2.3 0 3.1 0 1.3 1.2 2.2 2.5 2.1.4.8 1.3 1.3 2.3 1.1"
        transform="translate(16 0) scale(-1 1)"
      />
      <path d="M8 3.4v10.1" />
      <path d="M5.2 6c.7.1 1.2.6 1.4 1.3" />
      <path d="M4.3 9c.7.1 1.2.5 1.4 1.2" />
      <path d="M5.2 6c.7.1 1.2.6 1.4 1.3" transform="translate(16 0) scale(-1 1)" />
      <path d="M4.3 9c.7.1 1.2.5 1.4 1.2" transform="translate(16 0) scale(-1 1)" />
    </>
  ),
};

export type IconName = keyof typeof PATHS;

export function Icon({
  name,
  size,
  className,
}: {
  name: IconName;
  size?: "xs" | "sm" | "lg";
  className?: string;
}) {
  const classes = ["od-icon", size ?? "", className ?? ""].filter(Boolean).join(" ");
  return (
    <svg
      className={classes}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {PATHS[name]}
    </svg>
  );
}

export function BrandMark() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="15"
      height="15"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="6.5" cy="5" r="2.2" />
      <circle cx="6.5" cy="19" r="2.2" />
      <circle cx="17.5" cy="8.5" r="2.2" />
      <path d="M6.5 7.2v9.6" />
      <path d="M17.5 10.7c0 3-2.6 4.3-5.5 4.3H6.5" />
    </svg>
  );
}
