import type { ShortcutRecord } from "../../types/server";

/** Extracts unique {{variable}} names from a shortcut template in display order. */
export function shortcutVariables(template: string): string[] {
  const names: string[] = [];
  for (const match of template.matchAll(/\{\{\s*([a-zA-Z0-9_-]+)\s*\}\}/g)) {
    const name = match[1];
    if (name && !names.includes(name)) names.push(name);
  }
  return names;
}

/** Replaces shortcut variables with user values and rejects unresolved placeholders. */
export function materializeShortcut(template: string, values: Record<string, string>): string {
  const result = template.replace(/\{\{\s*([a-zA-Z0-9_-]+)\s*\}\}/g, (_, key: string) => values[key]?.trim() ?? "");
  return result.replace(/\s+/g, " ").trim();
}

/** Scores a shortcut against the current shell line using command, token, label, and usage matches. */
function scoreShortcut(shortcut: ShortcutRecord, input: string): number {
  const query = input.trim().toLocaleLowerCase();
  if (!query) return 100 + Math.min(shortcut.usageCount, 50);
  const command = shortcut.commandTemplate.toLocaleLowerCase();
  const name = shortcut.name.toLocaleLowerCase();
  const description = shortcut.description.toLocaleLowerCase();
  const tags = shortcut.tags.join(" ").toLocaleLowerCase();
  if (command === query) return 1_400 + Math.min(shortcut.usageCount, 100);
  if (command.startsWith(query)) return 1_200 + Math.min(shortcut.usageCount, 100);
  const queryTokens = query.split(/\s+/).filter(Boolean);
  const commandTokens = command.split(/\s+/);
  if (queryTokens.every((token, index) => commandTokens[index]?.startsWith(token))) {
    return 1_000 + Math.min(shortcut.usageCount, 100);
  }
  if (name.includes(query) || description.includes(query) || tags.includes(query)) {
    return 700 + Math.min(shortcut.usageCount, 100);
  }
  return -1;
}

/** Returns at most six deterministic completion candidates for a shell input line. */
export function matchShortcuts(shortcuts: ShortcutRecord[], input: string): ShortcutRecord[] {
  return shortcuts
    .filter((shortcut) => shortcut.enabled)
    .map((shortcut, index) => ({ shortcut, score: scoreShortcut(shortcut, input), index }))
    .filter((item) => item.score >= 0)
    .sort((left, right) => right.score - left.score || left.index - right.index)
    .slice(0, 6)
    .map((item) => item.shortcut);
}
