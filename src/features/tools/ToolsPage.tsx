import * as Dialog from "@radix-ui/react-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Download, PackageSearch, RefreshCw, ShieldAlert, X } from "lucide-react";
import { useState } from "react";
import { NavLink, useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage, isAppError } from "../../lib/errors";
import type { ToolInstallPlan, ToolStatus } from "../../types/server";
import { useCommandTaskStore } from "../tasks/taskStore";

/** 展示远端工具能力，并要求用户在执行包管理器安装前确认计划。 */
export function ToolsPage() {
  const { serverId = "" } = useParams();
  const queryClient = useQueryClient();
  const [plan, setPlan] = useState<ToolInstallPlan | null>(null);
  const [installOutput, setInstallOutput] = useState("");
  const [installTaskId, setInstallTaskId] = useState<string | null>(null);
  const addTask = useCommandTaskStore((state) => state.add);
  const markSuccess = useCommandTaskStore((state) => state.success);
  const markFail = useCommandTaskStore((state) => state.fail);
  const markCancelled = useCommandTaskStore((state) => state.cancelled);
  const tools = useQuery({ queryKey: ["tools", serverId], queryFn: () => api.listTools(serverId), enabled: !!serverId });
  const planMutation = useMutation({ mutationFn: (toolId: string) => api.toolInstallPlan(serverId, toolId), onSuccess: setPlan });
  const installMutation = useMutation({
    mutationFn: (taskId: string) => api.installTool({ serverId, toolId: plan!.tool.id, taskId }, (event) => { if (event.event === "output") setInstallOutput((current) => current + event.data.data); if (event.event === "cancelled") markCancelled(taskId); }),
    onSuccess: async (_value, taskId) => { markSuccess(taskId); setPlan(null); await queryClient.invalidateQueries({ queryKey: ["tools", serverId] }); },
    onError: (reason, taskId) => { if (isAppError(reason) && reason.code === "CANCELLED") markCancelled(taskId); else markFail(taskId, errorMessage(reason)); },
    onSettled: () => setInstallTaskId(null),
  });
  /** 打开安装计划并清空上一轮远端包管理器输出。 */
  const openInstallPlan = (toolId: string) => { setInstallOutput(""); planMutation.mutate(toolId); };
  /** 为一次用户确认的安装生成 task id，供取消按钮关闭远端 SSH channel。 */
  const startInstall = () => { const taskId = crypto.randomUUID(); setInstallTaskId(taskId); addTask({ id: taskId, type: "tool-install", serverId, title: `安装 ${plan?.tool.name ?? "工具"}`, status: "queued" }); installMutation.mutate(taskId); };
  /** 请求取消当前包管理器安装任务；远端命令不会被静默遗留。 */
  const cancelInstall = () => { if (installTaskId) void api.cancelCommandTask(installTaskId); };

  return <section className="tools-page">
    <div className="workspace-header">
      <div><div className="breadcrumb">服务器 / <span>工具中心</span></div><h1>工具中心</h1><p>能力探测、版本和受控安装计划</p></div>
      <div className="workspace-header__actions"><Button variant="secondary" onClick={() => tools.refetch()} disabled={tools.isFetching}><RefreshCw className={tools.isFetching ? "spin" : ""} size={14} /> 重新探测</Button></div>
    </div>
    <ToolTabs serverId={serverId} />
    {tools.isLoading && <div className="page-state">正在探测远端工具能力…</div>}
    {tools.error && <div className="tool-error-panel"><ShieldAlert size={19} /><div><strong>工具能力探测失败</strong><p>{errorMessage(tools.error)}。请确认 SSH 会话仍在线后重新探测。</p></div><Button size="sm" variant="secondary" onClick={() => tools.refetch()}>重新探测</Button></div>}
    {planMutation.error && <div className="page-state page-state--error">{errorMessage(planMutation.error)}</div>}
    {tools.data && <div className="tool-grid">{tools.data.map((tool) => <ToolCard key={tool.id} tool={tool} onInstall={() => openInstallPlan(tool.id)} loading={planMutation.isPending && planMutation.variables === tool.id} />)}</div>}
    <Dialog.Root open={!!plan} onOpenChange={(open) => !open && !installMutation.isPending && setPlan(null)}>
      <Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow">
        <div className="dialog-header"><div><Dialog.Title>确认安装 {plan?.tool.name}</Dialog.Title><Dialog.Description>{plan?.tool.description}</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div>
        <div className="install-plan"><div><span>包管理器</span><strong>{plan?.tool.packageManager ?? "未知"}</strong></div><div><span>安装包</span><strong className="mono">{plan?.tool.installPackage ?? "—"}</strong></div><div><span>执行计划</span><code>{plan?.command}</code></div><p><ShieldAlert size={16} />{plan?.risk}</p></div>
        {installMutation.error && <div className="form-error">{errorMessage(installMutation.error)}</div>}
        {installOutput && <pre className="install-output">{installOutput}</pre>}
        <div className="dialog-actions"><Button variant="ghost" onClick={() => setPlan(null)} disabled={installMutation.isPending}>取消</Button>{installMutation.isPending ? <Button variant="danger" onClick={cancelInstall}>取消远端任务</Button> : <Button variant="primary" onClick={startInstall}><Download size={14} />确认安装</Button>}</div>
      </Dialog.Content></Dialog.Portal>
    </Dialog.Root>
  </section>;
}

/** 复用服务器工作区导航，并标记当前工具页。 */
function ToolTabs({ serverId }: { serverId: string }) {
  return <nav className="workspace-tabs"><NavLink to={`/servers/${serverId}`}>概览</NavLink><NavLink to={`/servers/${serverId}/files`}>文件</NavLink><NavLink to={`/servers/${serverId}/terminal`}>终端</NavLink><NavLink to={`/servers/${serverId}/operations`}>端口与进程</NavLink><NavLink to={`/servers/${serverId}/services`}>服务</NavLink><NavLink className="active" to={`/servers/${serverId}/tools`}>工具</NavLink><NavLink to={`/servers/${serverId}/logs`}>日志</NavLink><NavLink to={`/servers/${serverId}/nginx`}>Nginx</NavLink><NavLink to={`/servers/${serverId}/docker`}>Docker</NavLink></nav>;
}

/** 展示单项工具状态和需要确认的安装入口。 */
function ToolCard({ tool, onInstall, loading }: { tool: ToolStatus; onInstall: () => void; loading: boolean }) {
  return <article className={`tool-card ${tool.installed ? "is-installed" : ""}`}><div className="tool-card__icon"><PackageSearch size={19} /></div><div className="tool-card__body"><div className="tool-card__title"><strong>{tool.name}</strong><span className={`tool-state ${tool.installed ? "ok" : "muted"}`}>{tool.installed ? "已安装" : "未安装"}</span></div><p>{tool.description}</p><small>{tool.version ?? "等待安装"}{tool.running === true ? " · 运行中" : tool.running === false ? " · 已停止" : ""}</small></div><div className="tool-card__action">{tool.installed ? <CheckCircle2 className="tool-ok" size={18} /> : <Button variant="secondary" size="sm" onClick={onInstall} disabled={loading}>{loading ? "读取计划…" : "查看安装计划"}</Button>}</div></article>;
}
