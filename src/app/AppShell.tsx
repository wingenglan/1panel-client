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
  Container,
  Database,
  Gem,
  Globe2,
  Package,
  Plus,
  Search,
  Server,
  ServerCog,
  Settings2,
  TerminalSquare,
  Wrench,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { NoticeHost } from "../components/ui/NoticeHost";
import { ServerDialog } from "../features/servers/ServerDialog";
import { useTransferStore } from "../features/files/transferStore";
import { TaskCenter } from "../features/tasks/TaskCenter";
import { useCommandTaskStore } from "../features/tasks/taskStore";
import { api } from "../lib/api";
import { applyLocale, connectionStatusLabel, readLocale } from "../lib/i18n";
import type { ServerProfile } from "../types/server";

type PanelMenuChild = {
  label: string;
  suffix?: string;
  globalPath?: string;
  available: boolean;
  hint?: string;
};

type PanelMenuItem = {
  label: string;
  icon: LucideIcon;
  suffix?: string;
  globalPath?: string;
  children?: PanelMenuChild[];
  available: boolean;
  hint?: string;
};

/** 左侧主导航与 web 1Panel v2.2.5 对齐（分组默认折叠）；客户端额外能力收进对应子菜单。 */
const PANEL_MENU: PanelMenuItem[] = [
  { label: "概览", icon: CircleGauge, suffix: "", available: true },
  { label: "应用商店", icon: Package, suffix: "/appstore", available: true },
  {
    label: "AI",
    icon: Bot,
    available: true,
    children: [
      { label: "智能体", globalPath: "/ai", available: true },
      { label: "模型", globalPath: "/ai", available: true, hint: "进入智能体页可查看本地模型" },
      { label: "MCP", globalPath: "/ai", available: true, hint: "进入智能体页使用 MCP 服务" },
      { label: "GPU 监控", globalPath: "/ai", available: true, hint: "进入智能体页查看 GPU 使用情况" },
    ],
  },
  {
    label: "网站",
    icon: Globe2,
    available: true,
    children: [
      { label: "网站", suffix: "/website", available: true },
      { label: "证书", suffix: "/website/certificates", available: true },
      { label: "模板", suffix: "/website/templates", available: true },
      { label: "运行环境", suffix: "/website/runtimes", available: true },
    ],
  },
  { label: "数据库", icon: Database, suffix: "/database", available: true },
  { label: "容器", icon: Container, suffix: "/docker", available: true },
  {
    label: "系统",
    icon: ServerCog,
    available: true,
    children: [
      { label: "文件", suffix: "/files", available: true, hint: "SSH 文件管理（含文件夹预览）" },
      { label: "监控", suffix: "/operations", available: false, hint: "主机监控视图将在后续版本开放" },
      { label: "防火墙", suffix: "/security", available: true },
      { label: "进程管理", suffix: "/operations", available: true, hint: "端口与进程" },
      { label: "SSH 管理", suffix: "/security", available: true, hint: "SSH 安全配置" },
      { label: "磁盘管理", suffix: "/operations", available: false, hint: "磁盘挂载管理将在后续版本开放" },
      { label: "服务", suffix: "/services", available: true, hint: "systemd 服务状态" },
    ],
  },
  { label: "终端", icon: TerminalSquare, suffix: "/terminal", available: true },
  { label: "计划任务", icon: ClipboardList, suffix: "/cronjob", available: true },
  { label: "工具箱", icon: Wrench, suffix: "/tools", available: true },
  {
    label: "高级功能",
    icon: Gem,
    available: true,
    children: [
      { label: "APP", suffix: "/advanced", available: false, hint: "应用编排能力将在客户端后续实现" },
      { label: "WAF", suffix: "/advanced", available: true },
      { label: "多机管理", suffix: "/advanced", available: false, hint: "客户端本机多服务器管理已由节点切换器提供" },
      { label: "网站监控", suffix: "/advanced", available: true, hint: "可用性探活与定时监控" },
      { label: "资源同步", suffix: "/advanced", available: false, hint: "资源同步将在客户端后续实现" },
      { label: "AI 建站", suffix: "/advanced", available: false, hint: "AI 建站将在客户端后续实现" },
      { label: "网站防篡改", suffix: "/advanced", available: false, hint: "网站防篡改将在客户端后续实现" },
      { label: "应用高可用", suffix: "/advanced", available: false, hint: "应用高可用将在客户端后续实现" },
      { label: "界面设置", globalPath: "/settings", available: true },
    ],
  },
  { label: "日志审计", icon: ClipboardList, suffix: "/logs", available: true },
  { label: "面板设置", icon: Settings2, globalPath: "/settings", available: true },
];

/** 记录一次导航打开过的页面，web 版以内容页签条形式展示。 */
const OPENED_PAGES_KEY = "1panel-client.opened-pages";
const ACTIVE_SERVER_KEY = "1panel-client.active-server";

/** 从路径推导页签标题：web 版格式为「页面-子页」，如 容器-概览、应用-全部。 */
function tabTitle(path: string): string {
  if (path === "/settings") return "面板设置-面板";
  if (path === "/ai") return "智能体-对话";
  const labels: Record<string, string> = {
    appstore: "应用-全部", website: "网站-网站", "website/certificates": "网站-证书",
    "website/templates": "网站-模板", "website/runtimes": "网站-运行环境",
    database: "数据库-MySQL", docker: "容器-概览", terminal: "终端-终端",
    cronjob: "计划任务-计划任务", tools: "工具箱-快速设置", advanced: "高级功能-概览", logs: "日志审计-操作日志",
    files: "文件-文件", security: "防火墙-防火墙", operations: "进程管理-进程管理", services: "服务-服务",
  };
  const two = path.match(/^\/servers\/[^/]+\/([^/]+\/[^/]+)$/)?.[1];
  const segment = two ?? path.match(/^\/servers\/[^/]+\/([^/]+)$/)?.[1] ?? "";
  return labels[segment] ?? "概览";
}

/** 渲染多服务器导航和发行版本；路由变化时同步页签，关闭页签仅在点击事件中导航。 */
export function AppShell() {
  const navigate = useNavigate();
  const location = useLocation();
  const workspaceRef = useRef<HTMLElement | null>(null);
  useEffect(() => { workspaceRef.current?.scrollTo(0, 0); }, [location.pathname, location.key]);
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
  const activeServerId = useMemo(() => {
    const routed = location.pathname.match(/^\/servers\/([^/]+)/)?.[1] ?? "";
    if (routed) return routed;
    // 全局页（面板设置/智能体）没有服务器路径段：回退到上次使用的节点，保证侧栏仍可点击。
    const remembered = localStorage.getItem(ACTIVE_SERVER_KEY) ?? "";
    if (remembered && servers.data && servers.data.some((server) => server.id === remembered)) return remembered;
    return "";
  }, [location.pathname, servers.data]);
  const activeServer = servers.data?.find((server) => server.id === activeServerId);

  useEffect(() => {
    if (activeServerId) localStorage.setItem(ACTIVE_SERVER_KEY, activeServerId);
  }, [activeServerId]);

  const [openedPages, setOpenedPages] = useState<string[]>(() => {
    try {
      const stored = JSON.parse(localStorage.getItem(OPENED_PAGES_KEY) ?? "[]") as unknown;
      return Array.isArray(stored) ? stored.filter((path): path is string => typeof path === "string").slice(-12) : [];
    } catch {
      return [];
    }
  });
  const [pageTabs, setPageTabs] = useState(() => localStorage.getItem("1panel-client.pageTabs") !== "false");
  useEffect(() => {
    const onPrefs = () => setPageTabs(localStorage.getItem("1panel-client.pageTabs") !== "false");
    window.addEventListener("1panel-client:prefs", onPrefs);
    return () => window.removeEventListener("1panel-client:prefs", onPrefs);
  }, []);

  const [lastPath, setLastPath] = useState("");
  // 在同一次渲染中更新路由对应的页签，避免 effect 触发额外级联渲染。
  if (lastPath !== location.pathname) {
    setLastPath(location.pathname);
    if (location.pathname && location.pathname !== "/" && !openedPages.includes(location.pathname)) {
      setOpenedPages([...openedPages, location.pathname].slice(-12));
    }
  }

  useEffect(() => {
    localStorage.setItem(OPENED_PAGES_KEY, JSON.stringify(openedPages));
  }, [openedPages]);

  /** 关闭页签；关闭的是当前页时回到最后一个页签。 */
  const closePage = (path: string) => closePages([path]);
  /** 批量关闭页签，并在当前页被关闭后转到最后一个保留页。 */
  const closePages = (paths: string[]) => {
    if (!paths.length) return;
    const next = openedPages.filter((page) => !paths.includes(page));
    setOpenedPages(next);
    if (paths.includes(location.pathname)) navigate(next.length ? next[next.length - 1] : "/");
  };
  const [tabMenu, setTabMenu] = useState<{ path: string; x: number; y: number } | null>(null);
  useEffect(() => {
    if (!tabMenu) return;
    const close = () => setTabMenu(null);
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [tabMenu]);
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

        <div className="panel-quick-actions">
          <button className="panel-icon-button" onClick={() => setPaletteOpen(true)} aria-label="搜索服务器或功能" title="搜索服务器或功能（Ctrl+K）"><Search size={15} /></button>
          <button className="panel-icon-button" onClick={() => setTasksOpen(true)} aria-label="任务中心" title="任务中心"><Bell size={15} />{activeTasks > 0 && <i>{activeTasks}</i>}</button>
          <button className="panel-icon-button panel-quick-actions__add" onClick={() => setAddOpen(true)} aria-label="添加服务器" title="添加服务器"><Plus size={15} /></button>
        </div>
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
        {pageTabs && openedPages.length > 0 && (
          <nav className="panel-tabs" aria-label="已打开页面">
            {openedPages.map((path) => (
              <NavLink key={path} to={path} className={`panel-tab${location.pathname === path ? " is-active" : ""}`} aria-current={location.pathname === path ? "page" : undefined} onAuxClick={(event) => { if (event.button === 1) { event.preventDefault(); closePage(path); } }} onContextMenu={(event) => { event.preventDefault(); setTabMenu({ path, x: event.clientX, y: event.clientY }); }}>
                <span>{tabTitle(path)}</span>
              </NavLink>
            ))}
          </nav>
        )}
        {tabMenu && (
          <div className="tab-context-menu" style={{ left: tabMenu.x, top: tabMenu.y }} onContextMenu={(event) => event.preventDefault()} onMouseDown={(event) => event.stopPropagation()}>
            <button onClick={() => { closePage(tabMenu.path); setTabMenu(null); }}>关闭</button>
            <button disabled={openedPages[0] === tabMenu.path} onClick={() => { const index = openedPages.indexOf(tabMenu.path); closePages(openedPages.slice(0, index)); setTabMenu(null); }}>关闭左侧</button>
            <button disabled={openedPages.length <= 1} onClick={() => { closePages(openedPages.filter((page) => page !== tabMenu.path)); setTabMenu(null); }}>关闭其它</button>
          </div>
        )}
        <main ref={workspaceRef} className="panel-workspace"><Outlet /></main>
        <footer className="panel-footer"><span>1Panel Client 0.1.1 · GPL-3.0</span><span>多服务器 SSH 直连 · 凭据本地保护</span><span>{activeServer ? connectionStatusLabel(activeConnection.data?.status) : "节点总览"}</span></footer>
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

/** 根据当前节点和真实实现状态渲染可用链接或明确的未开放入口；分组默认折叠，与 web 版 el-sub-menu 一致。 */
function PanelMenuLink({ item, serverId, collapsed }: { item: PanelMenuItem; serverId: string; collapsed: boolean }) {
  const Icon = item.icon;
  const [groupOpen, setGroupOpen] = useState(false);
  const title = item.hint ?? (collapsed ? item.label : undefined);
  if (item.children) {
    return (
      <div className={`panel-menu__group${groupOpen ? " is-open" : ""}`}>
        <button className="panel-menu__item panel-menu__group-toggle" onClick={() => setGroupOpen((value) => !value)} aria-expanded={groupOpen} title={title}>
          <Icon size={18} /><span>{item.label}</span><ChevronDown size={13} className="panel-menu__chevron" />
        </button>
        {groupOpen && !collapsed && (
          <div className="panel-menu__sub">
            {item.children.map((child) => <PanelMenuChildLink key={`${item.label}:${child.label}`} label={item.label} child={child} serverId={serverId} />)}
          </div>
        )}
      </div>
    );
  }
  const path = item.globalPath ?? (serverId && item.suffix !== undefined ? `/servers/${serverId}${item.suffix}` : "");
  if (!item.available || !path) {
    return <button className="panel-menu__item is-disabled" disabled title={title}><Icon size={18} /><span>{item.label}</span>{!collapsed && !item.available && <small>规划中</small>}</button>;
  }
  return <NavLink end={item.suffix === ""} className="panel-menu__item" to={path} title={title}><Icon size={18} /><span>{item.label}</span></NavLink>;
}

/** 渲染子菜单项；未开放入口明确显示为规划中，避免与 web 版形成误导。 */
function PanelMenuChildLink({ label, child, serverId }: { label: string; child: PanelMenuChild; serverId: string }) {
  const path = child.globalPath ?? (serverId && child.suffix !== undefined ? `/servers/${serverId}${child.suffix}` : "");
  if (!child.available || !path) {
    return <button className="panel-menu__child is-disabled" disabled title={child.hint ?? `${label} · ${child.label}`}><span>{child.label}</span><small>规划中</small></button>;
  }
  return <NavLink className="panel-menu__child" to={path} title={child.hint ?? `${label} · ${child.label}`}><span>{child.label}</span></NavLink>;
}

/** 将命令面板输入解析为已实现的服务器工作区导航命令。 */
function PaletteResults({ query, servers, onNavigate }: { query: string; servers: ServerProfile[]; onNavigate: (path: string) => void }) {
  const input = query.trim().toLocaleLowerCase();
  const mode = input === "nginx" || input.startsWith("nginx ") || input === "网站" || input.startsWith("网站 ") ? "website"
    : input === "docker" || input.startsWith("docker ") || input === "容器" || input.startsWith("容器 ") ? "docker"
      : input === "tools" || input.startsWith("tools ") || input === "工具" || input.startsWith("工具 ") || input === "工具箱" || input.startsWith("工具箱 ") ? "tools"
        : input === "terminal" || input.startsWith("terminal ") || input === "终端" || input.startsWith("终端 ") ? "terminal"
          : input === "logs" || input.startsWith("logs ") || input === "日志" || input.startsWith("日志 ") || input === "日志审计" || input.startsWith("日志审计 ") ? "logs"
            : input.startsWith("open files ") || input.startsWith("打开文件 ") || input.startsWith("文件 ") || input === "文件" ? "files"
              : input === "port" || input.startsWith("operations ") || input.startsWith("port ") || input === "进程" || input.startsWith("进程 ") ? "operations"
                : input === "security" || input.startsWith("security ") || input === "安全" || input.startsWith("安全 ") || input === "防火墙" || input.startsWith("防火墙 ") ? "security"
                  : input === "database" || input.startsWith("database ") || input === "db" || input.startsWith("db ") || input === "数据库" || input.startsWith("数据库 ") ? "database"
                    : input === "cron" || input.startsWith("cron ") || input === "cronjob" || input.startsWith("cronjob ") || input === "计划任务" || input.startsWith("计划任务 ") ? "cronjob"
                      : input === "app" || input.startsWith("app ") || input === "appstore" || input.startsWith("appstore ") || input === "应用商店" || input.startsWith("应用商店 ") ? "appstore"
                        : input === "advanced" || input.startsWith("advanced ") || input === "高级" || input.startsWith("高级 ") || input === "高级功能" || input.startsWith("高级功能 ") ? "advanced" : "overview";
  const needle = input.replace(/^(open files|打开文件|文件|terminal|终端|nginx|网站|tools|工具箱|工具|docker|容器|logs|日志|日志审计|operations|port|进程|security|安全|防火墙|database|db|数据库|cron|cronjob|计划任务|app|appstore|应用商店|advanced|高级|高级功能)\s+/, "");
  const matches = servers.filter((server) => `${server.name} ${server.host} ${server.username}`.toLocaleLowerCase().includes(needle));
  if (!matches.length) return <div className="palette__hint">没有匹配的服务器。请先添加节点，或换一个名称/地址搜索。</div>;
  const labels = { overview: "概览", files: "文件", terminal: "终端", website: "网站", tools: "工具箱", docker: "容器", logs: "日志审计", operations: "系统", security: "安全", database: "数据库", cronjob: "计划任务", appstore: "应用商店", advanced: "高级功能" } as const;
  return <div className="palette-results">{matches.slice(0, 8).map((server) => <button key={server.id} onClick={() => onNavigate(`/servers/${server.id}${mode === "overview" ? "" : `/${mode}`}`)}><span><strong>打开{labels[mode]}</strong><small>{server.name} · {server.host}</small></span><ChevronRight size={14} /></button>)}</div>;
}
