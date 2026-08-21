import { describe, expect, it } from "vitest";
import { isPathInside, removeBookmarksInside, replaceBookmarkPrefix, type FileBookmark } from "./bookmarks";

const bookmarks: FileBookmark[] = [
  { id: "root", name: "项目", path: "/srv/project", createdAt: 1 },
  { id: "nested", name: "配置", path: "/srv/project/config", createdAt: 2 },
  { id: "other", name: "其他", path: "/srv/other", createdAt: 3 },
];

describe("file bookmark path lifecycle", () => {
  it("recognizes only a folder subtree, not a similarly named sibling", () => {
    expect(isPathInside("/srv/project", "/srv/project/config")).toBe(true);
    expect(isPathInside("/srv/project", "/srv/project-old")).toBe(false);
  });

  it("moves all bookmarks below a renamed or moved folder", () => {
    expect(replaceBookmarkPrefix(bookmarks, "/srv/project", "/data/project").map((bookmark) => bookmark.path)).toEqual([
      "/data/project",
      "/data/project/config",
      "/srv/other",
    ]);
  });

  it("removes bookmarks below a deleted folder", () => {
    expect(removeBookmarksInside(bookmarks, "/srv/project").map((bookmark) => bookmark.id)).toEqual(["other"]);
  });
});
