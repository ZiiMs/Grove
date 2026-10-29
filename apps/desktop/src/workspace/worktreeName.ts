/**
 * Readable, generated worktree names. A worktree is named for a place, not for the task, so the
 * name never races the session title (which may not exist until the first message).
 */

const ADJECTIVES = [
  "quiet",
  "amber",
  "silent",
  "bright",
  "soft",
  "wild",
  "calm",
  "swift",
  "gentle",
  "hidden",
  "silver",
  "golden",
  "misty",
  "sunny",
  "cool",
  "warm",
  "brave",
  "clever",
  "lucky",
  "merry",
  "noble",
  "proud",
  "rapid",
  "steady",
  "tidy",
  "vivid",
  "witty",
  "young",
  "ancient",
  "brisk",
  "cozy",
  "eager",
  "fancy",
  "humble",
  "jolly",
  "kind",
  "lively",
  "mellow",
  "nimble",
  "plain",
  "polite",
  "royal",
  "sharp",
  "smart",
  "smooth",
  "solid",
  "tranquil",
  "wise",
] as const;

const PLANTS = [
  "cedar",
  "willow",
  "aspen",
  "birch",
  "alder",
  "hazel",
  "rowan",
  "juniper",
  "spruce",
  "larch",
  "cypress",
  "olive",
  "elm",
  "oak",
  "pine",
  "fir",
  "ash",
  "yew",
  "holly",
  "magnolia",
  "sequoia",
  "redwood",
  "poplar",
  "walnut",
  "chestnut",
  "dogwood",
  "sycamore",
  "hickory",
  "beech",
  "mulberry",
  "plum",
  "cherry",
  "apple",
  "pear",
  "fig",
  "lime",
  "lemon",
  "peach",
  "palm",
  "bamboo",
  "eucalyptus",
  "acacia",
  "ginkgo",
  "mango",
  "mangrove",
  "teak",
  "fern",
  "moss",
] as const;

const TOTAL = ADJECTIVES.length * PLANTS.length;

/**
 * Pick a readable adjective-plant name, avoiding every name in `taken`.
 *
 * The starting point is random; walking the whole pool from there means even a fixed seed skips
 * collisions instead of repeatedly drawing the same name. The numeric fallback is unreachable
 * unless every combination is already in use.
 */
export function generateWorktreeName(
  taken: ReadonlySet<string> = new Set<string>(),
  random: () => number = Math.random,
): string {
  const start = Math.floor(random() * TOTAL) % TOTAL;
  for (let offset = 0; offset < TOTAL; offset += 1) {
    const index = (start + offset) % TOTAL;
    const adjective = ADJECTIVES[Math.floor(index / PLANTS.length)];
    const plant = PLANTS[index % PLANTS.length];
    const name = `${adjective}-${plant}`;
    if (!taken.has(name)) return name;
  }

  const base = `${ADJECTIVES[0]}-${PLANTS[0]}`;
  let suffix = 2;
  while (taken.has(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}
