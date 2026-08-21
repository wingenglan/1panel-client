import { useQuery } from "@tanstack/react-query";
import type { LucideIcon } from "lucide-react";
import {
  Bell,
  Bot,
  Boxes,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  CircleGauge,
  ClipboardList,
  Command,
  Container,
  Database,
  Gem,
  Globe2,
  Package,
  Plus,
  Search,
  Server,
  ServerCog,
  ShieldCheck,
  Settings2,
  TerminalSquare,
  Wrench,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { NoticeHost } from "../components/ui/NoticeHost";
import { ServerDialog } from "../features/servers/ServerDialog";
import { useTransferStore } from "../features/files/transferStore";
import { TaskCenter } from "../features/tasks/TaskCenter";
import { useCommandTaskStore } from "../features/tasks/taskStore";
import { api } from "../lib/api";
import { applyLocale, connectionStatusLabel, readLocale } from "../lib/i18n";
import type { ServerProfile } from "../types/server";

type PanelMenuItem = {
  label: string;
  icon: LucideIcon;
  suffix?: string;
  globalPath?: string;
  available: boolean;
  hint?: string;
};

const PANEL_MENU: PanelMenuItem[] = [
  { label: "概览", icon: CircleGauge, suffix: "", available: true },
  { label: "应用商店", icon: Package, suffix: "/appstore", available: true, hint: "官方 Compose 应用目录与安装" },
  { label: "AI", icon: Bot, globalPath: "/ai", available: true, hint: "OpenAI-compatible 模型与智能体" },
  { label: "网站", icon: Globe2, suffix: "/website", available: true, hint: "OpenResty/Nginx 静态站点、反向代理与证书" },
  { label: "数据库", icon: Database, suffix: "/database", available: true },
  { label: "容器", icon: Container, suffix: "/docker", available: true },
  { label: "系统", icon: ServerCog, suffix: "/operations", available: true },
  { label: "安全", icon: ShieldCheck, suffix: "/security", available: true, hint: "防火墙与 SSH 安全配置" },
  { label: "终端", icon: TerminalSquare, suffix: "/terminal", available: true },
  { label: "计划任务", icon: ClipboardList, suffix: "/cronjob", available: true },
  { label: "工具箱", icon: Wrench, suffix: "/tools", available: true },
  { label: "高级功能", icon: Gem, suffix: "/advanced", available: true, hint: "网站探活与 WAF 能力探测" },
  { label: "日志审计", icon: ClipboardList, suffix: "/logs", available: true },
  { label: "面板设置", icon: Settings2, globalPath: "/settings", available: true },
];

/** 渲染 1Panel 风格的全局导航，并把单节点入口扩展为多服务器切换器。 */
export function AppShell() {
  const navigate = useNavigate();
  const location = useLocation();
  const [addOpen, setAddOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [paletteQuery, setPaletteQuery] = useState("");
  const [tasksOpen, setTasksOpen] = useState(false);
  const [nodesOpen, setNodesOpen] = useState(false);
  const [collapsed, setCollapsed] = useState(false);
  const transferTasks = useTransferStore((state) => state.tasks);
  const commandTasks = useCommandTaskStore((state) => state.tasks);
  const activeTasks = transferTasks.filter((task) => task.status === "queued" || task.status === "running").length
    + commandTasks.filter((task) => task.status === "queued" || task.status === "running").length;
  const servers = useQuery({ queryKey: ["servers"], queryFn: api.listServers });
  const activeServerId = useMemo(() => location.pathname.match(/^\/servers\/([^/]+)/)?.[1] ?? "", [location.pathname]);
  const activeServer = servers.data?.find((server) => server.id === activeServerId);
  const activeConnection = useQuery({
    queryKey: ["connection", activeServerId],
    queryFn: () => api.connectionState(activeServerId),
    enabled: Boolean(activeServerId),
    refetchInterval: 5_000,
  });

  useEffect(() => {
    applyLocale(readLocale());
    const savedTheme = localStorage.getItem("1panel-client.theme") ?? "light";
    const prefersLight = window.matchMedia("(prefers-color-scheme: light)").matches;
    document.documentElement.dataset.theme = savedTheme === "system" ? (prefersLight ? "light" : "dark") : savedTheme;
  }, []);

  useEffect(() => {
    void api.listTasks().then((records) => useCommandTaskStore.getState().hydrate(records)).catch(() => undefined);
  }, []);

  useEffect(() => {
    /** 打开全局命令面板，同时避免浏览器接管 Ctrl/Cmd+K。 */
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((value) => !value);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <div className={`panel-shell ${collapsed ? "is-collapsed" : ""}`}>
      <aside className="panel-sidebar">
        <button className="panel-brand" onClick={() => navigate(activeServerId ? `/servers/${activeServerId}` : "/")} aria-label="返回概览">
          <span className="panel-brand__mark"><Boxes size={22} /></span>
          {!collapsed && <span><strong>1Panel</strong><small>Client</small></span>}
        </button>

        <nav className="panel-menu" aria-label="主功能导航">
          {PANEL_MENU.map((item) => (
            <PanelMenuLink key={item.label} item={item} serverId={activeServerId} collapsed={collapsed} />
          ))}
        </nav>

        <div className="panel-node-switcher">
          <button className="panel-node-switcher__trigger" onClick={() => setNodesOpen((value) => !value)} title={collapsed ? (activeServer?.name ?? "选择服务器") : undefined}>
            <span className={`panel-node-state is-${activeConnection.data?.status ?? "offline"}`} />
            {!collapsed && <span><strong>{activeServer?.name ?? "选择服务器"}</strong><small>{activeServer ? `${activeServer.username}@${activeServer.host}` : `${servers.data?.length ?? 0} 个节点`}</small></span>}
            {!collapsed && <ChevronDown size={14} />}
          </button>
          {nodesOpen && (
            <div className="panel-node-popover">
              <header><span>服务器节点</span><button onClick={() => setAddOpen(true)}><Plus size={14} /> 添加</button></header>
              <div className="panel-node-list">
                {(servers.data ?? []).map((server) => (
                  <NavLink key={server.id} to={`/servers/${server.id}`} onClick={() => setNodesOpen(false)}>
                    <Server size={15} /><span><strong>{server.name}</strong><small>{server.username}@{server.host}:{server.port}</small></span><ChevronRight size={13} />
                  </NavLink>
                ))}
                {!servers.isLoading && !servers.data?.length && <button className="panel-node-empty" onClick={() => setAddOpen(true)}>添加第一台 SSH 服务器</button>}
              </div>
            </div>
          )}
        </div>
      </aside>

      <button className="panel-collapse" onClick={() => setCollapsed((value) => !value)} aria-label={collapsed ? "展开侧栏" : "折叠侧栏"}>
        <ChevronLeft size={13} />
      </button>

      <div className="panel-main">
        <header className="panel-topbar">
          <div className="panel-topbar__context">
            <span>{activeServer ? "服务器" : "1Panel Client"}</span>
            {activeServer && <><ChevronRight size={12} /><strong>{activeServer.name}</strong></>}
          </div>
          <button className="panel-search-trigger" onClick={() => setPaletteOpen(true)}><Search size={14} /><span>搜索服务器或功能</span><kbd><Command size={10} /> K</kbd></button>
          <div className="panel-topbar__actions">
            {activeServerId && <button onClick={() => navigate(`/servers/${activeServerId}/files`)}>文件</button>}
            <button className="panel-icon-button" onClick={() => setTasksOpen(true)} aria-label="任务中心"><Bell size={16} />{activeTasks > 0 && <i>{activeTasks}</i>}</button>
            <button className="panel-add-server" onClick={() => setAddOpen(true)}><Plus size={14} /> 添加服务器</button>
          </div>
        </header>
        <main className="panel-workspace"><Outlet /></main>
        <footer className="panel-footer"><span>1Panel Client 0.1.0 · GPL-3.0</span><span>多服务器 SSH 直连 · 凭据本地保护</span><span>{activeServer ? connectionStatusLabel(activeConnection.data?.status) : "节点总览"}</span></footer>
      </div>

      <ServerDialog key={addOpen ? "add-open" : "add-closed"} open={addOpen} onOpenChange={setAddOpen} />
      {paletteOpen && (
        <div className="palette-backdrop" onMouseDown={() => setPaletteOpen(false)}>
          <div className="palette" onMouseDown={(event) => event.stopPropagation()}>
            <div className="palette__input"><Search size={18} /><input autoFocus value={paletteQuery} onChange={(event) => setPaletteQuery(event.target.value)} placeholder="输入服务器名称，或尝试“终端 生产”" /></div>
            <PaletteResults query={paletteQuery} servers={servers.data ?? []} onNavigate={(path) => { setPaletteOpen(false); setPaletteQuery(""); navigate(path); }} />
          </div>
        </div>
      )}
      <TaskCenter open={tasksOpen} onClose={() => setTasksOpen(false)} />
      <NoticeHost />
    </div>
  );
}

/** 根据当前节点和真实实现状态渲染可用链接或明确的未开放入口。 */
function PanelMenuLink({ item, serverId, collapsed }: { item: PanelMenuItem; serverId: string; collapsed: boolean }) {
  const Icon = item.icon;
  const path = item.globalPath ?? (serverId && item.suffix !== undefined ? `/servers/${serverId}${item.suffix}` : "");
  const title = item.hint ?? (collapsed ? item.label : undefined);
  if (!item.available || !path) {
    return <button className="panel-menu__item is-disabled" disabled title={title}><Icon size={18} /><span>{item.label}</span>{!collapsed && !item.available && <small>规划中</small>}</button>;
  }
  return <NavLink end={item.suffix === ""} className="panel-menu__item" to={path} title={title}><Icon size={18} /><span>{item.label}</span></NavLink>;
}

/** 将命令面板输入解析为已实现的服务器工作区导航命令。 */
function PaletteResults({ query, servers, onNavigate }: { query: string; servers: ServerProfile[]; onNavigate: (path: string) => void }) {
  const input = query.trim().toLocaleLowerCase();
  const mode = input === "nginx" || input.startsWith("nginx ") || input === "网站" || input.startsWith("网站 ") ? "nginx"
    : input === "docker" || input.startsWith("docker ") || input === "容器" || input.startsWith("容器 ") ? "docker"
      : input === "tools" || input.startsWith("tools ") || input === "工具" || input.startsWith("工具 ") ? "tools"
        : input === "terminal" || input.startsWith("terminal ") || input === "终端" || input.startsWith("终端 ") ? "terminal"
          : input === "logs" || input.startsWith("logs ") || input === "日志" || input.startsWith("日志 ") ? "logs"
            : input.startsWith("open files ") || input.startsWith("打开文件 ") ? "files"
              : input === "port" || input.startsWith("operations ") || input.startsWith("port ") || input === "系统" || input.startsWith("系统 ") ? "operations"
                : input === "security" || input.startsWith("security ") || input === "安全" || input.startsWith("安全 ") ? "security"
                  : input === "database" || input.startsWith("database ") || input === "db" || input.startsWith("db ") || input === "数据库" || input.startsWith("数据库 ") ? "database"
                    : input === "cron" || input.startsWith("cron ") || input === "cronjob" || input.startsWith("cronjob ") || input === "计划任务" || input.startsWith("计划任务 ") ? "cronjob"
                      : input === "app" || input.startsWith("app ") || input === "appstore" || input.startsWith("appstore ") || input === "应用商店" || input.startsWith("应用商店 ") ? "appstore"
                        : input === "advanced" || input.startsWith("advanced ") || input === "高级" || input.startsWith("高级 ") || input === "高级功能" || input.startsWith("高级功能 ") ? "advanced" : "overview";
  const needle = input.replace(/^(open files|打开文件|terminal|终端|nginx|网站|tools|工具|docker|容器|logs|日志|operations|port|系统|security|安全|database|db|数据库|cron|cronjob|计划任务|app|appstore|应用商店|advanced|高级|高级功能)\s+/, "");
  const matches = servers.filter((server) => `${server.name} ${server.host} ${server.username}`.toLocaleLowerCase().includes(needle));
  if (!matches.length) return <div className="palette__hint">没有匹配的服务器。请先添加节点，或换一个名称/地址搜索。</div>;
  const labels = { overview: "概览", files: "文件", terminal: "终端", nginx: "网站", tools: "工具箱", docker: "容器", logs: "日志审计", operations: "系统", security: "安全", database: "数据库", cronjob: "计划任务", appstore: "应用商店", advanced: "高级功能" } as const;
  return <div className="palette-results">{matches.slice(0, 8).map((server) => <button key={server.id} onClick={() => onNavigate(`/servers/${server.id}${mode === "overview" ? "" : `/${mode}`}`)}><span><strong>打开{labels[mode]}</strong><small>{server.name} · {server.host}</small></span><ChevronRight size={14} /></button>)}</div>;
}
