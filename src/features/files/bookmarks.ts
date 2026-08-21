export type FileBookmark = {
  id: string;
  name: string;
  path: string;
  createdAt: number;
};

/** 返回当前服务器独立的文件书签存储键，避免不同服务器之间串用路径。 */
function storageKey(serverId: string) {
  return `relay:file-bookmarks:${serverId}`;
}

/** 读取并校验本地保存的文件书签；损坏数据会安全回退为空列表。 */
export function readFileBookmarks(serverId: string): FileBookmark[] {
  if (!serverId || typeof window === "undefined") return [];
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(storageKey(serverId)) ?? "[]");
    if (!Array.isArray(value)) return [];
    return value.filter((bookmark): bookmark is FileBookmark => {
      if (!bookmark || typeof bookmark !== "object") return false;
      const item = bookmark as Partial<FileBookmark>;
      return typeof item.id === "string" && typeof item.name === "string" && typeof item.path === "string" && typeof item.createdAt === "number";
    });
  } catch {
    return [];
  }
}

/** 持久化当前服务器的文件书签列表。 */
export function writeFileBookmarks(serverId: string, bookmarks: FileBookmark[]) {
  if (!serverId || typeof window === "undefined") return;
  try {
    window.localStorage.setItem(storageKey(serverId), JSON.stringify(bookmarks));
  } catch {
    // 本地存储不可用时保持当前页面状态，不能阻断远程文件操作。
  }
}

/** 判断 candidate 是否为 base 文件夹本身或其下级路径。 */
export function isPathInside(base: string, candidate: string) {
  const normalizedBase = base === "/" ? "/" : base.replace(/\/+$/, "");
  const normalizedCandidate = candidate === "/" ? "/" : candidate.replace(/\/+$/, "");
  return normalizedCandidate === normalizedBase || (normalizedBase === "/" ? normalizedCandidate.startsWith("/") : normalizedCandidate.startsWith(`${normalizedBase}/`));
}

/** 将移动或重命名后的目录路径前缀同步到所有受影响书签。 */
export function replaceBookmarkPrefix(bookmarks: FileBookmark[], source: string, destination: string) {
  return bookmarks.map((bookmark) => {
    if (!isPathInside(source, bookmark.path)) return bookmark;
    const suffix = bookmark.path.slice(source.length);
    return { ...bookmark, path: `${destination.replace(/\/+$/, "")}${suffix}` || "/" };
  });
}

/** 删除指定目录及其子树下的书签，防止留下无法跳转的书签。 */
export function removeBookmarksInside(bookmarks: FileBookmark[], root: string) {
  return bookmarks.filter((bookmark) => !isPathInside(root, bookmark.path));
}
