import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { lazy, Suspense } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { AppShell } from "./app/AppShell";
import { ServerLandingPage } from "./features/servers/ServerLandingPage";
import { SettingsPage } from "./features/settings/SettingsPage";
import { ServicesPage } from "./features/services/ServicesPage";
import { AiPage } from "./features/ai/AiPage";
import "./App.css";

const OverviewPage = lazy(() => import("./features/overview/OverviewPage").then((module) => ({ default: module.OverviewPage })));
const FilesPage = lazy(() => import("./features/files/FilesPage").then((module) => ({ default: module.FilesPage })));
const TerminalPage = lazy(() => import("./features/terminal/TerminalPage").then((module) => ({ default: module.TerminalPage })));
const OperationsPage = lazy(() => import("./features/operations/OperationsPage").then((module) => ({ default: module.OperationsPage })));
const ToolsPage = lazy(() => import("./features/tools/ToolsPage").then((module) => ({ default: module.ToolsPage })));
const NginxPage = lazy(() => import("./features/nginx/NginxPage").then((module) => ({ default: module.NginxPage })));
const DockerPage = lazy(() => import("./features/docker/DockerPage").then((module) => ({ default: module.DockerPage })));
const LogsPage = lazy(() => import("./features/logs/LogsPage").then((module) => ({ default: module.LogsPage })));
const DatabasePage = lazy(() => import("./features/database/DatabasePage").then((module) => ({ default: module.DatabasePage })));
const CronjobPage = lazy(() => import("./features/cronjob/CronjobPage").then((module) => ({ default: module.CronjobPage })));
const AppStorePage = lazy(() => import("./features/appstore/AppStorePage").then((module) => ({ default: module.AppStorePage })));
const SecurityPage = lazy(() => import("./features/security/SecurityPage").then((module) => ({ default: module.SecurityPage })));
const WebsitePage = lazy(() => import("./features/website/WebsitePage").then((module) => ({ default: module.WebsitePage })));
const CertificatesPage = lazy(() => import("./features/website/CertificatesPage").then((module) => ({ default: module.CertificatesPage })));
const TemplatesPage = lazy(() => import("./features/website/TemplatesPage").then((module) => ({ default: module.TemplatesPage })));
const RuntimesPage = lazy(() => import("./features/website/RuntimesPage").then((module) => ({ default: module.RuntimesPage })));
const AdvancedPage = lazy(() => import("./features/advanced/AdvancedPage").then((module) => ({ default: module.AdvancedPage })));

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false },
  },
});

/** 组装全局查询缓存、路由和按功能拆分的服务器工作区页面。 */
export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
            <Route element={<AppShell />}>
            <Route index element={<ServerLandingPage />} />
            <Route path="servers/:serverId" element={<Suspense fallback={<div className="page-state">正在载入概览…</div>}><OverviewPage /></Suspense>} />
            <Route path="servers/:serverId/files" element={<Suspense fallback={<div className="page-state">正在载入文件工作台…</div>}><FilesPage /></Suspense>} />
            <Route path="servers/:serverId/terminal" element={<Suspense fallback={<div className="page-state">正在载入终端…</div>}><TerminalPage /></Suspense>} />
            <Route path="servers/:serverId/operations" element={<Suspense fallback={<div className="page-state">正在载入运行现场…</div>}><OperationsPage /></Suspense>} />
            <Route path="servers/:serverId/services" element={<ServicesPage />} />
            <Route path="servers/:serverId/tools" element={<Suspense fallback={<div className="page-state">正在载入工具中心…</div>}><ToolsPage /></Suspense>} />
            <Route path="servers/:serverId/nginx" element={<Suspense fallback={<div className="page-state">正在载入 Nginx…</div>}><NginxPage /></Suspense>} />
            <Route path="servers/:serverId/docker" element={<Suspense fallback={<div className="page-state">正在载入 Docker…</div>}><DockerPage /></Suspense>} />
            <Route path="servers/:serverId/logs" element={<Suspense fallback={<div className="page-state">正在载入日志中心…</div>}><LogsPage /></Suspense>} />
            <Route path="servers/:serverId/database" element={<Suspense fallback={<div className="page-state">正在载入数据库…</div>}><DatabasePage /></Suspense>} />
            <Route path="servers/:serverId/cronjob" element={<Suspense fallback={<div className="page-state">正在载入计划任务…</div>}><CronjobPage /></Suspense>} />
            <Route path="servers/:serverId/appstore" element={<Suspense fallback={<div className="page-state">正在载入应用商店…</div>}><AppStorePage /></Suspense>} />
            <Route path="servers/:serverId/security" element={<Suspense fallback={<div className="page-state">正在载入安全中心…</div>}><SecurityPage /></Suspense>} />
            <Route path="servers/:serverId/website" element={<Suspense fallback={<div className="page-state">正在载入网站…</div>}><WebsitePage /></Suspense>} />
            <Route path="servers/:serverId/website/certificates" element={<Suspense fallback={<div className="page-state">正在载入证书…</div>}><CertificatesPage /></Suspense>} />
            <Route path="servers/:serverId/website/templates" element={<Suspense fallback={<div className="page-state">正在载入模板…</div>}><TemplatesPage /></Suspense>} />
            <Route path="servers/:serverId/website/runtimes" element={<Suspense fallback={<div className="page-state">正在载入运行环境…</div>}><RuntimesPage /></Suspense>} />
            <Route path="servers/:serverId/advanced" element={<Suspense fallback={<div className="page-state">正在载入高级功能…</div>}><AdvancedPage /></Suspense>} />
            <Route path="settings" element={<SettingsPage />} />
            <Route path="ai" element={<AiPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
