/**
 * PROTOTYPE — throwaway. Theme system for the UI exploration pass.
 * Owns its own tokens; nothing here touches src/styles/tokens.css.
 */

export type Scheme = "dark" | "light";

export interface Preset {
  id: string;
  name: string;
  scheme: Scheme;
  blurb: string;
  tokens: Record<string, string>;
}

export const presets: Preset[] = [
  {
    id: "lattice",
    name: "Lattice",
    scheme: "dark",
    blurb: "Neutral graphite with a mint signal. Grows out of today's Grove theme — safest base.",
    tokens: {
      "--bg": "#0d0f12",
      "--bg-sunken": "#08090c",
      "--bg-raised": "#14171c",
      "--bg-overlay": "#191d24",
      "--surface": "#11141a",
      "--surface-hover": "#181c23",
      "--border": "#232931",
      "--border-strong": "#333b46",
      "--ink": "#e7eaef",
      "--ink-muted": "#9aa3b2",
      "--ink-faint": "#68707d",
      "--accent": "#6ee7a8",
      "--accent-ink": "#04140c",
      "--accent-soft": "color-mix(in oklab, #6ee7a8 15%, transparent)",
      "--accent-line": "color-mix(in oklab, #6ee7a8 34%, transparent)",
      "--danger": "#f87171",
      "--danger-soft": "color-mix(in oklab, #f87171 15%, transparent)",
      "--success": "#4ade80",
      "--warning": "#fbbf24",
      "--info": "#7dd3fc",
      "--shadow": "0 12px 32px rgba(0, 0, 0, 0.45)",
    },
  },
  {
    id: "ember",
    name: "Ember",
    scheme: "dark",
    blurb: "Warm near-black and amber. Reads like a terminal at night; the loudest of the four.",
    tokens: {
      "--bg": "#12100d",
      "--bg-sunken": "#0b0a08",
      "--bg-raised": "#1a1713",
      "--bg-overlay": "#211d18",
      "--surface": "#17140f",
      "--surface-hover": "#1f1b16",
      "--border": "#2c2620",
      "--border-strong": "#3f372d",
      "--ink": "#f1eae0",
      "--ink-muted": "#ab9f8d",
      "--ink-faint": "#746a5c",
      "--accent": "#f0a04b",
      "--accent-ink": "#191007",
      "--accent-soft": "color-mix(in oklab, #f0a04b 16%, transparent)",
      "--accent-line": "color-mix(in oklab, #f0a04b 36%, transparent)",
      "--danger": "#e2725b",
      "--danger-soft": "color-mix(in oklab, #e2725b 16%, transparent)",
      "--success": "#9bbf6b",
      "--warning": "#f2c14e",
      "--info": "#7fb8c9",
      "--shadow": "0 12px 32px rgba(0, 0, 0, 0.5)",
    },
  },
  {
    id: "console",
    name: "Console",
    scheme: "dark",
    blurb: "Blue-black with cyan edges. The familiar dev-tool register — Conduit and OpenChamber territory.",
    tokens: {
      "--bg": "#070b12",
      "--bg-sunken": "#04070d",
      "--bg-raised": "#0d1420",
      "--bg-overlay": "#111a27",
      "--surface": "#0a1018",
      "--surface-hover": "#101a27",
      "--border": "#1b2635",
      "--border-strong": "#2a3a4e",
      "--ink": "#dde7f2",
      "--ink-muted": "#8fa1b8",
      "--ink-faint": "#5e6f86",
      "--accent": "#4cc9f0",
      "--accent-ink": "#03121a",
      "--accent-soft": "color-mix(in oklab, #4cc9f0 15%, transparent)",
      "--accent-line": "color-mix(in oklab, #4cc9f0 34%, transparent)",
      "--danger": "#ff6b6b",
      "--danger-soft": "color-mix(in oklab, #ff6b6b 15%, transparent)",
      "--success": "#3ddc97",
      "--warning": "#ffd166",
      "--info": "#7aa2f7",
      "--shadow": "0 12px 32px rgba(0, 0, 0, 0.55)",
    },
  },
  {
    id: "paperwork",
    name: "Paperwork",
    scheme: "light",
    blurb: "Warm paper, ink text, indigo accent. Built for spec reading rather than terminal adjacency.",
    tokens: {
      "--bg": "#f7f4ee",
      "--bg-sunken": "#efeae1",
      "--bg-raised": "#fffdf9",
      "--bg-overlay": "#ffffff",
      "--surface": "#fbf8f3",
      "--surface-hover": "#f3eee5",
      "--border": "#e0d8cb",
      "--border-strong": "#c8bcab",
      "--ink": "#24201a",
      "--ink-muted": "#6b6255",
      "--ink-faint": "#948a7c",
      "--accent": "#4c5fd5",
      "--accent-ink": "#ffffff",
      "--accent-soft": "color-mix(in oklab, #4c5fd5 10%, transparent)",
      "--accent-line": "color-mix(in oklab, #4c5fd5 28%, transparent)",
      "--danger": "#b42318",
      "--danger-soft": "color-mix(in oklab, #b42318 10%, transparent)",
      "--success": "#2f7d4f",
      "--warning": "#a15c07",
      "--info": "#1f6feb",
      "--shadow": "0 10px 26px rgba(60, 48, 32, 0.14)",
    },
  },
];

export interface AccentChoice {
  id: string;
  name: string;
  dark: string;
  light: string;
}

/** Knob: accent hue, in both schemes. `preset` means "keep the preset's own accent". */
export const accents: AccentChoice[] = [
  { id: "mint", name: "Mint", dark: "#6ee7a8", light: "#0f7a4a" },
  { id: "amber", name: "Amber", dark: "#f0a04b", light: "#9a5b06" },
  { id: "cyan", name: "Cyan", dark: "#4cc9f0", light: "#0b6f92" },
  { id: "violet", name: "Violet", dark: "#b28dff", light: "#5b3fc4" },
  { id: "rose", name: "Rose", dark: "#fb7185", light: "#a4123a" },
  { id: "lime", name: "Lime", dark: "#a3e635", light: "#4d6b00" },
];

export type RadiusStep = 0 | 4 | 8 | 12;
export type Density = "compact" | "cozy" | "comfy";

export interface ThemeState {
  presetId: string;
  /** "preset" keeps the palette's own accent. */
  accentId: string;
  radius: RadiusStep;
  density: Density;
}

export const defaultTheme: ThemeState = {
  presetId: "ember",
  accentId: "preset",
  radius: 0,
  density: "cozy",
};

export const densitySteps: { id: Density; name: string; blurb: string }[] = [
  { id: "compact", name: "Compact", blurb: "26px rows · 12.5px type" },
  { id: "cozy", name: "Cozy", blurb: "32px rows · 13px type" },
  { id: "comfy", name: "Comfy", blurb: "40px rows · 14px type" },
];

const densityVars: Record<Density, Record<string, string>> = {
  compact: {
    "--row-h": "26px",
    "--pad-y": "6px",
    "--pad-x": "10px",
    "--gap": "6px",
    "--font-md": "12.5px",
    "--font-sm": "11px",
    "--font-lg": "14px",
    "--font-display": "19px",
  },
  cozy: {
    "--row-h": "32px",
    "--pad-y": "10px",
    "--pad-x": "14px",
    "--gap": "8px",
    "--font-md": "13px",
    "--font-sm": "11.5px",
    "--font-lg": "15px",
    "--font-display": "22px",
  },
  comfy: {
    "--row-h": "40px",
    "--pad-y": "14px",
    "--pad-x": "18px",
    "--gap": "12px",
    "--font-md": "14px",
    "--font-sm": "12.5px",
    "--font-lg": "16px",
    "--font-display": "24px",
  },
};

export function findPreset(id: string): Preset {
  return presets.find((preset) => preset.id === id) ?? presets[0];
}

/**
 * Radius is a scale, not four independent values — and 0 means square everywhere, including the
 * elements styled as pills (chips, switches, progress). The prototype's own floating switcher opts
 * out with a literal, so it stays visibly not-part-of-the-design.
 */
function radiusVars(radius: RadiusStep): Record<string, string> {
  if (radius === 0) {
    return { "--radius-sm": "0px", "--radius-md": "0px", "--radius-lg": "0px", "--radius-xl": "0px", "--radius-pill": "0px" };
  }
  return {
    "--radius-sm": `${Math.round(radius / 2)}px`,
    "--radius-md": `${radius}px`,
    "--radius-lg": `${Math.round(radius * 1.5)}px`,
    "--radius-xl": `${radius * 2}px`,
    "--radius-pill": "999px",
  };
}

/** Every token the prototype's CSS reads, as inline custom properties on the root element. */
export function cssVars(theme: ThemeState): Record<string, string> {
  const preset = findPreset(theme.presetId);
  const accent = theme.accentId === "preset" ? null : accents.find((a) => a.id === theme.accentId);

  const vars: Record<string, string> = {
    ...preset.tokens,
    ...densityVars[theme.density],
    ...radiusVars(theme.radius),
    colorScheme: preset.scheme,
  };

  if (accent) {
    const hue = preset.scheme === "light" ? accent.light : accent.dark;
    vars["--accent"] = hue;
    vars["--accent-ink"] = preset.scheme === "light" ? "#ffffff" : "#04140c";
    vars["--accent-soft"] = `color-mix(in oklab, ${hue} 16%, transparent)`;
    vars["--accent-line"] = `color-mix(in oklab, ${hue} 34%, transparent)`;
  }
  return vars;
}
