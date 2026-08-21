import * as Dialog from "@radix-ui/react-dialog";
import { useQueries } from "@tanstack/react-query";
import { Check, ChevronRight, Folder, FolderOpen, LoaderCircle, RotateCw, X } from "lucide-react";
import { type FormEvent, type ReactNode, useMemo, useState } from "react";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import type { RemoteFileEntry } from "../../types/server";

type FileModalProps = {
  open: boolean;
  title: string;
  description?: string;
  icon?: ReactNode;
  className?: string;
  onOpenChange: (open: boolean) => void;
  children: ReactNode;
};

/** 渲染文件模块统一的标题、说明、关闭按钮和内容容器。 */
export function FileModal({ open, title, description, icon, className, onOpenChange, children }: FileModalProps) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay file-dialog-overlay" />
        <Dialog.Content className={`dialog-content file-modal${className ? ` ${className}` : ""}`}>
          <header className="file-modal__header">
            <div className="file-modal__heading">
              {icon && <span className="file-modal__icon">{icon}</span>}
              <div>
                <Dialog.Title>{title}</Dialog.Title>
                {description && <Dialog.Description>{description}</Dialog.Description>}
              </div>
            </div>
            <Dialog.Close asChild><button className="icon-control file-modal__close" aria-label="关闭"><X size={17} /></button></Dialog.Close>
          </header>
          <div className="file-modal__body">{children}</div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

type FolderPickerResult = { directory: string; name: string; destinationPath: string };

type RemoteFolderPickerDialogProps = {
  open: boolean;
  serverId: string;
  initialPath: string;
  source: RemoteFileEntry;
  operation: "copy" | "move";
  onOpenChange: (open: boolean) => void;
  onConfirm: (result: FolderPickerResult) => void;
  pending?: boolean;
  bookmarkImpactCount?: number;
};

/** 规范化远程 POSIX 路径，避免目录选择器生成双斜线或空路径。 */
function normalizeRemotePath(value: string) {
  const clean = value.trim().replace(/\\/g, "/").replace(/\/+/g, "/");
  if (!clean || clean === ".") return "/";
  const rooted = clean.startsWith("/") ? clean : `/${clean}`;
  return rooted.length > 1 ? rooted.replace(/\/+$/, "") : rooted;
}

/** 将远程目录路径拆成从根目录向右展开的多栏路径链。 */
function pathChain(value: string) {
  const path = normalizeRemotePath(value);
  if (path === "/") return ["/"];
  const segments = path.split("/").filter(Boolean);
  return ["/", ...segments.map((_, index) => `/${segments.slice(0, index + 1).join("/")}`)];
}

/** 组合远程目录和名称，返回可直接交给 Copy/Move API 的完整目标路径。 */
function joinRemotePath(directory: string, name: string) {
  const base = normalizeRemotePath(directory);
  return base === "/" ? `/${name}` : `${base}/${name}`;
}

/** 判断目标路径是否等于源路径或位于源目录内部，提前阻止无效目录操作。 */
function isSameOrInside(source: string, target: string) {
  const left = normalizeRemotePath(source);
  const right = normalizeRemotePath(target);
  return right === left || right.startsWith(`${left}/`);
}

/** 展示可横向浏览的远程目录列，并返回用户选中的目标目录和名称。 */
export function RemoteFolderPickerDialog({ open, serverId, initialPath, source, operation, onOpenChange, onConfirm, pending = false, bookmarkImpactCount = 0 }: RemoteFolderPickerDialogProps) {
  const [columns, setColumns] = useState<string[]>(() => pathChain(initialPath));
  const [selectedPath, setSelectedPath] = useState(() => normalizeRemotePath(initialPath));
  const [name, setName] = useState(() => operation === "copy" ? `${source.name}.copy` : source.name);
  const [error, setError] = useState<string | null>(null);
  const [bookmarkImpactAcknowledged, setBookmarkImpactAcknowledged] = useState(false);

  const listings = useQueries({
    queries: columns.map((folderPath) => ({
      queryKey: ["file-picker", serverId, folderPath],
      queryFn: () => api.listDirectory(serverId, folderPath),
      enabled: open && Boolean(serverId),
    })),
  });
  const destinationPath = useMemo(() => joinRemotePath(selectedPath, name.trim()), [name, selectedPath]);
  const actionLabel = operation === "copy" ? "复制" : "移动";
  const requiresBookmarkAck = operation === "move" && bookmarkImpactCount > 0;

  /** 选中一栏中的目录，并在右侧追加该目录的下一层内容。 */
  const selectDirectory = (columnIndex: number, entry: RemoteFileEntry) => {
    setColumns([...columns.slice(0, columnIndex + 1), entry.path]);
    setSelectedPath(normalizeRemotePath(entry.path));
    setError(null);
  };

  /** 选中某一栏的目录标题，让用户可以把文件放在该目录本身。 */
  const selectColumn = (columnIndex: number) => {
    setColumns(columns.slice(0, columnIndex + 1));
    setSelectedPath(normalizeRemotePath(columns[columnIndex]));
    setError(null);
  };

  /** 校验目标路径并把最终目录、名称交给文件页执行远程操作。 */
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!name.trim()) { setError("请输入目标名称"); return; }
    if (isSameOrInside(source.path, destinationPath)) { setError("目标不能与源路径相同，也不能放在源文件夹内部"); return; }
    if (requiresBookmarkAck && !bookmarkImpactAcknowledged) { setError("请确认移动后会同步更新受影响的书签"); return; }
    onConfirm({ directory: selectedPath, name: name.trim(), destinationPath });
  };

  return (
    <FileModal open={open} onOpenChange={onOpenChange} title={`${actionLabel}到…`} description={`为“${source.name}”选择目标文件夹；目标已存在时不会覆盖。`} icon={operation === "copy" ? <FolderOpen size={18} /> : <ChevronRight size={18} />} className="file-folder-picker-dialog">
      <form className="folder-picker-form" onSubmit={submit}>
        <div className="folder-picker-toolbar">
          <div><span className="section-kicker">目标位置</span><strong className="mono">{selectedPath}</strong></div>
          <span className="folder-picker-hint">从左向右展开目录</span>
        </div>
        <div className="folder-picker-columns" aria-label="远程目录选择器">
          {columns.map((folderPath, index) => {
            const query = listings[index];
            const directories = (query.data?.entries ?? []).filter((entry) => entry.kind === "directory").sort((left, right) => left.name.localeCompare(right.name, "zh-CN", { numeric: true }));
            return <section className={`folder-picker-column ${selectedPath === folderPath ? "is-selected" : ""}`} key={folderPath}>
              <button type="button" className="folder-picker-column__title" onClick={() => selectColumn(index)}><Folder size={14} /><span>{folderPath === "/" ? "根目录" : folderPath.split("/").pop()}</span><Check size={14} /></button>
              <div className="folder-picker-column__list">
                {query.isLoading && <div className="folder-picker-state"><LoaderCircle className="spin" size={15} />读取中</div>}
                {query.error && <div className="folder-picker-state is-error"><span>读取失败</span><Button type="button" size="sm" variant="ghost" onClick={() => void query.refetch()}><RotateCw size={13} />重试</Button></div>}
                {!query.isLoading && !query.error && !directories.length && <div className="folder-picker-state">没有子文件夹</div>}
                {!query.isLoading && !query.error && directories.map((entry) => <button type="button" className={`folder-picker-entry ${selectedPath === entry.path ? "is-selected" : ""}`} key={entry.path} onClick={() => selectDirectory(index, entry)}><Folder size={15} /><span>{entry.name}</span><ChevronRight size={14} /></button>)}
              </div>
            </section>;
          })}
        </div>
        <label className="folder-picker-name"><span>目标名称</span><input value={name} onChange={(event) => { setName(event.target.value); setError(null); }} autoFocus /></label>
        <div className="folder-picker-preview"><span>最终路径</span><strong className="mono">{destinationPath}</strong></div>
        {requiresBookmarkAck && <label className="folder-picker-impact"><input type="checkbox" checked={bookmarkImpactAcknowledged} onChange={(event) => { setBookmarkImpactAcknowledged(event.target.checked); setError(null); }} /><span>我知道移动后将同步更新 {bookmarkImpactCount} 个书签，原书签名称会保留。</span></label>}
        {error && <div className="form-error file-modal-error" role="alert">{error}</div>}
        <div className="dialog-actions folder-picker-actions"><Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>取消</Button><Button type="submit" variant="primary" disabled={pending || !name.trim() || (requiresBookmarkAck && !bookmarkImpactAcknowledged)}>{pending ? <><LoaderCircle className="spin" size={14} />{actionLabel}中…</> : <><Check size={14} />确认{actionLabel}</>}</Button></div>
      </form>
    </FileModal>
  );
}
