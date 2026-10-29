/** The seven shipped visual languages. Components consume tokens only; this file is metadata. */

export type ThemeId =
  | "house"
  | "terminal"
  | "blueprint"
  | "softstack"
  | "signal"
  | "editorial"
  | "mono";

export type ThemeMode = "dark" | "light";

export interface ThemeMeta {
  id: ThemeId;
  name: string;
  fonts: string;
  radius: number;
  swatch: string[];
  preview: {
    dark: ThemePreview;
    light: ThemePreview;
  };
}

export interface ThemePreview {
  bg: string;
  fg: string;
  fg2: string;
  accent: string;
  ink: string;
  warn: string;
  line: string;
  font: string;
}

export const THEMES: ThemeMeta[] = [
  {
    id: "house",
    name: "House",
    fonts: "IBM Plex Sans · Plex Mono",
    radius: 6,
    swatch: ["#0b0d10", "#161a20", "#3fcf9f", "#e3a45f", "#e0766f"],
    preview: {
      dark: {
        bg: "#0b0d10", fg: "#e7ebef", fg2: "#a6b0ba", accent: "#3fcf9f", ink: "#04201a",
        warn: "#e3a45f", line: "#22272e", font: "IBM Plex Sans",
      },
      light: {
        bg: "#f6f8f9", fg: "#0d1216", fg2: "#44515b", accent: "#0b8f68", ink: "#ffffff",
        warn: "#8a5a12", line: "#e1e6ea", font: "IBM Plex Sans",
      },
    },
  },
  {
    id: "terminal",
    name: "Terminal Native",
    fonts: "JetBrains Mono throughout",
    radius: 0,
    swatch: ["#08090a", "#0f1213", "#8ce99a", "#e6c07a", "#ea8a80"],
    preview: {
      dark: {
        bg: "#08090a", fg: "#d9ded9", fg2: "#98a09a", accent: "#8ce99a", ink: "#04170b",
        warn: "#e6c07a", line: "#171b1c", font: "JetBrains Mono",
      },
      light: {
        bg: "#fbfbfa", fg: "#101312", fg2: "#3f4643", accent: "#1d7a34", ink: "#ffffff",
        warn: "#7a5a0a", line: "#e3e6e3", font: "JetBrains Mono",
      },
    },
  },
  {
    id: "blueprint",
    name: "Blueprint",
    fonts: "Space Grotesk · JetBrains Mono",
    radius: 0,
    swatch: ["#0b1117", "#131c25", "#7ab6e8", "#e0b463", "#e08279"],
    preview: {
      dark: {
        bg: "#0b1117", fg: "#dbe7f2", fg2: "#9db1c4", accent: "#7ab6e8", ink: "#06121c",
        warn: "#e0b463", line: "#20303e", font: "Space Grotesk",
      },
      light: {
        bg: "#f1f5f9", fg: "#0b141c", fg2: "#3c4c5b", accent: "#1d5f96", ink: "#ffffff",
        warn: "#77540a", line: "#d6e0e9", font: "Space Grotesk",
      },
    },
  },
  {
    id: "softstack",
    name: "Soft Stack",
    fonts: "Manrope · JetBrains Mono",
    radius: 12,
    swatch: ["#0c0e12", "#1a1e25", "#f0a868", "#f0a868", "#e8827b"],
    preview: {
      dark: {
        bg: "#0c0e12", fg: "#eaedf2", fg2: "#a8b0be", accent: "#f0a868", ink: "#1c1206",
        warn: "#f0a868", line: "#20242b", font: "Manrope",
      },
      light: {
        bg: "#eff1f4", fg: "#10141a", fg2: "#454e5c", accent: "#a35a12", ink: "#ffffff",
        warn: "#7d4f06", line: "#e4e7ec", font: "Manrope",
      },
    },
  },
  {
    id: "signal",
    name: "Signal",
    fonts: "Archivo · JetBrains Mono",
    radius: 2,
    swatch: ["#0a0a0a", "#161616", "#ffb020", "#ffb020", "#ff6b5e"],
    preview: {
      dark: {
        bg: "#0a0a0a", fg: "#ececec", fg2: "#a8a8a8", accent: "#ffb020", ink: "#1a1200",
        warn: "#ffb020", line: "#232323", font: "Archivo",
      },
      light: {
        bg: "#f7f6f4", fg: "#14120f", fg2: "#494540", accent: "#8a5c00", ink: "#ffffff",
        warn: "#7d5200", line: "#e2dfd9", font: "Archivo",
      },
    },
  },
  {
    id: "editorial",
    name: "Editorial Dark",
    fonts: "Fraunces · Plex Sans · JetBrains Mono",
    radius: 2,
    swatch: ["#0d0c0b", "#191614", "#d9603f", "#d9a95c", "#d9603f"],
    preview: {
      dark: {
        bg: "#0d0c0b", fg: "#f0eae2", fg2: "#b3a99d", accent: "#d9603f", ink: "#ffffff",
        warn: "#d9a95c", line: "#262320", font: "Fraunces",
      },
      light: {
        bg: "#faf7f2", fg: "#16120e", fg2: "#4b433a", accent: "#a83a20", ink: "#ffffff",
        warn: "#75520a", line: "#e5ddd1", font: "Fraunces",
      },
    },
  },
  {
    id: "mono",
    name: "Monochrome",
    fonts: "DM Sans · JetBrains Mono",
    radius: 4,
    swatch: ["#0b0b0c", "#161618", "#f4f4f6", "#adadb4", "#8a8a92"],
    preview: {
      dark: {
        bg: "#0b0b0c", fg: "#ededef", fg2: "#a7a7ad", accent: "#f4f4f6", ink: "#0b0b0c",
        warn: "#adadb4", line: "#232326", font: "DM Sans",
      },
      light: {
        bg: "#fafafa", fg: "#111113", fg2: "#45454b", accent: "#141416", ink: "#ffffff",
        warn: "#5b5b61", line: "#e3e3e5", font: "DM Sans",
      },
    },
  },
];

export const THEME_STORAGE_KEY = "grove.theme";
/** The shipped default; a fresh install starts here until the user picks another language. */
export const DEFAULT_THEME_ID: ThemeId = "signal";

export function findTheme(id: string | null | undefined): ThemeMeta {
  return (
    THEMES.find((theme) => theme.id === id) ??
    THEMES.find((theme) => theme.id === DEFAULT_THEME_ID) ??
    THEMES[0]
  );
}

/** Read the theme the inline boot script found, so React starts on the same one. */
export function readBootTheme(): { id: ThemeId; mode: ThemeMode } {
  try {
    const raw = JSON.parse(localStorage.getItem(THEME_STORAGE_KEY) ?? "null") as
      | { id?: string; mode?: string }
      | null;
    const theme = findTheme(raw?.id);
    return { id: theme.id, mode: raw?.mode === "light" ? "light" : "dark" };
  } catch {
    return { id: DEFAULT_THEME_ID, mode: "dark" };
  }
}

export function applyTheme(id: ThemeId, mode: ThemeMode): void {
  document.documentElement.dataset.theme = id;
  document.documentElement.dataset.mode = mode;
  try {
    localStorage.setItem(THEME_STORAGE_KEY, JSON.stringify({ id, mode }));
  } catch {
    // Storage can be blocked; the choice still applies for this run.
  }
}
