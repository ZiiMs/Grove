/**
 * The human name of a model id. The picker shows this next to the provider glyph; the exact id stays
 * available as the option hint and the trigger tooltip, so nothing is hidden from a user who needs to
 * copy it into a config file.
 */

/** Vendors whose id prefix a gateway (OpenRouter and friends) adds in front of the routed model. */
const GATEWAY_VENDORS = new Set(["anthropic", "openai", "google", "openrouter"]);

/**
 * Brand spellings a plain capitalisation gets wrong. Everything else is capitalised by its first
 * letter, which is all the product names below need (`sonnet` → `Sonnet`, `longcat` → `Longcat`).
 */
const BRANDS: Record<string, string> = {
  gpt: "GPT",
  glm: "GLM",
  mimo: "MiMo",
  minimax: "MiniMax",
  deepseek: "DeepSeek",
  kimi: "Kimi",
  gemini: "Gemini",
};

/** A trailing snapshot suffix: `-20250929`, `-2025-08-07`, or `-latest`. */
const SNAPSHOT = /-(?:\d{8}|\d{4}-\d{2}-\d{2}|latest)$/i;

/**
 * Prettify one model id for the provider the glyph already names.
 *
 * `claude-sonnet-4-5` → `Sonnet 4.5`, `gpt-5-mini-2025-08-07` → `GPT-5 Mini`,
 * `anthropic/claude-sonnet-4.5` → `Sonnet 4.5`. An id the rules leave empty is returned untouched,
 * so an unknown vendor's model still has a name instead of a blank.
 */
export function modelLabel(model: string, provider: string | null): string {
  let base = model.trim();

  // A gateway's vendor-qualified id names the routed vendor, not the provider. The trigger already
  // shows the provider, so the prefix is redundant. A provider's own prefix (`openai/gpt-5`) goes too.
  const slash = base.indexOf("/");
  if (slash > 0) {
    const prefix = base.slice(0, slash).toLowerCase();
    if (prefix === provider || GATEWAY_VENDORS.has(prefix)) base = base.slice(slash + 1);
  }

  let stripped = true;
  while (stripped) {
    const next = base.replace(SNAPSHOT, "");
    stripped = next !== base;
    base = next;
  }

  const tokens = base.split("-").filter(Boolean);
  // The family token repeats what the picker's provider grouping says: "Claude" never adds anything
  // over the Anthropic glyph.
  if (tokens[0]?.toLowerCase() === "claude") tokens.shift();

  // A version split by the tokeniser (`4-5`, `2-5`) is one number in the product name.
  const joined: string[] = [];
  for (const token of tokens) {
    const previous = joined[joined.length - 1];
    if (/^\d+$/.test(token) && previous !== undefined && /^\d+$/.test(previous)) {
      joined[joined.length - 1] = `${previous}.${token}`;
    } else {
      joined.push(token);
    }
  }

  const words = joined.map((token) => BRANDS[token.toLowerCase()] ?? capitalise(token));
  let label = "";
  words.forEach((word, index) => {
    if (index === 0) {
      label = word;
      return;
    }
    // Lettered acronyms bind to the version they launch with: GPT-5, GLM-5.3. Word-branded families
    // keep a space: Kimi K3, DeepSeek V4.1.
    const acronym = /^[A-Z]{2,}$/.test(words[index - 1]);
    label += acronym && /^\d/.test(word) ? `-${word}` : ` ${word}`;
  });

  return label || model;
}

function capitalise(token: string): string {
  return token.charAt(0).toUpperCase() + token.slice(1);
}
