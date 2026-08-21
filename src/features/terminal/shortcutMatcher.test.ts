import { describe, expect, it } from "vitest";
import { materializeShortcut, matchShortcuts, shortcutVariables } from "./shortcutMatcher";
import type { ShortcutRecord } from "../../types/server";

/** Builds a typed matcher fixture with a representative Docker group. */
const shortcut = (commandTemplate: string, description = "") => ({
  id: commandTemplate,
  scope: "global",
  serverId: null,
  name: commandTemplate,
  groupName: "Docker",
  commandTemplate,
  description,
  tags: ["docker"],
  enabled: true,
  builtin: true,
  usageCount: 0,
  createdAt: "2026-08-13T00:00:00Z",
  updatedAt: "2026-08-13T00:00:00Z",
}) satisfies ShortcutRecord;

describe("shortcut matcher", () => {
  it("matches token prefixes such as docker r to docker run", () => {
    const result = matchShortcuts([shortcut("docker ps -a"), shortcut("docker run --name {{name}} -d {{image}}")], "docker r");
    expect(result[0]?.commandTemplate).toContain("docker run");
  });

  it("extracts and materializes variables without leaving placeholders", () => {
    const template = "docker run --name {{name}} -d {{image}}";
    expect(shortcutVariables(template)).toEqual(["name", "image"]);
    expect(materializeShortcut(template, { name: "web", image: "nginx:latest" })).toBe("docker run --name web -d nginx:latest");
  });

  it("can match by explanation or tags when command text is not typed", () => {
    expect(matchShortcuts([shortcut("df -h", "查看磁盘使用情况")], "磁盘")).toHaveLength(1);
  });
});
