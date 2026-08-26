#!/usr/bin/env bun

// Generate lazygitrs built-in themes from OpenCode's TUI themes.
// Run via: `bun run scripts/gen-themes.ts`

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

type GitHubFile = {
  name: string;
  download_url?: string;
};

type DarkLight = { dark: string; light: string };

type OpenCodeTheme = {
  defs: Record<string, string>;
  theme: Record<string, DarkLight | string>;
};

// ── lazygitrs native theme JSON shape ───────────────────────────────────

type LazygitrsTheme = {
  id: string;
  name: string;
  appearance: "dark" | "light";

  // Semantic base colors
  primary: string;
  secondary: string;
  accent: string;
  accent_secondary: string;
  success: string;
  error: string;
  warning: string;
  info: string;

  // Text
  text: string;
  text_strong: string;
  text_dimmed: string;

  // Background / chrome
  background: string;
  background_panel: string;
  selected_bg: string;
  separator: string;
  popup_border: string;

  // Borders
  border: string;
  border_active: string;

  // Diff
  diff_add: string;
  diff_remove: string;
  diff_context: string;
  diff_add_bg: string;
  diff_remove_bg: string;
  diff_add_word: string;
  diff_remove_word: string;
  diff_line_number: string;

  // Syntax
  syntax_comment: string;
  syntax_keyword: string;
  syntax_string: string;
  syntax_number: string;
  syntax_function: string;
  syntax_type: string;
  syntax_operator: string;
  syntax_punctuation: string;
  syntax_variable: string;

  // Graph colors (8)
  graph_colors: string[];
};

// ── Helpers ─────────────────────────────────────────────────────────────

const OPENCODE_REF = process.env.OPENCODE_REF ?? "production";
const GITHUB_API_URL = `https://api.github.com/repos/anomalyco/opencode/contents/packages/tui/src/theme/assets?ref=${encodeURIComponent(OPENCODE_REF)}`;
const THEMES_DIR = join(process.cwd(), "src", "generated_themes");

/** Resolve a value that may be a defs key or a hex color. */
function resolveDef(defs: Record<string, string>, value: string): string {
  if (value.startsWith("#")) return value;
  return defs[value] ?? value;
}

/** Pick the dark-mode value from a theme entry and resolve it. */
function resolveColor(
  defs: Record<string, string>,
  theme: Record<string, DarkLight | string>,
  key: string,
  fallback: string
): string {
  const entry = theme[key];
  if (!entry) return fallback;
  if (typeof entry === "string") return resolveDef(defs, entry);
  return resolveDef(defs, entry.dark);
}

/** Parse a hex color to [r, g, b]. */
function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ];
}

/** Convert [r, g, b] back to hex. */
function rgbToHex(r: number, g: number, b: number): string {
  return (
    "#" +
    [r, g, b].map((c) => Math.max(0, Math.min(255, c)).toString(16).padStart(2, "0")).join("")
  );
}

/** Darken a hex color by mixing toward black. `amount` is 0..1. */
function darken(hex: string, amount: number): string {
  const [r, g, b] = hexToRgb(hex);
  return rgbToHex(
    Math.round(r * (1 - amount)),
    Math.round(g * (1 - amount)),
    Math.round(b * (1 - amount))
  );
}

/** Mix two hex colors. `t` is 0..1 (0 = a, 1 = b). */
function mix(a: string, b: string, t: number): string {
  const [ar, ag, ab] = hexToRgb(a);
  const [br, bg, bb] = hexToRgb(b);
  return rgbToHex(
    Math.round(ar + (br - ar) * t),
    Math.round(ag + (bg - ag) * t),
    Math.round(ab + (bb - ab) * t)
  );
}

/** Convert a filename like "catppuccin-macchiato.json" to a display name. */
function fileNameToDisplayName(name: string): string {
  return name
    .replace(".json", "")
    .split(/[-_]/)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

// ── Transform ───────────────────────────────────────────────────────────

/** Infer light/dark from background luminance. */
function appearanceFromHex(hex: string): "dark" | "light" {
  const [r, g, b] = hexToRgb(hex);
  const lum = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  return lum >= 0.45 ? "light" : "dark";
}


function transformTheme(opencode: OpenCodeTheme, filename: string): LazygitrsTheme {
  const { defs, theme } = opencode;
  const r = (key: string, fallback: string) => resolveColor(defs, theme, key, fallback);

  // Base semantic colors
  const primary = r("primary", "#88c0d0");
  const secondary = r("secondary", "#b48ead");
  const success = r("success", "#a3be8c");
  const error = r("error", "#bf616a");
  const warning = r("warning", "#ebcb8b");
  const info = r("info", "#81a1c1");
  const text = r("text", "#d8dee9");
  const textMuted = r("textMuted", "#4c566a");
  const background = r("background", "#2e3440");
  const backgroundPanel = r("backgroundPanel", "#3b4252");
  const backgroundElement = r("backgroundElement", "#434c5e");
  const border = r("border", "#4c566a");
  const borderActive = r("borderActive", primary);

  // Diff colors (OpenCode provides these directly)
  const diffAdded = r("diffAdded", success);
  const diffRemoved = r("diffRemoved", error);
  const diffContext = r("diffContext", textMuted);
  const diffAddedBg = r("diffAddedBg", darken(success, 0.8));
  const diffRemovedBg = r("diffRemovedBg", darken(error, 0.8));
  const diffHighlightAdded = r("diffHighlightAdded", success);
  const diffHighlightRemoved = r("diffHighlightRemoved", error);
  const diffLineNumber = r("diffLineNumber", backgroundElement);

  // Syntax
  const syntaxComment = r("syntaxComment", textMuted);
  const syntaxKeyword = r("syntaxKeyword", secondary);
  const syntaxString = r("syntaxString", success);
  const syntaxNumber = r("syntaxNumber", warning);
  const syntaxFunction = r("syntaxFunction", primary);
  const syntaxType = r("syntaxType", warning);
  const syntaxOperator = r("syntaxOperator", secondary);
  const syntaxPunctuation = r("syntaxPunctuation", text);
  const syntaxVariable = r("syntaxVariable", error);

  const id = filename.replace(".json", "");

  return {
    id,
    name: fileNameToDisplayName(filename),
    appearance: appearanceFromHex(background),

    primary,
    secondary,
    accent: primary,
    accent_secondary: warning,
    success,
    error,
    warning,
    info,

    text: textMuted,
    text_strong: text,
    text_dimmed: mix(textMuted, background, 0.3),

    background,
    background_panel: backgroundPanel,
    selected_bg: backgroundElement,
    separator: border,
    popup_border: primary,

    border,
    border_active: borderActive,

    diff_add: diffAdded,
    diff_remove: diffRemoved,
    diff_context: diffContext,
    diff_add_bg: diffAddedBg,
    diff_remove_bg: diffRemovedBg,
    diff_add_word: mix(diffHighlightAdded, background, 0.35),
    diff_remove_word: mix(diffHighlightRemoved, background, 0.35),
    diff_line_number: diffLineNumber,

    syntax_comment: syntaxComment,
    syntax_keyword: syntaxKeyword,
    syntax_string: syntaxString,
    syntax_number: syntaxNumber,
    syntax_function: syntaxFunction,
    syntax_type: syntaxType,
    syntax_operator: syntaxOperator,
    syntax_punctuation: syntaxPunctuation,
    syntax_variable: syntaxVariable,

    graph_colors: [primary, success, warning, secondary, info, error, mix(primary, success, 0.5), mix(warning, error, 0.5)],
  };
}

// ── Main ────────────────────────────────────────────────────────────────

async function main() {
  console.log(`Fetching OpenCode themes (ref: ${OPENCODE_REF})...`);
  const response = await fetch(GITHUB_API_URL);
  if (!response.ok) {
    throw new Error(`Failed to fetch theme listing: ${response.status} ${response.statusText}`);
  }

  const files = (await response.json()) as GitHubFile[];

  rmSync(THEMES_DIR, { recursive: true, force: true });
  mkdirSync(THEMES_DIR, { recursive: true });

  let count = 0;
  for (const file of files) {
    if (!file?.name?.endsWith(".json")) continue;
    if (!file.download_url) continue;

    console.log(`  Fetching ${file.name}...`);
    const themeResponse = await fetch(file.download_url);
    if (!themeResponse.ok) {
      console.error(`  Failed to fetch ${file.name}: ${themeResponse.status}`);
      continue;
    }

    const opencode = (await themeResponse.json()) as OpenCodeTheme;
    if (!opencode.defs || !opencode.theme) {
      console.error(`  Skipping ${file.name}: not a valid TUI theme`);
      continue;
    }

    const lazygitrsTheme = transformTheme(opencode, file.name);
    const outPath = join(THEMES_DIR, file.name);
    writeFileSync(outPath, JSON.stringify(lazygitrsTheme, null, 2) + "\n");
    console.log(`  -> ${file.name}`);
    count++;
  }

  console.log(`\nDone! ${count} themes saved to src/generated_themes/`);
}

main().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
