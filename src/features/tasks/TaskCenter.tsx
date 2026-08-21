import { Check, CircleX, LoaderCircle, RefreshCw, Trash2, X } from "lucide-react";
import { useMemo, useState } from "react";
import { api } from "../../lib/api";
import { formatBytes } from "../../lib/format";
import { useTransferStore } from "../files/transferStore";
import { useCommandTaskStore, type CommandTask } from "./taskStore";
import { useNavigate } from "react-router-dom";

/** 展示所有远程命令与文件传输任务，并提供统一取消和清理入口。 */
export function TaskCenter({ open, onClose }: { open: boolean; onClose: () => void }) {
  const navigate = useNavigate();
  const [serverFilter, setServerFilter] = useState("all");
  const [statusFilter, setStatusFilter] = useState("all");
  const commandTasks = useCommandTaskStore((state) => state.tasks);
  const transferTasks = useTransferStore((state) => state.tasks);
  const clearCommands = useCommandTaskStore((state) => state.clearFinished);
  const clearTransfers = useTransferStore((state) => state.clearFinished);
  const activeCount = commandTasks.filter(isActive).length + transferTasks.filter((task) => task.status === "queued" || task.status === "running").length;
  const serverOptions = useMemo(() => [...new Set(commandTasks.map((task) => task.serverId).filter((serverId) => serverId && serverId !== "—"))], [commandTasks]);
  const filteredCommands = commandTasks.filter((task) => matchesTaskFilter(task.status, task.serverId, statusFilter, serverFilter));
  const filteredTransfers = transferTasks.filter((task) => matchesTaskFilter(task.status, "", statusFilter, serverFilter));
  if (!open) return null;
  return <aside className="task-center" aria-label="任务中心">
    <header><div><strong>任务中心</strong><small>{activeCount} 个活动任务</small></div><button onClick={onClose} aria-label="关闭"><X size={15} /></button></header>
    <div className="task-filters"><label><span>服务器</span><select value={serverFilter} onChange={(event) => setServerFilter(event.target.value)}><option value="all">全部服务器</option>{serverOptions.map((serverId) => <option key={serverId} value={serverId}>{serverId}</option>)}</select></label><label><span>状态</span><select value={statusFilter} onChange={(event) => setStatusFilter(event.target.value)}><option value="all">全部状态</option><option value="queued">等待中</option><option value="running">执行中</option><option value="success">已完成</option><option value="failed">失败</option><option value="cancelled">已取消</option><option value="interrupted">已中断</option></select></label></div>
    <div className="task-list">
      {!filteredCommands.length && !filteredTransfers.length && <div className="task-empty">没有符合筛选条件的任务</div>}
      {filteredCommands.map((task) => <CommandTaskRow key={task.id} task={task} onRetry={() => retryCommandTask(task, navigate)} />)}
      {filteredTransfers.map((task) => <article className="task-row" key={task.id}><div className="task-row__icon"><LoaderCircle size={15} className={isActive(task) ? "spin" : ""} /></div><div><strong>{task.label}</strong><small>{task.error ?? task.currentPath}</small><footer><span>{formatBytes(task.transferredBytes)}{task.totalBytes ? ` / ${formatBytes(task.totalBytes)}` : ""}</span><span>{task.status === "running" ? `${formatBytes(task.bytesPerSecond)}/s` : statusLabel(task.status)}</span></footer></div><span className={`task-status is-${task.status}`}>{task.status === "running" || task.status === "queued" ? <button title="取消传输" onClick={() => void api.cancelTransfer(task.id)}><CircleX size={15} /></button> : task.status === "success" ? <Check size={15} /> : task.retry ? <button title="重试传输" onClick={task.retry}><RefreshCw size={15} /></button> : task.status === "failed" ? <CircleX size={15} /> : <LoaderCircle size={15} />}</span></article>)}
    </div>
    <footer><button onClick={() => { clearCommands(); clearTransfers(); }}><Trash2 size={13} /> 清除已完成</button></footer>
  </aside>;
}

/** 渲染单个可取消的远程命令任务。 */
function CommandTaskRow({ task, onRetry }: { task: CommandTask; onRetry: () => void }) {
  const cancel = () => { if (task.cancelSupported) void api.cancelCommandTask(task.id); };
  const progress = task.progress === null ? null : `${Math.round(task.progress)}%`;
  return <article className="task-row"><div className="task-row__icon"><LoaderCircle size={15} className={isActive(task) ? "spin" : ""} /></div><div><strong>{task.title}</strong><small>{task.status === "interrupted" ? "应用重启后未自动重放，请返回原模块手动重试" : task.error ?? task.serverId}</small>{progress && <div className="task-progress"><i style={{ width: `${Math.max(0, Math.min(100, task.progress ?? 0))}%` }} /></div>}<footer><span>{typeLabel(task.type)}{task.totalBytes ? ` · ${task.bytesTransferred}/${task.totalBytes} B` : ""}</span><span>{progress ?? statusLabel(task.status)}</span></footer></div><span className={`task-status is-${task.status}`}>{isActive(task) && task.cancelSupported ? <button title="取消远程任务" onClick={cancel}><CircleX size={15} /></button> : task.status === "success" ? <Check size={15} /> : task.status === "failed" || task.status === "interrupted" ? <button title={task.retry ? "重试任务" : "返回原模块手动重试"} onClick={onRetry} disabled={!task.retry && !hasRetryRoute(task)}><RefreshCw size={15} /></button> : <LoaderCircle size={15} />}</span></article>;
}

/** Applies the optional task-center status and server filters to both persisted and in-memory tasks. */
function matchesTaskFilter(status: string, serverId: string, statusFilter: string, serverFilter: string) {
  return (statusFilter === "all" || status === statusFilter) && (serverFilter === "all" || serverId === serverFilter);
}

/** 为中断任务提供安全的手动重试入口；没有可恢复回调时只导航回原模块，不自动执行远端操作。 */
function retryCommandTask(task: CommandTask, navigate: (path: string) => void) {
  if (task.retry) {
    task.retry();
    return;
  }
  if (hasRetryRoute(task)) navigate(`/servers/${task.serverId}/${retryRoute(task.type)}`);
}

/** 判断任务是否仍能回到包含原始操作入口的服务器模块。 */
function hasRetryRoute(task: CommandTask) { return task.serverId !== "" && task.serverId !== "—"; }

/** 将持久化任务类型映射到需要用户重新确认操作的安全页面。 */
function retryRoute(type: CommandTask["type"]) { return type === "tool-install" ? "tools" : type === "database-install" ? "database" : type === "log-follow" ? "logs" : "docker"; }

/** 判断任务是否仍在队列或执行中。 */
function isActive(task: { status: string }) { return task.status === "queued" || task.status === "running"; }

/** 将任务类型转换为用户可读的中文标签。 */
function typeLabel(type: CommandTask["type"]) { return { "tool-install": "工具安装", "database-install": "数据库安装", "docker-pull": "Docker Pull", "docker-build": "Docker Build", "docker-follow": "Docker Follow", "docker-compose": "Docker Compose", "ai-chat": "AI 对话", "log-follow": "日志跟随" }[type]; }

/** 将统一任务状态转换为用户可读文本。 */
function statusLabel(status: string) { return { queued: "等待中", running: "执行中", success: "已完成", failed: "失败", cancelled: "已取消", interrupted: "已中断" }[status] ?? status; }
