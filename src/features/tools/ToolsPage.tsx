import * as Dialog from "@radix-ui/react-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, Clock3, Download, Globe2, HardDrive, LockKeyhole, PackageSearch, RefreshCw, Settings2, ShieldAlert, TerminalSquare, X } from "lucide-react";
import { useState } from "react";
import { useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage, isAppError } from "../../lib/errors";
import type { ToolInstallPlan, ToolStatus } from "../../types/server";
import { useCommandTaskStore } from "../tasks/taskStore";

type ToolboxTab = "quick" | "tools" | "cache" | "guard" | "virus" | "ftp" | "fail2ban";
const plannedTools: Array<{ key: ToolboxTab; label: string }> = [
  { key: "cache", label: "缓存清理" },
  { key: "guard", label: "进程守护" },
  { key: "virus", label: "病毒扫描" },
  { key: "ftp", label: "FTP" },
  { key: "fail2ban", label: "Fail2ban" },
];

/** 工具箱：左侧功能分组对齐 Web 1Panel（快速设置优先），工具集保留客户端能力探测与受控安装。 */
export function ToolsPage() {
  const { serverId = "" } = useParams();
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<ToolboxTab>("quick");
  const [plan, setPlan] = useState<ToolInstallPlan | null>(null);
  const [installOutput, setInstallOutput] = useState("");
  const [installTaskId, setInstallTaskId] = useState<string | null>(null);
  const addTask = useCommandTaskStore((state) => state.add);
  const markSuccess = useCommandTaskStore((state) => state.success);
  const markFail = useCommandTaskStore((state) => state.fail);
  const markCancelled = useCommandTaskStore((state) => state.cancelled);
  const tools = useQuery({ queryKey: ["tools", serverId], queryFn: () => api.listTools(serverId), enabled: !!serverId && tab === "tools" });
  const overview = useQuery({ queryKey: ["overview", serverId], queryFn: () => api.overview(serverId), enabled: !!serverId && tab === "quick" });
  const hosts = useQuery({ queryKey: ["read-text", serverId, "/etc/hosts"], queryFn: () => api.readText(serverId, "/etc/hosts"), enabled: !!serverId && tab === "quick" });
  const resolv = useQuery({ queryKey: ["read-text", serverId, "/etc/resolv.conf"], queryFn: () => api.readText(serverId, "/etc/resolv.conf"), enabled: !!serverId && tab === "quick" });
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
  /** 刷新当前页面的全部数据源。 */
  const refresh = () => { void tools.refetch(); void overview.refetch(); void hosts.refetch(); void resolv.refetch(); };
  const busy = tools.isFetching || overview.isFetching || hosts.isFetching || resolv.isFetching;

  return <section className="toolbox-page">
    <div className="toolbox-layout">
      <nav className="toolbox-nav" role="radiogroup" aria-label="工具箱功能">
        <button role="radio" aria-checked={tab === "quick"} className={tab === "quick" ? "is-active" : ""} onClick={() => setTab("quick")}>快速设置</button>
        <button role="radio" aria-checked={tab === "tools"} className={tab === "tools" ? "is-active" : ""} onClick={() => setTab("tools")}>工具集</button>
        {plannedTools.map((item) => <button key={item.key} role="radio" aria-checked="false" className="is-planned" disabled title="规划中">{item.label}</button>)}
      </nav>
      <div className="toolbox-content">
        {tab === "quick" && <article className="toolbox-card">
          <header><div><span className="section-kicker">Quick settings</span><h2>快速设置</h2><p>读取远端系统配置；修改操作请在 Web 面板中执行。</p></div><div className="toolbox-card__actions"><span className="status-chip status-chip--ok">只读</span><Button size="sm" variant="secondary" onClick={refresh} disabled={busy}><RefreshCw className={busy ? "spin" : ""} size={13} /> 刷新</Button></div></header>
          <div className="toolbox-quick-actions"><Button variant="secondary" disabled title="请在 Web 面板中执行">重启面板</Button><Button variant="secondary" disabled title="请在 Web 面板中执行">重启服务器</Button></div>
          <div className="toolbox-quick-rows">
            <div className="toolbox-quick-row"><Globe2 size={15} /><span>DNS 服务器</span><code className="toolbox-quick-row__value">{resolv.data?.content.trim() || (resolv.isLoading ? "读取中…" : "未读取到")}</code><Button size="sm" variant="secondary" disabled title="请在 Web 面板中设置">设置</Button></div>
            <div className="toolbox-quick-row"><HardDrive size={15} /><span>Hosts</span><code className="toolbox-quick-row__value">{hosts.data?.content.trim() || (hosts.isLoading ? "读取中…" : "未读取到")}</code><Button size="sm" variant="secondary" disabled title="请在 Web 面板中设置">设置</Button></div>
            <div className="toolbox-quick-row"><TerminalSquare size={15} /><span>主机名</span><strong className="toolbox-quick-row__value">{overview.data?.hostname ?? (overview.isLoading ? "读取中…" : "—")}</strong><Button size="sm" variant="secondary" disabled title="请在 Web 面板中设置">设置</Button></div>
            <div className="toolbox-quick-row"><Settings2 size={15} /><span>系统时区</span><strong className="toolbox-quick-row__value">{overview.data?.timezone ?? (overview.isLoading ? "读取中…" : "—")}</strong><Button size="sm" variant="secondary" disabled title="请在 Web 面板中设置">设置</Button></div>
            <div className="toolbox-quick-row"><Clock3 size={15} /><span>服务器时间</span><strong className="toolbox-quick-row__value">{overview.data?.currentTime ?? (overview.isLoading ? "读取中…" : "—")}</strong><Button size="sm" variant="secondary" disabled title="请在 Web 面板中同步">同步</Button></div>
          </div>
        </article>}
        {tab === "tools" && <>
          {tools.isLoading && <div className="page-state">正在探测远端工具能力…</div>}
          {tools.error && <div className="tool-error-panel"><ShieldAlert size={19} /><div><strong>工具能力探测失败</strong><p>{errorMessage(tools.error)}。请确认 SSH 会话仍在线后重新探测。</p></div><Button size="sm" variant="secondary" onClick={() => tools.refetch()}>重新探测</Button></div>}
          {planMutation.error && <div className="page-state page-state--error">{errorMessage(planMutation.error)}</div>}
          {tools.data && <div className="tool-grid">{tools.data.map((tool) => <ToolCard key={tool.id} tool={tool} onInstall={() => openInstallPlan(tool.id)} loading={planMutation.isPending && planMutation.variables === tool.id} />)}</div>}
        </>}
        {plannedTools.some((item) => item.key === tab) && <div className="empty-panel empty-panel--small"><LockKeyhole size={20} /><span>「{plannedTools.find((item) => item.key === tab)?.label}」模块正在规划中，将在后续版本提供。</span></div>}
      </div>
    </div>
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

/** 展示单项工具状态和需要确认的安装入口。 */
function ToolCard({ tool, onInstall, loading }: { tool: ToolStatus; onInstall: () => void; loading: boolean }) {
  return <article className={`tool-card ${tool.installed ? "is-installed" : ""}`}><div className="tool-card__icon"><PackageSearch size={19} /></div><div className="tool-card__body"><div className="tool-card__title"><strong>{tool.name}</strong><span className={`tool-state ${tool.installed ? "ok" : "muted"}`}>{tool.installed ? "已安装" : "未安装"}</span></div><p>{tool.description}</p><small>{tool.version ?? "等待安装"}{tool.running === true ? " · 运行中" : tool.running === false ? " · 已停止" : ""}</small></div><div className="tool-card__action">{tool.installed ? <CheckCircle2 className="tool-ok" size={18} /> : <Button variant="secondary" size="sm" onClick={onInstall} disabled={loading}>{loading ? "读取计划…" : "查看安装计划"}</Button>}</div></article>;
}
