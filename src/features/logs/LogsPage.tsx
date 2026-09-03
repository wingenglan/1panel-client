import { useMutation, useQuery } from "@tanstack/react-query";
import { Clipboard, Download, FileText, Pause, Play, RefreshCw, Search, Square, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { api, type CommandEvent } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { useCommandTaskStore, type TaskStatus } from "../tasks/taskStore";
import type { LogQuery, LogSource } from "../../types/server";

const sourceLabels: Record<LogSource, string> = {
  system: "系统 journal",
  systemd: "systemd 服务",
  "nginx-access": "Nginx access",
  "nginx-error": "Nginx error",
  docker: "Docker 容器",
  "docker-compose": "Compose 服务",
};

const taskStatusLabels: Record<TaskStatus, string> = {
  queued: "排队中",
  running: "运行中",
  success: "成功",
  failed: "失败",
  cancelled: "已取消",
  interrupted: "已中断",
};

type AuditTab = "panel" | "host" | "login" | "website";
type PanelSub = "operation" | "task";

/** 日志审计：对齐 Web 1Panel 的面板日志/主机日志分组，面板日志列出本地审计事件与任务摘要。 */
export function LogsPage() {
  const { serverId = "" } = useParams();
  const profile = useQuery({ queryKey: ["server", serverId], queryFn: () => api.getServer(serverId), enabled: !!serverId });
  const [auditTab, setAuditTab] = useState<AuditTab>("panel");
  const [panelSub, setPanelSub] = useState<PanelSub>("operation");
  const [source, setSource] = useState<LogSource>("system");
  const [target, setTarget] = useState("");
  const [workingDir, setWorkingDir] = useState("");
  const [service, setService] = useState("");
  const [tail, setTail] = useState(200);
  const [privileged, setPrivileged] = useState(false);
  const [search, setSearch] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [output, setOutput] = useState("");
  const [paused, setPaused] = useState(false);
  const [following, setFollowing] = useState(false);
  const followIdRef = useRef<string | null>(null);
  const followWantedRef = useRef(false);
  const pausedRef = useRef(false);
  const audit = useQuery({ queryKey: ["audit-events"], queryFn: () => api.listAuditEvents(50), enabled: auditTab === "panel" && panelSub === "operation" });
  const tasks = useCommandTaskStore((state) => state.tasks);
  const clearFinished = useCommandTaskStore((state) => state.clearFinished);

  useEffect(() => { pausedRef.current = paused; }, [paused]);
  useEffect(() => () => { followWantedRef.current = false; if (followIdRef.current) void api.cancelCommandTask(followIdRef.current); }, []);

  const query = useMemo<LogQuery>(() => ({
    serverId,
    source,
    target: target.trim() || undefined,
    workingDir: workingDir.trim() || undefined,
    service: service.trim() || undefined,
    tail,
    privileged,
  }), [serverId, source, target, workingDir, service, tail, privileged]);
  const load = useMutation({ mutationFn: () => api.getLogs(query), onSuccess: (value) => setOutput(limitOutput(value.output)) });

  const onEvent = (event: CommandEvent) => {
    if (event.event === "output" && !pausedRef.current) setOutput((current) => limitOutput(`${current}${event.data.data}`));
    if (event.event === "cancelled") setFollowing(false);
  };
  const startFollow = (clearView = true) => {
    if (followIdRef.current) return;
    const taskId = crypto.randomUUID();
    followIdRef.current = taskId;
    followWantedRef.current = true;
    setFollowing(true);
    if (clearView) setOutput("");
    useCommandTaskStore.getState().add({ id: taskId, type: "log-follow", serverId, title: `${sourceLabels[source]}跟随`, status: "running", cancelSupported: true });
    void api.followLogs(query, taskId, onEvent).then((value) => {
      if (value.output && !pausedRef.current) setOutput((current) => limitOutput(`${current}${value.output}`));
      useCommandTaskStore.getState().success(taskId);
      followIdRef.current = null;
      if (followWantedRef.current) {
        setFollowing(false);
        window.setTimeout(() => { if (followWantedRef.current) startFollow(false); }, 350);
      } else setFollowing(false);
    }).catch((reason) => {
      if (reason?.code === "CANCELLED") useCommandTaskStore.getState().cancelled(taskId);
      else { setOutput((current) => `${current}\n[跟随失败] ${errorMessage(reason)}`); useCommandTaskStore.getState().fail(taskId, errorMessage(reason)); }
      followIdRef.current = null;
      setFollowing(false);
    });
  };
  const stopFollow = () => { followWantedRef.current = false; if (followIdRef.current) void api.cancelCommandTask(followIdRef.current); followIdRef.current = null; setFollowing(false); };
  const visibleOutput = useMemo(() => filterOutput(output, search, caseSensitive), [output, search, caseSensitive]);

  return <section className="logs-page">
    <div className="page-toolbar"><span className={`logs-state ${following ? "is-live" : ""}`}><i /> {following ? "实时跟随中" : "按需读取"}</span></div>
    <div className="audit-layout">
      <nav className="audit-nav" role="radiogroup" aria-label="日志分类">
        <button role="radio" aria-checked={auditTab === "panel"} className={auditTab === "panel" ? "is-active" : ""} onClick={() => setAuditTab("panel")}>面板日志</button>
        <button role="radio" aria-checked={auditTab === "host"} className={auditTab === "host" ? "is-active" : ""} onClick={() => setAuditTab("host")}>主机日志</button>
        <button role="radio" aria-checked="false" className="is-planned" disabled title="规划中">登录日志</button>
        <button role="radio" aria-checked="false" className="is-planned" disabled title="规划中">网站日志</button>
      </nav>
      <div className="audit-content">
        {auditTab === "panel" && <>
          <nav className="audit-subnav"><button className={panelSub === "operation" ? "is-active" : ""} onClick={() => setPanelSub("operation")}>操作日志</button><button className={panelSub === "task" ? "is-active" : ""} onClick={() => setPanelSub("task")}>任务日志</button>{panelSub === "task" && <Button size="sm" variant="ghost" onClick={clearFinished}><Trash2 size={13} />清空已结束任务</Button>}</nav>
          {panelSub === "operation" && <>
            <div className="logs-meta"><span>本地审计事件 · 最近 {audit.data?.length ?? 50} 条</span><Button size="sm" variant="secondary" onClick={() => audit.refetch()} disabled={audit.isFetching}><RefreshCw size={13} className={audit.isFetching ? "spin" : ""} />刷新</Button></div>
            {audit.isLoading && <div className="page-state">正在读取本地审计事件…</div>}
            {audit.error && <div className="page-state page-state--error">{errorMessage(audit.error)}</div>}
            {audit.data && (audit.data.length ? <div className="database-table"><div className="ops-head"><span>时间</span><span>资源</span><span>操作</span><span>结果</span><span>服务器</span></div>{audit.data.map((event) => <div className="ops-row" key={event.id}><span className="mono">{new Date(event.createdAt).toLocaleString()}</span><span>{event.resourceType}{event.resourceId ? ` / ${event.resourceId}` : ""}</span><span>{event.summary || event.action}</span><span className={`status-chip ${event.result === "success" ? "status-chip--ok" : "status-chip--danger"}`}>{event.result}</span><span className="mono">{event.serverId?.slice(0, 8) ?? "—"}</span></div>)}</div> : <div className="empty-panel empty-panel--small"><FileText size={20} /><span>还没有本地审计事件。</span></div>)}
          </>}
          {panelSub === "task" && <>
            <div className="logs-meta"><span>本机任务清单 · {tasks.length} 条</span></div>
            {tasks.length ? <div className="database-table"><div className="ops-head"><span>任务</span><span>状态</span><span>开始时间</span><span>详情</span></div>{tasks.map((task) => <div className="ops-row" key={task.id}><strong>{task.title}</strong><span className={`status-chip ${task.status === "success" ? "status-chip--ok" : task.status === "failed" || task.status === "interrupted" ? "status-chip--danger" : task.status === "running" ? "is-live" : ""}`}>{taskStatusLabels[task.status]}</span><span className="mono">{new Date(task.startedAt).toLocaleString()}</span><span>{task.error ?? (task.finishedAt ? new Date(task.finishedAt).toLocaleString() : "—")}</span></div>)}</div> : <div className="empty-panel empty-panel--small"><FileText size={20} /><span>本机还没有任务记录。</span></div>}
          </>}
        </>}
        {auditTab === "host" && <>
          <div className="logs-toolbar"><label><FileText size={15} /><select value={source} onChange={(event) => { setSource(event.target.value as LogSource); setOutput(""); }}><option value="system">系统 journal</option><option value="systemd">systemd 服务</option><option value="nginx-access">Nginx access</option><option value="nginx-error">Nginx error</option><option value="docker">Docker 容器</option><option value="docker-compose">Compose 服务</option></select></label>{(source === "systemd" || source === "docker" || source === "docker-compose") && <label><span className="logs-label">{source === "systemd" ? "服务" : source === "docker" ? "容器" : "项目"}</span><input value={target} onChange={(event) => setTarget(event.target.value)} placeholder={source === "systemd" ? "nginx.service" : source === "docker" ? "容器名或 ID" : "项目名"} /></label>}{source === "docker-compose" && <><label><span className="logs-label">目录</span><input value={workingDir} onChange={(event) => setWorkingDir(event.target.value)} placeholder="/opt/app" /></label><label><span className="logs-label">服务</span><input value={service} onChange={(event) => setService(event.target.value)} placeholder="可选" /></label></>}<label><span className="logs-label">行数</span><select value={tail} onChange={(event) => setTail(Number(event.target.value))}><option value={100}>100</option><option value={200}>200</option><option value={500}>500</option><option value={1000}>1000</option><option value={5000}>5000</option></select></label>{profile.data?.sudoMode !== "none" && <label className="logs-check"><input type="checkbox" checked={privileged} onChange={(event) => setPrivileged(event.target.checked)} /> 使用已配置 sudo</label>}<Button variant="primary" onClick={() => load.mutate()} disabled={load.isPending || following}><RefreshCw size={14} className={load.isPending ? "spin" : ""} /> {load.isPending ? "读取中…" : "读取日志"}</Button>{following ? <Button variant="danger" onClick={stopFollow}><Square size={13} /> 停止跟随</Button> : <Button variant="secondary" onClick={() => startFollow()}><Play size={13} /> 跟随</Button>}</div>
          <div className="logs-filterbar"><label><Search size={15} /><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="在当前日志中搜索" /></label><label className="logs-check"><input type="checkbox" checked={caseSensitive} onChange={(event) => setCaseSensitive(event.target.checked)} /> 区分大小写</label>{following && <Button size="sm" variant="ghost" onClick={() => setPaused((value) => !value)}><Pause size={13} /> {paused ? "继续接收" : "暂停视图"}</Button>}<Button size="sm" variant="ghost" onClick={() => void navigator.clipboard?.writeText(visibleOutput)}><Clipboard size={13} /> 复制</Button><Button size="sm" variant="ghost" onClick={() => downloadText(`relay-${source}.log`, visibleOutput)}><Download size={13} /> 下载</Button><Button size="sm" variant="ghost" onClick={() => setOutput("")}><X size={13} /> 清空视图</Button></div>
          {load.error && <div className="logs-state-panel is-error"><strong>日志读取失败</strong><span>{errorMessage(load.error)}。请检查权限、服务名称和 SSH 会话。</span><Button size="sm" variant="secondary" onClick={() => load.mutate()}>重试</Button></div>}
          <div className="logs-meta"><span>{sourceLabels[source]} · 最多保留 5000 行 / 1 MB</span><span>{visibleOutput ? `${visibleOutput.split("\n").length} 行` : "暂无内容"}</span></div>
          <pre className="logs-output">{visibleOutput || (load.isPending ? "正在建立 SSH 查询…" : "选择来源并读取日志；跟随模式会在远端通道断开后显示状态。")}</pre>
        </>}
        {(auditTab === "login" || auditTab === "website") && <div className="empty-panel empty-panel--small"><FileText size={20} /><span>「{auditTab === "login" ? "登录日志" : "网站日志"}」模块正在规划中，将在后续版本提供。</span></div>}
      </div>
    </div>
  </section>;
}

/** Keeps the live log view bounded to prevent a long-running follow from exhausting memory. */
function limitOutput(value: string) {
  const maxBytes = 1_000_000;
  const bounded = value.length > maxBytes ? value.slice(-maxBytes) : value;
  const lines = bounded.split("\n");
  return lines.length > 5_000 ? lines.slice(-5_000).join("\n") : bounded;
}

/** Applies the local search filter without sending log content back to the remote host. */
function filterOutput(output: string, search: string, caseSensitive: boolean) {
  const needle = caseSensitive ? search : search.toLocaleLowerCase();
  if (!needle) return output;
  return output.split("\n").filter((line) => (caseSensitive ? line : line.toLocaleLowerCase()).includes(needle)).join("\n");
}

/** Downloads only the currently visible in-memory log slice through the browser/Tauri webview. */
function downloadText(filename: string, content: string) {
  const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}
