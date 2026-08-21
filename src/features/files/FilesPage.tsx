import Editor from "@monaco-editor/react";
import "../../lib/monaco";
import * as ContextMenu from "@radix-ui/react-context-menu";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertCircle, ArrowDownToLine, ArrowLeft, ArrowRight, ArrowUp, Bookmark, CheckCircle2, Eye, File, FilePlus2, FileText, Folder, FolderPlus, FolderUp, Link2, LoaderCircle, PanelRightClose, Pencil, RefreshCw, RotateCw, Save, Search, Star, Trash2, Upload, X } from "lucide-react";
import { type KeyboardEvent as ReactKeyboardEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { NavLink, useParams, useSearchParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage, isAppError } from "../../lib/errors";
import { formatBytes } from "../../lib/format";
import type { DirectoryListing, RemoteFileEntry, RemoteTextFile } from "../../types/server";
import { FileModal, RemoteFolderPickerDialog } from "./FileDialogs";
import { isPathInside, readFileBookmarks, removeBookmarksInside, replaceBookmarkPrefix, type FileBookmark, writeFileBookmarks } from "./bookmarks";
import { useTransferStore } from "./transferStore";

type SortKey = "name" | "size" | "modifiedAt";
type FileNotice = { kind: "success" | "error"; title: string; detail?: string };
type InputDialogState = { kind: "create-folder" | "create-file" | "rename" | "chmod" | "symlink"; entry?: RemoteFileEntry; value: string };
type BookmarkDialogState = { mode: "create" | "rename"; bookmark?: FileBookmark; value: string };
type FilePreview =
  | { kind: "directory"; entries: RemoteFileEntry[] }
  | { kind: "image"; dataUrl: string; size: number }
  | { kind: "text"; content: string; size: number }
  | { kind: "unsupported"; message: string }
  | { kind: "error"; message: string };

/** 组合当前远程目录和对象名称，返回规范的 POSIX 路径。 */
const join = (base: string, name: string) => base === "/" ? `/${name}` : `${base.replace(/\/$/, "")}/${name}`;

/** 返回远程路径的父目录；根目录没有更高层级。 */
const parent = (value: string) => { if (value === "/") return "/"; const clean = value.replace(/\/$/, ""); return clean.slice(0, clean.lastIndexOf("/")) || "/"; };

/** 展示文件行对应的图标，保持目录、普通文件和特殊对象的视觉区分。 */
function fileIcon(entry: RemoteFileEntry, loading: boolean) {
  if (loading) return <LoaderCircle className="spin" size={15} />;
  if (entry.kind === "directory") return <Folder size={15} />;
  if (entry.kind === "file") return <FileText size={15} />;
  return <File size={15} />;
}

/** 判断文件扩展名是否适合交给远程图片预览接口读取。 */
function isImagePath(path: string) {
  return /\.(png|jpe?g|gif|webp|bmp|ico)$/i.test(path);
}

/** 读取当前选中对象的轻量预览数据，并把不可预览对象转换为可解释状态。 */
async function loadFilePreview(serverId: string, entry: RemoteFileEntry): Promise<FilePreview> {
  if (entry.kind === "directory") {
    const listing: DirectoryListing = await api.listDirectory(serverId, entry.path);
    return { kind: "directory", entries: listing.entries };
  }
  if (entry.kind !== "file") return { kind: "unsupported", message: "符号链接和特殊文件暂不支持安全预览，请双击打开或下载。" };
  if (isImagePath(entry.path)) {
    try {
      const preview = await api.readImagePreview(serverId, entry.path);
      return { kind: "image", dataUrl: `data:${preview.mimeType};base64,${preview.dataBase64}`, size: preview.size };
    } catch (reason) {
      return { kind: "unsupported", message: isAppError(reason) ? reason.message : errorMessage(reason) };
    }
  }
  try {
    const file = await api.readText(serverId, entry.path);
    return { kind: "text", content: file.content, size: file.size };
  } catch (reason) {
    if (isAppError(reason) && reason.code === "FILE_TOO_LARGE") return { kind: "unsupported", message: "文件较大，预览面板只读取 10 MB 以内的文本；可双击使用大文件查看器。" };
    if (isAppError(reason) && reason.code === "FILE_NOT_TEXT") return { kind: "unsupported", message: "该文件不是 UTF-8 文本，当前暂不支持此类型的在线预览。" };
    return { kind: "error", message: errorMessage(reason) };
  }
}

/** 展示远程文件浏览器，并协调 SFTP 编辑、上传和下载任务。 */
export function FilesPage() {
  const { serverId = "" } = useParams();
  const [searchParams] = useSearchParams();
  const queryClient = useQueryClient();
  const [path, setPath] = useState("/");
  const [pathInput, setPathInput] = useState("/");
  const [history, setHistory] = useState(["/"]);
  const [historyIndex, setHistoryIndex] = useState(0);
  const [filter, setFilter] = useState("");
  const [showHidden, setShowHidden] = useState(false);
  const [sort, setSort] = useState<SortKey>("name");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [editorFile, setEditorFile] = useState<RemoteTextFile | null>(null);
  const [editorValue, setEditorValue] = useState("");
  const [editorDirty, setEditorDirty] = useState(false);
  const [largeViewer, setLargeViewer] = useState<RemoteTextFile | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<RemoteFileEntry | null>(null);
  const [inputDialog, setInputDialog] = useState<InputDialogState | null>(null);
  const [inputValidationError, setInputValidationError] = useState<string | null>(null);
  const [folderPicker, setFolderPicker] = useState<{ entry: RemoteFileEntry; operation: "copy" | "move" } | null>(null);
  const [bookmarkDialog, setBookmarkDialog] = useState<BookmarkDialogState | null>(null);
  const [bookmarkManagerOpen, setBookmarkManagerOpen] = useState(false);
  const [, setBookmarkVersion] = useState(0);
  const [notice, setNotice] = useState<FileNotice | null>(null);
  const [openingPath, setOpeningPath] = useState<string | null>(null);
  const [previewOpen, setPreviewOpen] = useState(true);
  const [dropActive, setDropActive] = useState(false);
  const [uploadConflict, setUploadConflict] = useState<"replace" | "skip" | "rename">("replace");
  const listRef = useRef<HTMLDivElement>(null);
  const openedPathRef = useRef<string | null>(null);
  const listing = useQuery({ queryKey: ["files", serverId, path], queryFn: () => api.listDirectory(serverId, path) });
  const profile = useQuery({ queryKey: ["server", serverId], queryFn: () => api.getServer(serverId) });
  const bookmarks = readFileBookmarks(serverId);
  const currentBookmark = bookmarks.find((bookmark) => bookmark.path === path) ?? null;

  /** 显示可关闭的文件操作反馈，成功和失败都使用同一套视觉语言。 */
  const showNotice = useCallback((next: FileNotice) => setNotice(next), []);

  // 文件操作反馈保留五秒后自动收起；用户手动关闭时由 cleanup 取消计时器。
  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(null), 5000);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  /** 在本地持久化书签变更，并让当前服务器的书签条立即刷新。 */
  const persistBookmarks = useCallback((next: FileBookmark[]) => {
    writeFileBookmarks(serverId, next);
    setBookmarkVersion((version) => version + 1);
  }, [serverId]);

  /** 读取远程文件或大文件尾部，并把错误交给文件页内的可关闭反馈。 */
  const openRemotePath = useCallback(async (requestedPath: string) => {
    setOpeningPath(requestedPath);
    try {
      const file = await api.readText(serverId, requestedPath);
      setEditorFile(file); setEditorValue(file.content); setEditorDirty(false);
    } catch (reason) {
      if (isAppError(reason) && reason.code === "FILE_TOO_LARGE") {
        try { setLargeViewer(await api.readTail(serverId, requestedPath)); }
        catch (tailReason) { showNotice({ kind: "error", title: "大文件读取失败", detail: errorMessage(tailReason) }); }
      } else showNotice({ kind: "error", title: "文件打开失败", detail: errorMessage(reason) });
    } finally { setOpeningPath(null); }
  }, [serverId, showNotice]);

  useEffect(() => {
    const requestedPath = searchParams.get("open");
    if (!requestedPath || openedPathRef.current === requestedPath) return;
    openedPathRef.current = requestedPath;
    void openRemotePath(requestedPath);
  }, [openRemotePath, searchParams]);

  const entries = useMemo(() => {
    const values = (listing.data?.entries ?? []).filter((entry) => (showHidden || !entry.name.startsWith(".")) && entry.name.toLocaleLowerCase().includes(filter.toLocaleLowerCase()));
    return values.sort((left, right) => {
      if (left.kind === "directory" && right.kind !== "directory") return -1;
      if (right.kind === "directory" && left.kind !== "directory") return 1;
      if (sort === "size") return left.size - right.size;
      if (sort === "modifiedAt") return (right.modifiedAt ?? 0) - (left.modifiedAt ?? 0);
      return left.name.localeCompare(right.name, "zh-CN", { numeric: true });
    });
  }, [listing.data, showHidden, filter, sort]);
  const selectedEntries = entries.filter((entry) => selected.has(entry.path));
  const selectedEntry = selectedEntries.length === 1 ? selectedEntries[0] : null;
  const previewEntry = selectedEntry;
  const previewQuery = useQuery({
    queryKey: ["file-preview", serverId, previewEntry?.path],
    queryFn: () => loadFilePreview(serverId, previewEntry as RemoteFileEntry),
    enabled: Boolean(previewOpen && previewEntry),
  });
  // TanStack Virtual deliberately returns an imperative object that React Compiler cannot memoize.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({ count: entries.length, getScrollElement: () => listRef.current, estimateSize: () => 38, overscan: 12 });

  /** 切换当前目录并按用户意图记录浏览历史。 */
  const navigatePath = (next: string, record = true) => {
    setPath(next); setPathInput(next); setSelected(new Set());
    if (record) { const nextHistory = [...history.slice(0, historyIndex + 1), next]; setHistory(nextHistory); setHistoryIndex(nextHistory.length - 1); }
  };

  /** 刷新当前目录列表，保留当前路径、筛选条件和选择器状态。 */
  const refresh = useCallback(() => { void queryClient.invalidateQueries({ queryKey: ["files", serverId, path] }); }, [path, queryClient, serverId]);

  /** 打开新增或编辑书签对话框，默认使用当前目录名称作为书签名。 */
  const promptBookmark = () => {
    setBookmarkDialog(currentBookmark ? { mode: "rename", bookmark: currentBookmark, value: currentBookmark.name } : { mode: "create", value: path === "/" ? "根目录" : path.split("/").filter(Boolean).pop() ?? path });
  };

  /** 打开指定书签的重命名对话框。 */
  const promptRenameBookmark = (bookmark: FileBookmark) => setBookmarkDialog({ mode: "rename", bookmark, value: bookmark.name });

  /** 删除一个书签并立即给出可关闭反馈。 */
  const removeBookmark = (bookmark: FileBookmark) => {
    persistBookmarks(bookmarks.filter((item) => item.id !== bookmark.id));
    showNotice({ kind: "success", title: "书签已移除", detail: bookmark.name });
  };

  /** 先验证书签路径仍可访问，再跳转并关闭书签管理面板。 */
  const goBookmark = async (bookmark: FileBookmark) => {
    try {
      const result = await api.listDirectory(serverId, bookmark.path);
      setBookmarkManagerOpen(false);
      navigatePath(result.path);
    } catch (reason) {
      showNotice({ kind: "error", title: "书签路径不可用", detail: `${bookmark.name} · ${errorMessage(reason)}` });
    }
  };

  /** 提交书签名称并保持同一路径，避免重命名书签改变跳转目标。 */
  const submitBookmark = (value: string) => {
    const name = value.trim();
    if (!bookmarkDialog || !name) return;
    if (bookmarkDialog.mode === "rename" && bookmarkDialog.bookmark) {
      persistBookmarks(bookmarks.map((bookmark) => bookmark.id === bookmarkDialog.bookmark?.id ? { ...bookmark, name } : bookmark));
      showNotice({ kind: "success", title: "书签已更新", detail: name });
    } else {
      const next: FileBookmark = { id: crypto.randomUUID(), name, path, createdAt: Date.now() };
      persistBookmarks([...bookmarks, next]);
      showNotice({ kind: "success", title: "书签已添加", detail: path });
    }
    setBookmarkDialog(null);
  };

  const create = useMutation({
    mutationFn: ({ name, directory }: { name: string; directory: boolean }) => api.createEntry(serverId, join(path, name), directory),
    onSuccess: (_value, variables) => { setInputDialog(null); setInputValidationError(null); showNotice({ kind: "success", title: variables.directory ? "文件夹已创建" : "文件已创建", detail: join(path, variables.name) }); void refresh(); },
  });
  const rename = useMutation({
    mutationFn: ({ entry, name }: { entry: RemoteFileEntry; name: string }) => api.renameEntry(serverId, entry.path, join(path, name)),
    onSuccess: (_value, variables) => { const nextPath = join(path, variables.name); if (variables.entry.kind === "directory") persistBookmarks(replaceBookmarkPrefix(bookmarks, variables.entry.path, nextPath)); setInputDialog(null); setInputValidationError(null); showNotice({ kind: "success", title: "名称已更新", detail: `${variables.entry.name} → ${variables.name}` }); void refresh(); },
  });
  const remove = useMutation({
    mutationFn: (entry: RemoteFileEntry) => api.removeEntry(serverId, entry.path, entry.kind === "directory"),
    onSuccess: (_value, entry) => { if (entry.kind === "directory") persistBookmarks(removeBookmarksInside(bookmarks, entry.path)); setDeleteTarget(null); showNotice({ kind: "success", title: "已删除", detail: entry.path }); void refresh(); },
    onError: (reason) => showNotice({ kind: "error", title: "删除失败", detail: errorMessage(reason) }),
  });
  const chmodMutation = useMutation({
    mutationFn: ({ entry, mode }: { entry: RemoteFileEntry; mode: number }) => api.chmod({ serverId, path: entry.path, mode }),
    onSuccess: (_value, variables) => { setInputDialog(null); setInputValidationError(null); showNotice({ kind: "success", title: "权限已更新", detail: `${variables.entry.name} · ${variables.mode.toString(8).padStart(4, "0")}` }); void refresh(); },
  });
  const symlinkMutation = useMutation({
    mutationFn: ({ entry, linkPath }: { entry: RemoteFileEntry; linkPath: string }) => api.createSymlink({ serverId, targetPath: entry.path, linkPath }),
    onSuccess: (_value, variables) => { setInputDialog(null); setInputValidationError(null); showNotice({ kind: "success", title: "符号链接已创建", detail: variables.linkPath }); void refresh(); },
  });
  const copyMoveMutation = useMutation({
    mutationFn: ({ entry, destinationPath, operation }: { entry: RemoteFileEntry; destinationPath: string; operation: "copy" | "move" }) => api.copyMove({ serverId, sourcePath: entry.path, destinationPath, operation, recursive: entry.kind === "directory", confirmed: true }),
    onSuccess: (_value, variables) => { if (variables.operation === "move" && variables.entry.kind === "directory") persistBookmarks(replaceBookmarkPrefix(bookmarks, variables.entry.path, variables.destinationPath)); setFolderPicker(null); showNotice({ kind: "success", title: variables.operation === "copy" ? "复制完成" : "移动完成", detail: variables.destinationPath }); void queryClient.invalidateQueries({ queryKey: ["files", serverId] }); },
    onError: (reason) => showNotice({ kind: "error", title: "文件操作失败", detail: errorMessage(reason) }),
  });

  /** 打开文件或进入目录；打开过程会在对应行显示加载状态。 */
  const openEntry = async (entry: RemoteFileEntry) => {
    if (entry.kind === "directory") { navigatePath(entry.path); return; }
    await openRemotePath(entry.path);
  };

  /** 打开创建文件或文件夹对话框，避免使用浏览器原生输入框。 */
  const promptCreate = (directory: boolean) => { create.reset(); setInputValidationError(null); setInputDialog({ kind: directory ? "create-folder" : "create-file", value: "" }); };

  /** 打开重命名对话框，并保留当前对象用于提交和成功反馈。 */
  const promptRename = (entry: RemoteFileEntry) => { rename.reset(); setInputValidationError(null); setInputDialog({ kind: "rename", entry, value: entry.name }); };

  /** 打开权限编辑对话框，提交前验证八进制格式。 */
  const promptChmod = (entry: RemoteFileEntry) => { chmodMutation.reset(); setInputValidationError(null); setInputDialog({ kind: "chmod", entry, value: entry.permissions }); };

  /** 打开符号链接对话框，并把当前目录作为默认链接位置。 */
  const promptSymlink = (entry: RemoteFileEntry) => { symlinkMutation.reset(); setInputValidationError(null); setInputDialog({ kind: "symlink", entry, value: join(path, `${entry.name}.link`) }); };

  /** 打开多栏远程目录选择器，让用户选择目标文件夹而非手填完整路径。 */
  const promptCopyMove = (entry: RemoteFileEntry, operation: "copy" | "move") => { copyMoveMutation.reset(); setFolderPicker({ entry, operation }); };

  /** 提交统一输入对话框中的名称、权限或符号链接路径。 */
  const submitInputDialog = (value: string) => {
    const next = value.trim();
    if (!inputDialog || !next) return;
    if (inputDialog.kind === "chmod" && !/^[0-7]{3,4}$/.test(next)) { setInputValidationError("权限必须是 3 或 4 位八进制数字，例如 0644"); return; }
    setInputValidationError(null);
    if (inputDialog.kind === "create-folder" || inputDialog.kind === "create-file") create.mutate({ name: next, directory: inputDialog.kind === "create-folder" });
    else if (inputDialog.kind === "rename" && inputDialog.entry && next !== inputDialog.entry.name) rename.mutate({ entry: inputDialog.entry, name: next });
    else if (inputDialog.kind === "chmod" && inputDialog.entry) chmodMutation.mutate({ entry: inputDialog.entry, mode: Number.parseInt(next, 8) });
    else if (inputDialog.kind === "symlink" && inputDialog.entry) symlinkMutation.mutate({ entry: inputDialog.entry, linkPath: next });
    else setInputDialog(null);
  };

  /** 创建本地上传传输任务，并把远程刷新交给传输完成回调。 */
  const startUpload = useCallback((localPath: string) => {
    const id = crypto.randomUUID();
    useTransferStore.getState().add({ id, label: localPath.split(/[\\/]/).pop() ?? localPath, direction: "upload", status: "queued", transferredBytes: 0, totalBytes: null, bytesPerSecond: 0, currentPath: path, retry: () => startUpload(localPath) });
    void api.upload(id, serverId, localPath, path, uploadConflict, (event) => useTransferStore.getState().event(event)).then(refresh).catch((reason) => { if (!isAppError(reason) || reason.code !== "CANCELLED") useTransferStore.getState().fail(id, errorMessage(reason)); });
  }, [path, refresh, serverId, uploadConflict]);

  /** 打开系统文件选择器并为每个本地文件创建独立传输任务。 */
  const chooseUpload = async () => { const paths = await open({ multiple: true, title: "选择要上传的文件" }); if (paths) (Array.isArray(paths) ? paths : [paths]).forEach(startUpload); };

  /** 选择本地目录并交给 Rust 传输层递归上传。 */
  const chooseUploadFolder = async () => { const folder = await open({ directory: true, recursive: true, title: "选择要上传的文件夹" }); if (typeof folder === "string") startUpload(folder); };

  /** 启动一次下载并把同一目标目录保存在任务重试回调中。 */
  const runDownload = useCallback((entry: RemoteFileEntry, directory: string) => {
    const id = crypto.randomUUID();
    useTransferStore.getState().add({ id, label: entry.name, direction: "download", status: "queued", transferredBytes: 0, totalBytes: null, bytesPerSecond: 0, currentPath: entry.path, retry: () => runDownload(entry, directory) });
    void api.download(id, serverId, entry.path, directory, (event) => useTransferStore.getState().event(event)).catch((reason) => { if (!isAppError(reason) || reason.code !== "CANCELLED") useTransferStore.getState().fail(id, errorMessage(reason)); });
  }, [serverId]);

  /** 选择本地目录并开始下载；目录路径用于后续重试。 */
  const startDownload = async (entry: RemoteFileEntry) => { const directory = await open({ directory: true, recursive: true, title: `选择“${entry.name}”的下载位置` }); if (!directory || Array.isArray(directory)) return; runDownload(entry, directory); };

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "over") setDropActive(true);
      else if (event.payload.type === "leave") setDropActive(false);
      else if (event.payload.type === "drop") { setDropActive(false); event.payload.paths.forEach(startUpload); }
    }).then((value) => { unlisten = value; });
    return () => unlisten?.();
  }, [startUpload]);

  /** 处理文件列表的快捷键，保持 F2、Delete 和全选符合桌面文件管理器习惯。 */
  const onKeyDown = (event: ReactKeyboardEvent) => {
    const current = entries.find((entry) => selected.has(entry.path));
    if (event.key === "F2" && current) { event.preventDefault(); promptRename(current); }
    if (event.key === "Delete" && current) { event.preventDefault(); setDeleteTarget(current); }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a") { event.preventDefault(); setSelected(new Set(entries.map((entry) => entry.path))); }
  };

  const inputDialogTitle = inputDialog?.kind === "create-folder" ? "新建文件夹" : inputDialog?.kind === "create-file" ? "新建文件" : inputDialog?.kind === "rename" ? "重命名" : inputDialog?.kind === "chmod" ? "修改权限" : "创建符号链接";
  const inputDialogDescription = inputDialog?.kind === "create-folder" ? `将在 ${path} 中创建一个新文件夹` : inputDialog?.kind === "create-file" ? `将在 ${path} 中创建一个空文件` : inputDialog?.kind === "rename" ? `为“${inputDialog.entry?.name ?? ""}”输入新的名称` : inputDialog?.kind === "chmod" ? "输入 3 或 4 位八进制权限，例如 0644" : "符号链接会指向当前选中的远程对象";
  const inputDialogLabel = inputDialog?.kind === "chmod" ? "权限（八进制）" : inputDialog?.kind === "symlink" ? "链接路径" : "名称";
  const inputDialogError = inputValidationError ?? (inputDialog?.kind === "create-folder" || inputDialog?.kind === "create-file" ? create.error : inputDialog?.kind === "rename" ? rename.error : inputDialog?.kind === "chmod" ? chmodMutation.error : inputDialog?.kind === "symlink" ? symlinkMutation.error : null);
  const inputDialogPending = inputDialog?.kind === "create-folder" || inputDialog?.kind === "create-file" ? create.isPending : inputDialog?.kind === "rename" ? rename.isPending : inputDialog?.kind === "chmod" ? chmodMutation.isPending : inputDialog?.kind === "symlink" ? symlinkMutation.isPending : false;
  const renameBookmarkImpact = inputDialog?.kind === "rename" && inputDialog.entry?.kind === "directory" ? bookmarks.filter((bookmark) => isPathInside(inputDialog.entry?.path ?? "", bookmark.path)) : [];
  const deleteBookmarkImpact = deleteTarget?.kind === "directory" ? bookmarks.filter((bookmark) => isPathInside(deleteTarget.path, bookmark.path)) : [];
  const moveBookmarkImpact = folderPicker?.operation === "move" && folderPicker.entry.kind === "directory" ? bookmarks.filter((bookmark) => isPathInside(folderPicker.entry.path, bookmark.path)) : [];
  const bookmarkDialogTitle = bookmarkDialog?.mode === "rename" ? "重命名书签" : "添加文件夹书签";

  return <section className="files-page" onKeyDown={onKeyDown} tabIndex={-1}>
    <div className="workspace-header terminal-header"><div><div className="breadcrumb">服务器 / {profile.data?.name ?? "…"} / <span>文件</span></div><h1>远程文件</h1><p>SFTP · {profile.data ? `${profile.data.username}@${profile.data.host}` : "正在载入"}</p></div><div className="workspace-header__actions file-primary-actions"><Button variant="secondary" onClick={chooseUpload}><Upload size={14} /> 上传文件</Button><Button variant="secondary" onClick={chooseUploadFolder}><FolderPlus size={14} /> 上传文件夹</Button><Button variant="secondary" onClick={() => promptCreate(true)}><FolderPlus size={14} /> 新建文件夹</Button><Button variant="primary" onClick={() => promptCreate(false)}><FilePlus2 size={14} /> 新建文件</Button></div></div>
    <nav className="workspace-tabs"><NavLink end to={`/servers/${serverId}`}>概览</NavLink><NavLink className="active" to={`/servers/${serverId}/files`}>文件</NavLink><NavLink to={`/servers/${serverId}/terminal`}>终端</NavLink><NavLink to={`/servers/${serverId}/operations`}>端口与进程</NavLink><NavLink to={`/servers/${serverId}/services`}>服务</NavLink><NavLink to={`/servers/${serverId}/tools`}>工具</NavLink><NavLink to={`/servers/${serverId}/logs`}>日志</NavLink><NavLink to={`/servers/${serverId}/nginx`}>Nginx</NavLink><NavLink to={`/servers/${serverId}/docker`}>Docker</NavLink></nav>
    <div className="file-toolbar"><div className="file-history"><button disabled={historyIndex === 0} onClick={() => { const index = historyIndex - 1; setHistoryIndex(index); navigatePath(history[index], false); }}><ArrowLeft size={14} /></button><button disabled={historyIndex >= history.length - 1} onClick={() => { const index = historyIndex + 1; setHistoryIndex(index); navigatePath(history[index], false); }}><ArrowRight size={14} /></button><button disabled={path === "/"} onClick={() => navigatePath(parent(path))}><ArrowUp size={14} /></button><button onClick={() => void refresh()}><RefreshCw size={14} /></button></div><form className="path-input" onSubmit={(event) => { event.preventDefault(); navigatePath(pathInput); }}><FolderUp size={14} /><input value={pathInput} onChange={(event) => setPathInput(event.target.value)} /><button>转到</button></form><label className="file-filter"><Search size={13} /><input value={filter} onChange={(event) => setFilter(event.target.value)} placeholder="筛选当前目录" /></label><div className="file-selection-actions"><span>{selectedEntries.length ? `已选 ${selectedEntries.length} 项` : "未选择文件"}</span><Button size="sm" variant="ghost" disabled={!selectedEntry} onClick={() => selectedEntry && void startDownload(selectedEntry)}><ArrowDownToLine size={13} /> 下载</Button><Button size="sm" variant="ghost" disabled={!selectedEntry} onClick={() => selectedEntry && promptRename(selectedEntry)}><Pencil size={13} /> 重命名</Button><Button size="sm" variant="ghost" disabled={!selectedEntry} onClick={() => selectedEntry && promptCopyMove(selectedEntry, "copy")}>复制</Button><Button size="sm" variant="ghost" disabled={!selectedEntry} onClick={() => selectedEntry && promptCopyMove(selectedEntry, "move")}>移动</Button><Button size="sm" variant="danger" disabled={!selectedEntry} onClick={() => selectedEntry && setDeleteTarget(selectedEntry)}><Trash2 size={13} /> 删除</Button></div><label className="hidden-toggle"><input type="checkbox" checked={showHidden} onChange={(event) => setShowHidden(event.target.checked)} /> 隐藏文件</label><label className="hidden-toggle">冲突<select value={uploadConflict} onChange={(event) => setUploadConflict(event.target.value as "replace" | "skip" | "rename")}><option value="replace">替换</option><option value="skip">跳过</option><option value="rename">重命名</option></select></label></div>
    {notice && <div className={`file-feedback file-feedback--${notice.kind}`} role="status"><span>{notice.kind === "success" ? <CheckCircle2 size={17} /> : <X size={17} />}</span><div><strong>{notice.title}</strong>{notice.detail && <p>{notice.detail}</p>}</div><button type="button" onClick={() => setNotice(null)} aria-label="关闭提示"><X size={15} /></button></div>}
    <div className="file-bookmarks-bar"><div className="file-bookmarks__label"><Bookmark size={14} /> 书签</div>{bookmarks.length ? bookmarks.map((bookmark) => <div className={`file-bookmark-chip ${bookmark.path === path ? "is-current" : ""}`} key={bookmark.id}><button type="button" onClick={() => void goBookmark(bookmark)} title={bookmark.path}><Star size={12} />{bookmark.name}</button><button type="button" className="file-bookmark-chip__remove" onClick={() => removeBookmark(bookmark)} aria-label={`移除书签 ${bookmark.name}`}><X size={12} /></button></div>) : <span className="file-bookmarks__empty">将常用目录固定在这里，随时一键返回</span>}<Button size="sm" variant="ghost" onClick={promptBookmark}><Star size={13} />{currentBookmark ? "编辑当前书签" : "添加当前目录"}</Button>{bookmarks.length > 0 && <Button size="sm" variant="ghost" onClick={() => setBookmarkManagerOpen(true)}>管理</Button>}</div>
    <div className="file-browser"><div className="file-workspace"><div className="file-main-pane"><div className="file-columns"><button onClick={() => setSort("name")}>名称</button><button onClick={() => setSort("size")}>大小</button><span>类型</span><span>权限</span><span>所有者</span><button onClick={() => setSort("modifiedAt")}>修改时间</button></div><div className="file-list" ref={listRef}>
      {listing.isLoading && <div className="file-state"><LoaderCircle className="spin" size={16} />正在读取远程目录…</div>}{listing.error && <div className="file-state is-error"><span>{errorMessage(listing.error)}</span><Button size="sm" onClick={() => void refresh()}>重试</Button></div>}{!listing.isLoading && !listing.error && !entries.length && <div className="file-state file-state--empty"><Folder size={28} /><strong>此目录为空</strong><span>可以直接创建内容，或从本机上传文件。</span><div><Button size="sm" variant="secondary" onClick={() => promptCreate(true)}><FolderPlus size={13} /> 新建文件夹</Button><Button size="sm" variant="primary" onClick={() => promptCreate(false)}><FilePlus2 size={13} /> 新建文件</Button><Button size="sm" variant="ghost" onClick={chooseUpload}><Upload size={13} /> 上传文件</Button></div></div>}
      <div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}>{virtualizer.getVirtualItems().map((item) => { const entry = entries[item.index]; return <ContextMenu.Root key={entry.path}><ContextMenu.Trigger asChild><div className={`file-row ${selected.has(entry.path) ? "is-selected" : ""}`} style={{ transform: `translateY(${item.start}px)` }} onClick={(event) => { setPreviewOpen(true); if (event.ctrlKey || event.metaKey) setSelected((current) => { const next = new Set(current); if (next.has(entry.path)) next.delete(entry.path); else next.add(entry.path); return next; }); else setSelected(new Set([entry.path])); }} onDoubleClick={() => void openEntry(entry)}><span className="file-name">{fileIcon(entry, openingPath === entry.path)}<strong>{entry.name}</strong></span><span>{entry.kind === "directory" ? "—" : formatBytes(entry.size)}</span><span>{entry.kind === "directory" ? "文件夹" : entry.kind === "symlink" ? "符号链接" : "文件"}</span><span className="mono">{entry.permissions}</span><span>{entry.owner}:{entry.group}</span><span>{entry.modifiedAt ? new Date(entry.modifiedAt * 1000).toLocaleString() : "—"}</span></div></ContextMenu.Trigger><ContextMenu.Portal><ContextMenu.Content className="file-context"><ContextMenu.Item onSelect={() => void openEntry(entry)}>{entry.kind === "directory" ? <Folder size={13} /> : <FileText size={13} />} 打开</ContextMenu.Item><ContextMenu.Item onSelect={() => promptRename(entry)}><Pencil size={13} /> 重命名 <kbd>F2</kbd></ContextMenu.Item><ContextMenu.Item onSelect={() => promptChmod(entry)}><Pencil size={13} /> 修改权限（chmod）</ContextMenu.Item><ContextMenu.Item onSelect={() => promptSymlink(entry)}><Link2 size={13} /> 创建符号链接</ContextMenu.Item><ContextMenu.Item onSelect={() => promptCopyMove(entry, "copy")}><FilePlus2 size={13} /> 复制到…</ContextMenu.Item><ContextMenu.Item onSelect={() => promptCopyMove(entry, "move")}><ArrowRight size={13} /> 移动到…</ContextMenu.Item><ContextMenu.Item onSelect={() => void startDownload(entry)}><ArrowDownToLine size={13} /> 下载</ContextMenu.Item><ContextMenu.Separator /><ContextMenu.Item className="is-danger" onSelect={() => setDeleteTarget(entry)}><Trash2 size={13} /> 删除 <kbd>Del</kbd></ContextMenu.Item></ContextMenu.Content></ContextMenu.Portal></ContextMenu.Root>; })}</div>
    </div></div>{previewOpen && previewEntry && <FilePreviewPanel entry={previewEntry} query={previewQuery} onClose={() => setPreviewOpen(false)} />}</div><footer className="file-status"><span>{entries.length} 项 · 已选 {selected.size} 项</span><span>单击预览 · 双击打开 · F2 重命名 · Delete 删除</span></footer>{dropActive && <div className="file-drop"><Upload size={28} /><strong>上传到 {path}</strong><span>释放即可开始递归传输</span></div>}</div>
    {editorFile && <FileEditor file={editorFile} value={editorValue} dirty={editorDirty} canSudo={profile.data?.sudoMode !== "none"} onChange={(value) => { setEditorValue(value); setEditorDirty(value !== editorFile.content); }} onClose={() => setEditorFile(null)} onSaved={(file) => { setEditorFile(file); setEditorValue(file.content); setEditorDirty(false); void refresh(); }} />}
    <FileModal open={!!inputDialog} onOpenChange={(opened) => !opened && setInputDialog(null)} title={inputDialogTitle ?? "文件操作"} description={inputDialogDescription} icon={<FilePlus2 size={18} />}>
      <form onSubmit={(event) => { event.preventDefault(); submitInputDialog(inputDialog?.value ?? ""); }}><label className="file-form-field"><span>{inputDialogLabel}</span><input autoFocus value={inputDialog?.value ?? ""} onChange={(event) => { setInputValidationError(null); setInputDialog((current) => current ? { ...current, value: event.target.value } : current); }} placeholder={inputDialog?.kind === "chmod" ? "0644" : undefined} /></label>{renameBookmarkImpact.length > 0 && <div className="file-impact-warning"><AlertCircle size={16} /><span>此文件夹下有 {renameBookmarkImpact.length} 个书签，重命名后书签路径会自动同步。</span></div>}{inputDialogError && <div className="form-error file-modal-error" role="alert">{errorMessage(inputDialogError)}</div>}<div className="dialog-actions"><Button type="button" variant="ghost" onClick={() => setInputDialog(null)}>取消</Button><Button type="submit" variant="primary" disabled={!inputDialog?.value.trim() || inputDialogPending}>{inputDialogPending ? <><LoaderCircle className="spin" size={14} />处理中…</> : "确认"}</Button></div></form>
    </FileModal>
    {folderPicker && <RemoteFolderPickerDialog key={`${folderPicker.entry.path}:${folderPicker.operation}`} open serverId={serverId} initialPath={path} source={folderPicker.entry} operation={folderPicker.operation} bookmarkImpactCount={moveBookmarkImpact.length} onOpenChange={(opened) => !opened && setFolderPicker(null)} pending={copyMoveMutation.isPending} onConfirm={({ destinationPath }) => copyMoveMutation.mutate({ entry: folderPicker.entry, destinationPath, operation: folderPicker.operation })} />}
    <FileModal open={bookmarkManagerOpen} onOpenChange={setBookmarkManagerOpen} title="管理文件夹书签" description="书签仅保存在当前桌面用户中，并按服务器独立隔离。" icon={<Bookmark size={18} />} className="file-bookmark-manager-dialog"><div className="file-bookmark-manager-list">{bookmarks.map((bookmark) => <div className="file-bookmark-manager-row" key={bookmark.id}><button type="button" onClick={() => void goBookmark(bookmark)}><Star size={14} /><span><strong>{bookmark.name}</strong><small className="mono">{bookmark.path}</small></span></button><Button size="sm" variant="ghost" onClick={() => { setBookmarkManagerOpen(false); promptRenameBookmark(bookmark); }}><Pencil size={13} /> 重命名</Button><Button size="sm" variant="danger" onClick={() => removeBookmark(bookmark)}><Trash2 size={13} /> 移除</Button></div>)}</div><div className="dialog-actions"><Button variant="secondary" onClick={() => { setBookmarkManagerOpen(false); promptBookmark(); }}><Star size={13} /> 添加当前目录</Button><Button variant="ghost" onClick={() => setBookmarkManagerOpen(false)}>完成</Button></div></FileModal>
    <FileModal open={!!bookmarkDialog} onOpenChange={(opened) => !opened && setBookmarkDialog(null)} title={bookmarkDialogTitle} description={bookmarkDialog?.mode === "rename" ? `修改“${bookmarkDialog.bookmark?.path ?? ""}”的显示名称` : `将当前目录 ${path} 保存为快速入口`} icon={<Star size={18} />} className="file-bookmark-dialog"><form onSubmit={(event) => { event.preventDefault(); submitBookmark(bookmarkDialog?.value ?? ""); }}><label className="file-form-field"><span>书签名称</span><input autoFocus value={bookmarkDialog?.value ?? ""} onChange={(event) => setBookmarkDialog((current) => current ? { ...current, value: event.target.value } : current)} placeholder="例如：项目根目录" /></label><div className="file-bookmark-path mono">{bookmarkDialog?.bookmark?.path ?? path}</div><div className="dialog-actions"><Button type="button" variant="ghost" onClick={() => setBookmarkDialog(null)}>取消</Button><Button type="submit" variant="primary" disabled={!bookmarkDialog?.value.trim()}>保存书签</Button></div></form></FileModal>
    <FileModal open={!!largeViewer} onOpenChange={(opened) => !opened && setLargeViewer(null)} title="大文件查看" description={`${largeViewer?.path ?? ""} · 显示最后 5000 行 · 文件总大小 ${largeViewer ? formatBytes(largeViewer.size) : "—"}`} icon={<FileText size={18} />} className="docker-logs-dialog"><pre className="docker-logs large-file-viewer">{largeViewer?.content}</pre></FileModal>
    <FileModal open={!!deleteTarget} onOpenChange={(opened) => !opened && setDeleteTarget(null)} title={`删除“${deleteTarget?.name ?? ""}”`} description={deleteTarget?.kind === "directory" ? "将递归删除该文件夹及其全部内容。删除后无法从本应用恢复。" : "将永久删除该远程文件。删除后无法从本应用恢复。"} icon={<Trash2 size={18} />} className="file-confirm-dialog"><div className="file-confirm-copy"><strong>{deleteTarget?.path}</strong><span>这是不可逆操作，请确认你选择的是正确的远程对象。</span></div>{deleteBookmarkImpact.length > 0 && <div className="file-impact-warning"><AlertCircle size={16} /><span>该文件夹下有 {deleteBookmarkImpact.length} 个书签，确认删除后这些书签会一并移除。</span></div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => setDeleteTarget(null)}>取消</Button><Button variant="danger" onClick={() => deleteTarget && remove.mutate(deleteTarget)} disabled={remove.isPending}>{remove.isPending ? <><LoaderCircle className="spin" size={14} />删除中…</> : "确认删除"}</Button></div></FileModal>
  </section>;
}

/** 展示当前选中文件或文件夹的右侧预览，单击列表行即可触发加载。 */
function FilePreviewPanel({ entry, query, onClose }: { entry: RemoteFileEntry; query: { isLoading: boolean; error: unknown; data?: FilePreview }; onClose: () => void }) {
  const preview = query.data;
  const folderEntries = preview?.kind === "directory" ? [...preview.entries].sort((left, right) => { if (left.kind === "directory" && right.kind !== "directory") return -1; if (right.kind === "directory" && left.kind !== "directory") return 1; return left.name.localeCompare(right.name, "zh-CN", { numeric: true }); }) : [];
  return <aside className="file-preview-panel"><header className="file-preview-panel__header"><div><span className="file-preview-eyebrow"><Eye size={13} /> 快速预览</span><strong title={entry.path}>{entry.name}</strong></div><button type="button" className="icon-control" onClick={onClose} aria-label="关闭预览"><PanelRightClose size={16} /></button></header><div className="file-preview-panel__body">{query.isLoading && <div className="file-preview-state"><LoaderCircle className="spin" size={18} /><span>正在读取预览…</span></div>}{Boolean(query.error) && <div className="file-preview-state is-error"><AlertCircle size={20} /><strong>预览失败</strong><span>{errorMessage(query.error)}</span></div>}{!query.isLoading && !query.error && preview?.kind === "image" && <div className="file-preview-image"><img src={preview.dataUrl} alt={entry.name} /><span>{formatBytes(preview.size)} · 图片</span></div>}{!query.isLoading && !query.error && preview?.kind === "text" && <div className="file-preview-text"><div className="file-preview-meta"><FileText size={14} /> 文本内容 · {formatBytes(preview.size)}</div><pre>{preview.content || "（空文件）"}</pre></div>}{!query.isLoading && !query.error && preview?.kind === "directory" && <div className="file-preview-directory"><div className="file-preview-meta"><Folder size={14} /> 第一层内容 · {folderEntries.length} 项</div>{folderEntries.length ? folderEntries.map((child) => <div className="file-preview-directory__row" key={child.path}>{fileIcon(child, false)}<span>{child.name}</span><small>{child.kind === "directory" ? "文件夹" : formatBytes(child.size)}</small></div>) : <div className="file-preview-state"><Folder size={22} /><span>此文件夹为空</span></div>}</div>}{!query.isLoading && !query.error && preview?.kind === "unsupported" && <div className="file-preview-state"><AlertCircle size={22} /><strong>暂不支持预览</strong><span>{preview.message}</span></div>}{!query.isLoading && !query.error && preview?.kind === "error" && <div className="file-preview-state is-error"><AlertCircle size={22} /><strong>读取失败</strong><span>{preview.message}</span></div>}</div><footer className="file-preview-panel__footer"><span className="mono">{entry.path}</span><span>双击打开完整内容</span></footer></aside>;
}

/** 编辑远程文本，提供保存前冲突保护、比较、重新加载和可确认退出。 */
function FileEditor({ file, value, dirty, canSudo, onChange, onClose, onSaved }: { file: RemoteTextFile; value: string; dirty: boolean; canSudo: boolean; onChange: (value: string) => void; onClose: () => void; onSaved: (file: RemoteTextFile) => void }) {
  const { serverId = "" } = useParams();
  const [conflict, setConflict] = useState<"normal" | "sudo" | null>(null);
  const [compareContent, setCompareContent] = useState<string | null>(null);
  const [confirmClose, setConfirmClose] = useState(false);
  const [pendingReload, setPendingReload] = useState<RemoteTextFile | null>(null);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);
  const save = useMutation<RemoteTextFile, unknown, boolean>({ mutationFn: (force) => api.saveText({ serverId, path: file.path, content: value, expectedSize: file.size, expectedModifiedAt: file.modifiedAt, force }), onSuccess: (saved) => { setConflict(null); setSaveMessage("已保存到远程服务器"); onSaved(saved); }, onError: (reason) => { if (isAppError(reason) && reason.code === "FILE_CONFLICT") setConflict("normal"); } });
  const sudoSave = useMutation<RemoteTextFile, unknown, boolean>({ mutationFn: (force) => api.saveTextPrivileged({ serverId, path: file.path, content: value, expectedSize: file.size, expectedModifiedAt: file.modifiedAt, force }), onSuccess: (saved) => { setConflict(null); setSaveMessage("已使用 sudo 保存"); onSaved(saved); }, onError: (reason) => { if (isAppError(reason) && reason.code === "FILE_CONFLICT") setConflict("sudo"); } });
  const reload = useMutation({ mutationFn: () => api.readText(serverId, file.path), onSuccess: (latest) => { if (dirty) setPendingReload(latest); else onSaved(latest); } });
  const compare = useMutation({ mutationFn: () => api.readText(serverId, file.path), onSuccess: (latest) => setCompareContent(latest.content) });

  /** 关闭编辑器前决定是否放弃当前未保存内容。 */
  const requestClose = () => { if (dirty) setConfirmClose(true); else onClose(); };

  /** 应用用户确认的最新远程内容，并清空比较和待确认状态。 */
  const applyReload = (latest: RemoteTextFile) => { setPendingReload(null); setCompareContent(null); onSaved(latest); };

  /** 关闭编辑器内部错误条，保留 mutation 状态以便用户重新尝试。 */
  const dismissEditorError = () => { save.reset(); sudoSave.reset(); reload.reset(); compare.reset(); };

  useEffect(() => { const handler = (event: KeyboardEvent) => { if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") { event.preventDefault(); if (dirty) save.mutate(false); } }; window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler); }, [dirty, save]);
  const editorError = save.error ?? sudoSave.error ?? reload.error ?? compare.error;
  return <div className="editor-overlay"><div className="editor-window"><header><div className="editor-heading"><FileText size={16} /><strong>{file.path}</strong>{dirty && <i>未保存</i>}{saveMessage && <span className="editor-saved">{saveMessage}</span>}</div><div className="editor-actions"><Button variant="ghost" size="sm" onClick={() => compare.mutate()} disabled={compare.isPending}>{compare.isPending ? "读取中…" : "比较"}</Button><Button variant="ghost" size="sm" onClick={() => reload.mutate()} disabled={reload.isPending}>{reload.isPending ? "重新加载中…" : "重新加载"}</Button>{canSudo && <Button variant="ghost" size="sm" onClick={() => sudoSave.mutate(false)} disabled={!dirty || sudoSave.isPending}>sudo 保存</Button>}<Button size="sm" onClick={() => save.mutate(false)} disabled={!dirty || save.isPending}><Save size={13} /> {save.isPending ? "保存中…" : "保存"}</Button><button className="editor-close" onClick={requestClose} aria-label="关闭编辑器"><X size={16} /></button></div></header>{editorError && <div className="editor-error" role="alert"><span>{errorMessage(editorError)}</span><button onClick={dismissEditorError} aria-label="关闭错误"><X size={14} /></button></div>}<Editor height="100%" path={file.path} value={value} loading={<div className="editor-loading"><LoaderCircle className="spin" size={18} />正在启动编辑器…</div>} onChange={(next) => onChange(next ?? "")} theme="vs-dark" options={{ minimap: { enabled: false }, wordWrap: "on", fontSize: 13, automaticLayout: true, scrollBeyondLastLine: false }} /></div>{conflict && <div className="editor-conflict"><strong>远程文件已改变</strong><p>保存已被阻止，避免覆盖其他程序的修改。可关闭并重新打开查看最新内容，或明确选择强制覆盖。</p><div><Button variant="ghost" onClick={() => setConflict(null)}>继续比较</Button><Button variant="danger" onClick={() => conflict === "sudo" ? sudoSave.mutate(true) : save.mutate(true)}>强制覆盖</Button></div></div>}<FileModal open={confirmClose} onOpenChange={setConfirmClose} title="放弃未保存的修改？" description="当前内容还没有写回远程服务器。" icon={<X size={18} />} className="file-confirm-dialog"><div className="file-confirm-copy"><strong>{file.path}</strong><span>关闭后本次编辑内容会丢失。</span></div><div className="dialog-actions"><Button variant="ghost" onClick={() => setConfirmClose(false)}>继续编辑</Button><Button variant="danger" onClick={onClose}>放弃并关闭</Button></div></FileModal><FileModal open={pendingReload !== null} onOpenChange={(open) => !open && setPendingReload(null)} title="替换当前编辑内容？" description="重新加载会用远程最新版本覆盖当前未保存内容。" icon={<RotateCw size={18} />} className="file-confirm-dialog"><div className="file-confirm-copy"><strong>{file.path}</strong><span>如果你还需要保留本地修改，请先取消并保存或比较。</span></div><div className="dialog-actions"><Button variant="ghost" onClick={() => setPendingReload(null)}>取消</Button><Button variant="primary" onClick={() => pendingReload && applyReload(pendingReload)}>用最新内容替换</Button></div></FileModal><FileModal open={compareContent !== null} onOpenChange={(open) => !open && setCompareContent(null)} title="远程 Compare" description="左侧是当前编辑内容，右侧是刚刚读取的远程内容。" icon={<RotateCw size={18} />} className="compare-dialog"><div className="compare-editors"><Editor height="420px" language="plaintext" value={value} theme="vs-dark" options={{ readOnly: true, minimap: { enabled: false }, wordWrap: "on" }} /><Editor height="420px" language="plaintext" value={compareContent ?? ""} theme="vs-dark" options={{ readOnly: true, minimap: { enabled: false }, wordWrap: "on" }} /></div></FileModal></div>;
}
