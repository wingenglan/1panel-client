import { ArrowRight, Container, Globe2, ServerCog, Waypoints } from "lucide-react";
import { type ReactNode } from "react";
import { NavLink, useParams } from "react-router-dom";

/** 展示服务器服务目录，让 systemd、Nginx 和 Docker 的归属关系清晰可见。 */
export function ServicesPage() {
  const { serverId = "" } = useParams();

  return <section className="services-page">
    <div className="service-catalog">
      <header className="service-catalog__header"><div><span className="section-kicker">服务模块</span><h2>运行中的服务，都从这里开始</h2></div><span>3 个服务域</span></header>
      <div className="service-catalog__grid">
        <ServiceCard icon={<ServerCog size={22} />} eyebrow="系统服务" title="systemd 服务" description="查看单元状态、进程、端口和服务日志，执行启动、停止与重启。" meta="端口与进程" to={`/servers/${serverId}/operations`} />
        <ServiceCard icon={<Globe2 size={22} />} eyebrow="Web 服务" title="Nginx" description="管理反向代理、配置测试、证书信息和安全 reload。" meta="反向代理" to={`/servers/${serverId}/nginx`} />
        <ServiceCard icon={<Container size={22} />} eyebrow="容器服务" title="Docker" description="查看容器、镜像、卷、网络和 Compose 项目，并执行受控操作。" meta="容器与镜像" to={`/servers/${serverId}/docker`} />
      </div>
    </div>
    <div className="service-catalog__footnote"><Waypoints size={17} /><span>工具中心用于探测和安装依赖；服务目录用于进入具体运行时管理页面。</span><NavLink to={`/servers/${serverId}/tools`}>查看工具中心 <ArrowRight size={15} /></NavLink></div>
  </section>;
}

/** 绘制一个可进入具体服务管理页的目录卡片。 */
function ServiceCard({ icon, eyebrow, title, description, meta, to }: { icon: ReactNode; eyebrow: string; title: string; description: string; meta: string; to: string }) {
  return <NavLink className="service-card" to={to}><div className="service-card__icon">{icon}</div><div className="service-card__body"><span>{eyebrow}</span><h3>{title}</h3><p>{description}</p><small>{meta}</small></div><ArrowRight className="service-card__arrow" size={19} /></NavLink>;
}
