import { ArrowDownToLine, ArrowUpFromLine, Check, CircleX, LoaderCircle, RefreshCw, Trash2, X } from "lucide-react";
import { api } from "../../lib/api";
import { formatBytes } from "../../lib/format";
import { useTransferStore } from "./transferStore";

/** 显示跨页面持续存在的文件传输，并允许失败或取消任务重新提交。 */
export function TransferCenter({ open, onClose }: { open: boolean; onClose: () => void }) {
  const tasks = useTransferStore((state) => state.tasks);
  const clearFinished = useTransferStore((state) => state.clearFinished);
  if (!open) return null;
  return <aside className="transfer-center" aria-label="传输中心">
    <header><div><strong>传输中心</strong><small>{tasks.filter((task) => task.status === "running" || task.status === "queued").length} 个活动任务</small></div><button onClick={onClose} aria-label="关闭"><X size={15} /></button></header>
    <div className="transfer-list">
      {!tasks.length && <div className="transfer-empty">尚无传输任务</div>}
      {tasks.map((task) => {
        const percent = task.totalBytes ? Math.min(100, task.transferredBytes / task.totalBytes * 100) : 0;
        return <article key={task.id}>
          <span className="transfer-direction">{task.direction === "upload" ? <ArrowUpFromLine size={15} /> : <ArrowDownToLine size={15} />}</span>
          <div><strong>{task.label}</strong><small>{task.error ?? task.currentPath}</small><div className={`transfer-progress ${task.totalBytes ? "" : "is-indeterminate"}`}><i style={{ width: task.totalBytes ? `${percent}%` : undefined }} /></div><footer><span>{formatBytes(task.transferredBytes)}{task.totalBytes ? ` / ${formatBytes(task.totalBytes)}` : ""}</span><span>{task.status === "running" ? `${formatBytes(task.bytesPerSecond)}/s` : statusLabel(task.status)}</span></footer></div>
          <span className={`transfer-status is-${task.status}`}>{task.status === "running" || task.status === "queued" ? <button title="取消传输" onClick={() => void api.cancelTransfer(task.id)}><CircleX size={15} /></button> : task.status === "success" ? <Check size={15} /> : task.retry ? <button title="重试传输" onClick={task.retry}><RefreshCw size={15} /></button> : task.status === "failed" ? <CircleX size={15} /> : <LoaderCircle size={15} />}</span>
        </article>;
      })}
    </div>
    <footer><button onClick={clearFinished}><Trash2 size={13} /> 清除已完成</button></footer>
  </aside>;
}

function statusLabel(status: string) {
  return { queued: "等待中", running: "传输中", success: "已完成", failed: "失败", cancelled: "已取消" }[status] ?? status;
}
