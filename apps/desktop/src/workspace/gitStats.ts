export interface FileStat {
  added: number;
  removed: number;
}

function diffPath(raw: string): string | null {
  const trimmed = raw.trim().replace(/^"|"$/g, "");
  if (!trimmed || trimmed === "/dev/null") return null;
  return trimmed.replace(/^[ab]\//, "");
}

/** Per-file counts from one whole-tree diff, so a list never runs a command per file. */
export function statsFromPatch(patch: string): Record<string, FileStat> {
  const stats: Record<string, FileStat> = {};
  let current: string | null = null;
  for (const line of patch.split("\n")) {
    if (line.startsWith("--- ")) {
      const candidate = diffPath(line.slice(4));
      if (candidate) current = candidate;
      continue;
    }
    if (line.startsWith("+++ ")) {
      const candidate = diffPath(line.slice(4));
      if (candidate) current = candidate;
      continue;
    }
    if (line.startsWith("diff --git ")) {
      current = null;
      continue;
    }
    if (!current) continue;
    const stat = (stats[current] ??= { added: 0, removed: 0 });
    if (line.startsWith("+")) stat.added += 1;
    else if (line.startsWith("-")) stat.removed += 1;
  }
  return stats;
}

export function totalStats(stats: Record<string, FileStat>): FileStat {
  return Object.values(stats).reduce(
    (total, stat) => ({
      added: total.added + stat.added,
      removed: total.removed + stat.removed,
    }),
    { added: 0, removed: 0 },
  );
}
