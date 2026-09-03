import * as Dialog from "@radix-ui/react-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import ReactECharts from "echarts-for-react";
import { Check, ChevronDown, Copy, Edit, Eye, EyeOff, FileText, LogOut, Pencil, Power, RefreshCw, ShieldAlert, Terminal, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { api, isHostKeyChallenge } from "../../lib/api";
import { formatDuration, formatBytes } from "../../lib/format";
import { errorMessage } from "../../lib/errors";
import type { HostKeyChallenge, MetricSample, SystemOverview } from "../../types/server";
import { ServerDialog } from "../servers/ServerDialog";

/** 1Panel 面板首页：概览统计、系统状态、监控图表、系统信息与本地备忘、应用入口。 */
export function OverviewPage() {
  const { serverId = "" } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [challenge, setChallenge] = useState<HostKeyChallenge | null>(null);
  const [editOpen, setEditOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [serviceTarget, setServiceTarget] = useState<{ name: string; action: "start" | "stop" | "restart" } | null>(null);
  const [serviceLogs, setServiceLogs] = useState<{ name: string; output: string } | null>(null);
  const profile = useQuery({ queryKey: ["server", serverId], queryFn: () => api.getServer(serverId), enabled: !!serverId });
  const connection = useQuery({ queryKey: ["connection", serverId], queryFn: () => api.connectionState(serverId), enabled: !!serverId, refetchInterval: 5000 });
  const overview = useQuery({ queryKey: ["overview", serverId], queryFn: () => api.overview(serverId), enabled: connection.data?.status === "online", refetchInterval: 5000 });
  const history = useQuery({ queryKey: ["metric-history", serverId, "1h"], queryFn: () => api.metricHistory(serverId, new Date(Date.now() - 60 * 60 * 1000).toISOString()), enabled: connection.data?.status === "online", refetchInterval: 5000 });
  const memo = useQuery({ queryKey: ["overview-memo", serverId], queryFn: () => api.overviewMemo(serverId), enabled: !!serverId });
  const websites = useQuery({ queryKey: ["websites", serverId], queryFn: () => api.websites(serverId), enabled: connection.data?.status === "online", refetchInterval: 15000 });
  const databases = useQuery({ queryKey: ["databases", serverId], queryFn: () => api.database(serverId), enabled: connection.data?.status === "online", refetchInterval: 15000 });
  const installedApps = useQuery({ queryKey: ["installed-apps", serverId], queryFn: () => api.installedApps(serverId), enabled: connection.data?.status === "online", refetchInterval: 15000 });
  const connect = useMutation({ mutationFn: () => api.connectServer(serverId), onSuccess: async (value) => { if (isHostKeyChallenge(value)) setChallenge(value); else await queryClient.invalidateQueries({ queryKey: ["connection", serverId] }); } });
  const reconnect = useMutation({ mutationFn: () => api.reconnectServer(serverId), onSuccess: async (value) => { if (isHostKeyChallenge(value)) setChallenge(value); else await queryClient.invalidateQueries({ queryKey: ["connection", serverId] }); } });
  const trust = useMutation({ mutationFn: () => api.trustHostKey(challenge!), onSuccess: async () => { setChallenge(null); await queryClient.invalidateQueries({ queryKey: ["connection", serverId] }); } });
  const disconnect = useMutation({ mutationFn: () => api.disconnectServer(serverId), onSuccess: async () => queryClient.invalidateQueries({ queryKey: ["connection", serverId] }) });
  const duplicate = useMutation({ mutationFn: () => api.duplicateServer(serverId), onSuccess: async (server) => { await queryClient.invalidateQueries({ queryKey: ["servers"] }); navigate(`/servers/${server.id}`); } });
  const remove = useMutation({ mutationFn: () => api.deleteServer(serverId), onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["servers"] }); navigate("/"); } });
  const service = useMutation({ mutationFn: () => api.manageService(serverId, serviceTarget!.name, serviceTarget!.action), onSuccess: async () => { setServiceTarget(null); await queryClient.invalidateQueries({ queryKey: ["overview", serverId] }); } });
  const logs = useMutation({ mutationFn: (name: string) => api.serviceLogs(serverId, name), onSuccess: setServiceLogs });
  const saveMemo = useMutation({ mutationFn: (content: string) => api.saveOverviewMemo({ serverId, content }), onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["overview-memo", serverId] }); } });

  if (profile.isLoading) return <div className="page-state">正在读取服务器配置…</div>;
  if (profile.error || !profile.data) return <div className="page-state page-state--error">无法读取服务器配置。</div>;
  const server = profile.data;
  const online = connection.data?.status === "online";
  const data = overview.data;

  const quickJump = [
    { name: "智能体", count: 0, to: "/ai" },
    { name: "网站", count: websites.data?.websites.length ?? 0, to: `/servers/${serverId}/website` },
    { name: "数据库 - 所有", count: databases.data?.databases.length ?? 0, to: `/servers/${serverId}/database` },
    { name: "已安装应用", count: installedApps.data?.apps.length ?? 0, to: `/servers/${serverId}/appstore` },
  ];

  return (
    <section className="home-page">
      <div className="home-router">
        <div className="home-router__tabs">
          <button className="home-router__tab is-active" type="button">概览</button>
        </div>
        <div className="home-router__actions">
          {online && <Button variant="ghost" onClick={() => setEditOpen(true)} aria-label="编辑服务器"><Pencil size={14} /></Button>}
          {online && <Button variant="ghost" onClick={() => duplicate.mutate()} disabled={duplicate.isPending} aria-label="复制档案"><Copy size={14} /></Button>}
          {online && <Button variant="ghost" onClick={() => disconnect.mutate()} disabled={disconnect.isPending} aria-label="断开连接"><LogOut size={14} /></Button>}
          {!online && <Button variant="primary" onClick={() => connect.mutate()} disabled={connect.isPending || reconnect.isPending}><Power size={14} /> {connect.isPending || reconnect.isPending ? "连接中…" : connection.data?.status === "error" ? "重新连接" : "连接"}</Button>}
          <Button variant="ghost" onClick={() => navigate(`/servers/${serverId}/terminal`)} aria-label="打开终端"><Terminal size={14} /></Button>
          <Button variant="ghost" onClick={() => setDeleteOpen(true)} aria-label="删除服务器"><Trash2 size={14} /></Button>
        </div>
      </div>

      {!online && <div className="connect-panel home-connect-panel"><div className="connect-panel__icon"><ShieldAlert size={27} /></div><h2>{connection.data?.status === "error" ? "连接失败" : "建立安全 SSH 会话"}</h2><p>{connection.data?.error?.message ?? "连接后将从远程标准接口读取真实系统状态。首次连接需要核对服务器 Host Key 指纹。"}</p><Button variant="primary" onClick={() => (connection.data?.status === "error" ? reconnect.mutate() : connect.mutate())} disabled={connect.isPending || reconnect.isPending}>{connect.isPending || reconnect.isPending ? <RefreshCw className="spin" size={16} /> : <Power size={16} />} {connect.isPending || reconnect.isPending ? "正在握手" : "连接"}</Button>{(connect.error || reconnect.error) && <div className="form-error">{errorMessage(connect.error ?? reconnect.error)}</div>}</div>}
      {duplicate.error && <div className="page-state page-state--error">{errorMessage(duplicate.error)}</div>}

      {online && !data && <div className="metrics-skeleton"><div /><div /><div /><div /></div>}
      {online && data && (
        <>
          <div className="home-grid">
            <div className="home-grid__left">
              <QuickJumpCard items={quickJump} />
              <SystemStatusCard data={data} />
              <MonitorCard data={data} history={history.data ?? []} />
            </div>
            <div className="home-grid__right">
              <DashboardCarousel data={data} memoQuery={memo} saveMemo={saveMemo} />
              <AppLauncherCard serverId={serverId} data={data} onServiceAction={(name, action) => setServiceTarget({ name, action })} onServiceLogs={(name) => logs.mutate(name)} />
            </div>
          </div>
          <footer className="home-footer"><span>Copyright © 2014-2026 飞致云</span><a href="https://fit2cloud.com/" target="_blank" rel="noreferrer">了解商业版</a><span className="home-footer__sep" /><a href="https://forum.fit2cloud.com/" target="_blank" rel="noreferrer">论坛求助</a><a href="https://www.fit2cloud.com/" target="_blank" rel="noreferrer">使用手册</a></footer>
        </>
      )}

      <Dialog.Root open={!!challenge} onOpenChange={(open) => !open && setChallenge(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow"><div className="hostkey-hero"><ShieldAlert size={30} /><span>首次连接</span></div><Dialog.Title>核对服务器身份</Dialog.Title><Dialog.Description>服务器返回了一个尚未信任的 Host Key。请通过可信渠道核对指纹后再继续。</Dialog.Description><dl className="fingerprint"><div><dt>主机</dt><dd>{challenge?.host}:{challenge?.port}</dd></div><div><dt>算法</dt><dd>{challenge?.keyType}</dd></div><div><dt>SHA256 指纹</dt><dd>{challenge?.fingerprint}</dd></div></dl><div className="dialog-actions"><Button variant="ghost" onClick={() => setChallenge(null)}>取消</Button><Button variant="primary" onClick={() => trust.mutate()} disabled={trust.isPending}>信任并连接</Button></div></Dialog.Content></Dialog.Portal></Dialog.Root>
      <ServerDialog key={`${server.updatedAt}:${editOpen}`} open={editOpen} onOpenChange={setEditOpen} profile={server} />
      <Dialog.Root open={!!serviceTarget} onOpenChange={(open) => !open && setServiceTarget(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow confirm-dialog"><div className="destructive-icon"><ShieldAlert size={22} /></div><Dialog.Title>确认服务操作</Dialog.Title><Dialog.Description>将对 {serviceTarget?.name} 执行 {serviceTarget?.action}，完成后重新读取 Overview。</Dialog.Description>{service.error && <div className="form-error">{errorMessage(service.error)}</div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => setServiceTarget(null)}>取消</Button><Button variant="primary" onClick={() => service.mutate()} disabled={service.isPending}>{service.isPending ? "执行并验证中…" : "确认执行"}</Button></div></Dialog.Content></Dialog.Portal></Dialog.Root>
      <Dialog.Root open={!!serviceLogs} onOpenChange={(open) => !open && setServiceLogs(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content docker-logs-dialog"><div className="dialog-header"><div><Dialog.Title>服务日志</Dialog.Title><Dialog.Description>{serviceLogs?.name} · 最近 200 行</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div>{logs.error && <div className="form-error">{errorMessage(logs.error)}</div>}<pre className="docker-logs">{serviceLogs?.output}</pre></Dialog.Content></Dialog.Portal></Dialog.Root>
      <Dialog.Root open={deleteOpen} onOpenChange={setDeleteOpen}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow confirm-dialog"><div className="destructive-icon"><Trash2 size={22} /></div><Dialog.Title>删除“{server.name}”</Dialog.Title><Dialog.Description>这会删除本机保存的服务器档案和关联凭据，不会删除或修改远端服务器。此操作无法撤销。</Dialog.Description>{remove.error && <div className="form-error">{errorMessage(remove.error)}</div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => setDeleteOpen(false)}>取消</Button><Button variant="danger" onClick={() => remove.mutate()} disabled={remove.isPending}>{remove.isPending ? "正在删除…" : `删除 ${server.name}`}</Button></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    </section>
  );
}

/** 概览统计卡：智能体 / 网站 / 数据库 - 所有 / 已安装应用，数字为快速跳转入口。 */
function QuickJumpCard({ items }: { items: Array<{ name: string; count: number; to: string }> }) {
  const navigate = useNavigate();
  return (
    <article className="home-card">
      <div className="home-card__header"><span className="home-card__title">概览</span></div>
      <div className="home-card__body">
        <div className="home-quick">
          {items.map((item) => <a key={item.name} className="home-quick__item" onClick={() => navigate(item.to)}><span>{item.name}</span><strong>{item.count}</strong></a>)}
        </div>
      </div>
    </article>
  );
}

const ringColor = ["#7faef5", "#005eeb"];

/** 负载 / CPU / 内存 / 磁盘四个环形状态图，与 web v-charts pie 比例一致。 */
function MiniRing({ percent, title }: { percent: number; title: string }) {
  const clamped = Math.max(0, Math.min(100, percent));
  const [whole, decimal] = String(clamped.toFixed(2)).split(".");
  const option = {
    animation: false,
    title: [{
      text: `{a|${whole}.}{b|${decimal || 0} %}`,
      left: "34%", top: "32%",
      textStyle: { rich: { a: { fontSize: 22 }, b: { fontSize: 14, padding: [5, 0, 0, 0] } }, color: "#303133", lineHeight: 25, fontWeight: 500 },
      subtext: title, subtextStyle: { color: "#858a97", fontSize: 13 }, textAlign: "center",
    }],
    polar: { radius: ["71%", "80%"], center: ["50%", "50%"] },
    angleAxis: { max: 100, show: false },
    radiusAxis: { type: "category", show: false },
    series: [
      { type: "bar", roundCap: true, barWidth: 30, showBackground: true, coordinateSystem: "polar", backgroundStyle: { color: "#e5eefd" }, color: ringColor, label: { show: false }, data: [clamped] },
      { type: "pie", radius: ["0%", "60%"], center: ["50%", "50%"], label: { show: false }, color: "#ffffff", data: [{ value: 0, itemStyle: { shadowColor: "rgba(0, 94, 235, 0.1)", shadowBlur: 5 } }] },
    ],
  };
  return <ReactECharts option={option} opts={{ renderer: "canvas" }} style={{ height: 160, width: "100%" }} />;
}

/** 系统状态卡：运行流畅 / 核数 / 内存 / 磁盘四个环形图。 */
function SystemStatusCard({ data }: { data: SystemOverview }) {
  const memoryUsed = data.memoryTotalBytes - data.memoryAvailableBytes;
  const loadPercent = data.logicalCores > 0 ? (data.load[0] / data.logicalCores) * 100 : 0;
  const memoryPercent = data.memoryTotalBytes > 0 ? (memoryUsed / data.memoryTotalBytes) * 100 : 0;
  const disk = data.disks[0];
  const diskPercent = disk?.usagePercent ?? 0;
  const loadLabel = data.load[0] < data.logicalCores * 0.7 ? "运行流畅" : "负载较高";
  return (
    <article className="home-card home-card--status">
      <div className="home-card__header"><span className="home-card__title">状态</span></div>
      <div className="home-card__body">
        <div className="home-status">
          <div className="home-status__item"><MiniRing percent={loadPercent} title="负载" /><span className="home-status__label">{loadLabel}</span></div>
          <div className="home-status__item"><MiniRing percent={data.cpuUsagePercent ?? 0} title="CPU" /><span className="home-status__label">( {data.load[0].toFixed(2)} / {data.logicalCores} ) 核</span></div>
          <div className="home-status__item"><MiniRing percent={memoryPercent} title="内存" /><span className="home-status__label">{formatBytes(memoryUsed)} / {formatBytes(data.memoryTotalBytes)}</span></div>
          <div className="home-status__item"><MiniRing percent={diskPercent} title="磁盘" /><span className="home-status__label">{disk ? `${formatBytes(disk.usedBytes)} / ${formatBytes(disk.totalBytes)}` : "—"}</span></div>
        </div>
      </div>
    </article>
  );
}

/** 监控卡：流量 / 磁盘 IO 切换 + 网卡筛选 + 实时标签 + 折线图。 */
function MonitorCard({ data, history }: { data: SystemOverview; history: MetricSample[] }) {
  const [mode, setMode] = useState<"network" | "io">("network");
  const points = useMemo(() => history.slice(-20), [history]);
  const categories = points.map((sample) => new Date(sample.sampledAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }));
  const networkSeries = [
    { name: "上行", data: points.map((item) => item.networkRxBytesPerSecond / 1024), color: "#005eeb" },
    { name: "下行", data: points.map((item) => item.networkTxBytesPerSecond / 1024), color: "#7faef5" },
  ];
  const ioSeries = [
    { name: "读取", data: points.map((item) => (item.ioReadBytesPerSecond ?? 0) / 1024 / 1024), color: "#005eeb" },
    { name: "写入", data: points.map((item) => (item.ioWriteBytesPerSecond ?? 0) / 1024 / 1024), color: "#7faef5" },
  ];
  const chartOption = {
    animation: false,
    grid: { left: 65, right: 65, bottom: "20%", top: 40 },
    legend: { top: 0, left: "center", textStyle: { color: "var(--text-soft)", fontSize: 12 } },
    tooltip: { trigger: "axis" },
    xAxis: { type: "category", boundaryGap: false, data: categories.length ? categories : Array.from({ length: 20 }, () => ""), axisLabel: { color: "var(--muted)", fontSize: 12 }, axisLine: { lineStyle: { color: "var(--line)" } } },
    yAxis: { type: "value", axisLabel: { color: "var(--muted)", fontSize: 12 }, splitLine: { lineStyle: { color: "var(--line-soft)" } } },
    series: [...(mode === "network" ? networkSeries : ioSeries)].map((item) => ({ name: item.name, type: "line", smooth: true, showSymbol: false, data: item.data, lineStyle: { color: item.color, width: 2 }, itemStyle: { color: item.color }, areaStyle: { color: `${item.color}18` } })),
    dataZoom: [{ type: "inside" }],
  };
  const totalRx = data.networkRxBytesTotal ?? 0;
  const totalTx = data.networkTxBytesTotal ?? 0;
  return (
    <article className="home-card home-card--monitor">
      <div className="home-card__header">
        <span className="home-card__title">监控</span>
        <div className="home-card__header-right">
          <div className="home-segmented">
            <button className={mode === "network" ? "is-active" : ""} onClick={() => setMode("network")}>流量</button>
            <button className={mode === "io" ? "is-active" : ""} onClick={() => setMode("io")}>磁盘 IO</button>
          </div>
          {mode === "network" ? (
            <label className="home-select"><span>网卡</span><select value="所有" disabled><option>所有</option></select><ChevronDown size={13} /></label>
          ) : (
            <span className="home-select home-select--static"><span>磁盘</span><b>所有</b></span>
          )}
        </div>
      </div>
      <div className="home-card__body">
        <div className="home-monitor__chart-wrap">
          <div className="home-monitor__tags">
            {mode === "network" ? (
              <>
                <span>上行: {formatBytes(data.networkRxBytesPerSecond)}/s</span>
                <span>下行: {formatBytes(data.networkTxBytesPerSecond)}/s</span>
                <span>总发送: {formatBytes(totalTx)}</span>
                <span>总接收: {formatBytes(totalRx)}</span>
              </>
            ) : (
              <>
                <span>读取: {formatBytes(data.ioReadBytesPerSecond ?? 0)}/s</span>
                <span>写入: {formatBytes(data.ioWriteBytesPerSecond ?? 0)}/s</span>
                <span>读写速率: {data.ioCountPerSecond ?? 0} 次/s</span>
                <span>延迟: {data.ioLatencyMs ?? 0} ms</span>
              </>
            )}
          </div>
          <ReactECharts option={chartOption} opts={{ renderer: "svg" }} style={{ height: 383 }} />
        </div>
      </div>
    </article>
  );
}

/** 右侧系统信息/备忘录轮播；点击编辑时初始化草稿，查询刷新不覆盖输入。 */
function DashboardCarousel({ data, memoQuery, saveMemo }: { data: SystemOverview; memoQuery: { data?: { content: string; updatedAt: string | null } | undefined; isLoading: boolean; error: unknown }; saveMemo: { mutate: (content: string) => void; isPending: boolean; error: unknown } }) {
  const [index, setIndex] = useState(0);
  const [showSensitive, setShowSensitive] = useState(true);
  const [memoEditing, setMemoEditing] = useState(false);
  const [memoDraft, setMemoDraft] = useState("");
  useEffect(() => {
    if (memoEditing) return;
    const timer = setInterval(() => setIndex((current) => (current + 1) % 2), 5000);
    return () => clearInterval(timer);
  }, [memoEditing]);
  const bootTime = new Date(new Date(data.currentTime).getTime() - data.uptimeSeconds * 1000).toLocaleString();
  const rows: Array<[string, string]> = [
    ["主机名称", showSensitive ? data.hostname : "****"],
    ["发行版本", `${data.osName} ${data.osVersion}`.trim() || data.osName],
    ["内核版本", data.kernel],
    ["系统类型", data.architecture],
    ["主机地址", showSensitive ? data.primaryIp : "****"],
    ["启动时间", bootTime],
    ["运行时间", formatDuration(data.uptimeSeconds)],
  ];
  return (
    <div className="home-carousel">
      {index === 0 && (
        <article className="home-card home-carousel__card">
          <div className="home-card__header">
            <span className="home-card__title">系统信息</span>
            <div className="home-card__header-right">
              <button className="home-icon-btn" onClick={() => setShowSensitive((value) => !value)} aria-label="切换敏感信息">{showSensitive ? <Eye size={14} /> : <EyeOff size={14} />}</button>
              <button className="home-icon-btn" onClick={() => window.navigator.clipboard?.writeText(rows.map(([label, value]) => `${label}: ${value}`).join("\n"))} aria-label="复制"><Copy size={14} /></button>
            </div>
          </div>
          <div className="home-card__body home-carousel__body">
            <table className="home-sysinfo">
              <tbody>{rows.map(([label, value]) => <tr key={label}><td>{label}</td><td>{value}</td></tr>)}</tbody>
            </table>
          </div>
        </article>
      )}
      {index === 1 && (
        <article className="home-card home-carousel__card">
          <div className="home-card__header">
            <span className="home-card__title">备忘录</span>
            <div className="home-card__header-right">
              {!memoEditing && <button className="home-icon-btn" onClick={() => { setMemoDraft(memoQuery.data?.content ?? ""); setMemoEditing(true); }} aria-label="编辑备忘录"><Edit size={14} /></button>}
              {memoEditing && <button className="home-icon-btn" onClick={() => { saveMemo.mutate(memoDraft); setMemoEditing(false); }} aria-label="保存"><Check size={14} /></button>}
              {memoEditing && <button className="home-icon-btn" onClick={() => setMemoEditing(false)} aria-label="取消"><X size={14} /></button>}
            </div>
          </div>
          <div className="home-card__body home-carousel__body">
            {memoEditing ? (
              <textarea className="home-memo-editor" maxLength={500} value={memoDraft} onChange={(event) => setMemoDraft(event.target.value)} placeholder="点击编辑按钮启用编辑" rows={9} />
            ) : (
              <p className="home-memo-content">{memoQuery.data?.content || "点击编辑按钮启用编辑"}</p>
            )}
            {!!(memoQuery.error || saveMemo.error) && <div className="form-error">{errorMessage(memoQuery.error ?? saveMemo.error)}</div>}
          </div>
        </article>
      )}
      <div className="home-carousel__indicator"><button className={index === 0 ? "is-active" : ""} aria-label="系统信息" onClick={() => setIndex(0)} /><button className={index === 1 ? "is-active" : ""} aria-label="备忘录" onClick={() => setIndex(1)} /></div>
    </div>
  );
}

/** 应用卡：已安装应用操作 + 推荐安装（对齐 web AppLauncher）。 */
const LAUNCHER_APPS: Array<{ key: string; name: string; description: string }> = [
  { key: "mysql", name: "MySQL", description: "开源关系型数据库" },
  { key: "redis", name: "Redis", description: "高性能的开源键值数据库" },
  { key: "deepseek-harness", name: "DeepSeek Harness", description: "DeepSeek 开源智能体开发环境" },
  { key: "maxkb", name: "MaxKB", description: "强大易用的企业级智能体平台" },
  { key: "openclaw", name: "OpenClaw", description: "开源、自托管的个人 AI 助理" },
];

function AppLauncherCard({ serverId, data, onServiceAction, onServiceLogs }: { serverId: string; data: SystemOverview; onServiceAction: (name: string, action: "start" | "stop" | "restart") => void; onServiceLogs: (name: string) => void }) {
  const navigate = useNavigate();
  return (
    <article className="home-card home-card--apps">
      <div className="home-card__header"><span className="home-card__title">应用</span></div>
      <div className="home-card__body">
        <div className="home-app-list">
          {data.nginx.installed && (
            <div className="home-app">
              <div className="home-app__media">{data.nginx.version ? "N" : "?"}</div>
              <div className="home-app__main">
                <div className="home-app__title">openresty</div>
                <div className="home-app__desc">版本: {data.nginx.version ?? "未知"}</div>
                <div className="home-app__actions">
                  <button className="home-app__action" onClick={() => onServiceAction("nginx", data.nginx.running ? "stop" : "start")}>{data.nginx.running ? "关闭" : "启动"}</button>
                  <button className="home-app__action" onClick={() => onServiceAction("nginx", "restart")}>重启</button>
                  <button className="home-app__action" onClick={() => navigate(`/servers/${serverId}/appstore`)}>更多</button>
                  <button className="home-app__action" onClick={() => onServiceLogs("nginx")}><FileText size={12} />日志</button>
                </div>
              </div>
              <button className="home-app__install" disabled={data.nginx.installed} onClick={() => navigate(`/servers/${serverId}/appstore`)}>安装</button>
            </div>
          )}
          {LAUNCHER_APPS.map((app) => (
            <div className="home-app" key={app.key}>
              <div className="home-app__media">{app.name.charAt(0)}</div>
              <div className="home-app__main">
                <div className="home-app__title">{app.name}</div>
                <div className="home-app__desc">{app.description}</div>
              </div>
              <button className="home-app__install" onClick={() => navigate(`/servers/${serverId}/appstore`)}>安装</button>
            </div>
          ))}
        </div>
      </div>
    </article>
  );
}
