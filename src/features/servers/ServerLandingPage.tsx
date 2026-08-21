import { useQuery } from "@tanstack/react-query";
import { Activity, ArrowRight, Plus, Server, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { connectionStatusLabel } from "../../lib/i18n";
import type { ServerProfile } from "../../types/server";
import { ServerDialog } from "./ServerDialog";

/** 展示客户端的多节点总览，并引导用户进入真实服务器工作区。 */
export function ServerLandingPage() {
  const [open, setOpen] = useState(false);
  const servers = useQuery({ queryKey: ["servers"], queryFn: api.listServers });
  const total = servers.data?.length ?? 0;

  return (
    <section className="node-overview-page">
      <header className="panel-page-heading">
        <div><span>节点</span><h1>服务器总览</h1><p>从一台桌面客户端管理所有 Linux 服务器。</p></div>
        <Button variant="primary" onClick={() => setOpen(true)}><Plus size={14} /> 添加服务器</Button>
      </header>

      <div className="node-summary-strip">
        <article><Server size={18} /><span><small>服务器</small><strong>{total}</strong></span></article>
        <article><Activity size={18} /><span><small>活动任务</small><strong>本地管理</strong></span></article>
        <article><ShieldCheck size={18} /><span><small>凭据策略</small><strong>系统安全存储</strong></span></article>
      </div>

      {servers.isLoading && <div className="panel-loading">正在读取本地服务器档案…</div>}
      {!servers.isLoading && !total && (
        <div className="node-empty-state">
          <span className="node-empty-state__icon"><Server size={28} /></span>
          <h2>添加第一台服务器</h2>
          <p>支持账号密码或私钥连接；首次连接会要求核对 SSH Host Key。</p>
          <Button variant="primary" onClick={() => setOpen(true)}>添加 SSH 服务器 <ArrowRight size={14} /></Button>
        </div>
      )}
      {Boolean(total) && <div className="node-card-grid">{servers.data?.map((server) => <ServerCard key={server.id} server={server} />)}</div>}

      <ServerDialog key={open ? "landing-open" : "landing-closed"} open={open} onOpenChange={setOpen} />
    </section>
  );
}

/** 显示单台服务器的本地档案与当前 SSH 会话状态。 */
function ServerCard({ server }: { server: ServerProfile }) {
  const navigate = useNavigate();
  const connection = useQuery({
    queryKey: ["connection", server.id],
    queryFn: () => api.connectionState(server.id),
    refetchInterval: 10_000,
  });
  const status = connection.data?.status ?? "offline";
  return (
    <button className="node-card" onClick={() => navigate(`/servers/${server.id}`)}>
      <span className="node-card__mark"><Server size={18} /></span>
      <span className="node-card__body"><strong>{server.name}</strong><small>{server.username}@{server.host}:{server.port}</small></span>
      <span className={`node-card__status is-${status}`}><i />{connectionStatusLabel(status)}</span>
      <ArrowRight size={15} />
    </button>
  );
}
