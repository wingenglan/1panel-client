import { RefreshCw, FileText, Trash2, Info } from "lucide-react";
import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { Pager } from "../../components/ui/Pager";
import { errorMessage } from "../../lib/errors";
import type { PhpRuntime } from "../../types/server";
import { PhpInstallDialog } from "./dialogs";
import { useWebsiteSnapshot } from "./useWebsiteSnapshot";

const LANGS = [
  { key: "PHP", label: "PHP" },
  { key: "Java", label: "Java" },
  { key: "Node.js", label: "Node.js" },
  { key: "Go", label: "Go" },
  { key: "Python", label: "Python" },
  { key: ".NET", label: ".NET" },
];
const REMARKS_KEY = "1panel-client.runtime-remarks";

function readRemarks(): Record<string, string> {
  try { const parsed = JSON.parse(localStorage.getItem(REMARKS_KEY) ?? "{}") as unknown; return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed as Record<string, string> : {}; } catch { return {}; }
}

/** web 版 网站-运行环境 页：语言标签（PHP 可用）+ 运行环境列表；空数据时
 *  保留空态表头与分页条；按需挂载当前节点的 PHP 安装弹窗。 */
export function RuntimesPage() {
  const { serverId = "" } = useParams();
  const [autoRefresh, setAutoRefresh] = useState(true);
  const websites = useWebsiteSnapshot(serverId, { refetchInterval: autoRefresh ? 15_000 : false });
  const [lang, setLang] = useState("PHP");
  const [phpOpen, setPhpOpen] = useState(false);
  const [detail, setDetail] = useState<PhpRuntime | null>(null);
  const [remarks, setRemarks] = useState<Record<string, string>>(() => readRemarks());
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  useEffect(() => localStorage.setItem(REMARKS_KEY, JSON.stringify(remarks)), [remarks]);
  const runtimes = websites.data?.phpRuntimes ?? [];
  const paged = runtimes.slice((page - 1) * pageSize, page * pageSize);

  return <section className="website-page">
    <div className="page-tabbar"><nav className="docker-tabs runtime-tabs">{LANGS.map((item) => <button key={item.key} className={lang === item.key ? "is-active" : ""} onClick={() => { setLang(item.key); setPage(1); }}>{item.label}</button>)}</nav></div>

    <div className="web-toolbar">
      <div className="web-toolbar__left">
        {lang === "PHP"
          ? <>
            <Button variant="primary" onClick={() => setPhpOpen(true)} disabled={!websites.data?.supported}>创建</Button>
            <Button variant="secondary" disabled title="PHP 扩展模版由 1Panel 服务端管理，可在 Web 面板中维护">扩展模版</Button>
            <Button variant="secondary" disabled title="构建缓存由 1Panel 服务端管理，可在 Web 面板中清理">清理构建缓存</Button>
          </>
          : <Button variant="primary" disabled title={`${lang} 运行环境由 1Panel 服务端管理，可在 Web 面板中创建`}>创建</Button>}
      </div>
      <div className="web-toolbar__right">
        <button className="icon-control" onClick={() => void websites.refetch()} disabled={websites.isFetching} title="刷新"><RefreshCw size={15} className={websites.isFetching ? "spin" : ""} /></button>
        <Button variant="secondary" onClick={() => setAutoRefresh((value) => !value)} title={autoRefresh ? "点击后停止自动刷新" : "点击后按 15 秒间隔自动刷新"}>{autoRefresh ? "不刷新" : "自动刷新"}</Button>
      </div>
    </div>
    {websites.isLoading && <div className="page-state">正在读取 PHP-FPM 运行环境…</div>}
    {websites.error && <div className="page-state page-state--error">{errorMessage(websites.error)}</div>}

    {websites.data && <div className="web-table-wrap">
      <div className="web-table">
        <div className="ops-head rt-grid"><span>名称</span><span>目录</span><span>来源</span><span>版本</span><span>镜像</span><span>端口</span><span>状态</span><span>日志</span><span>备注</span><span>时间</span><span className="web-ops-cell">操作</span></div>
        {paged.length > 0 ? paged.map((runtime) => <div className="ops-row rt-grid" key={runtime.id}>
          <span className="web-kind">{runtime.id}</span>
          <span className="web-muted mono">{runtime.socketPath || runtime.binary || "—"}</span>
          <span>客户端安装</span>
          <span>{runtime.version ? `PHP ${runtime.version}` : "—"}</span>
          <span className="web-muted">—</span>
          <span className="web-muted">—</span>
          <span><span className={`web-ssl-tag ${runtime.installed && runtime.running ? "is-ok" : runtime.installed ? "is-warn" : "is-down"}`}>{runtime.running ? "运行中" : runtime.installed ? "已停止" : "未安装"}</span></span>
          <span><button className="web-text-btn" disabled title="日志由 1Panel 服务端管理，可在 Web 面板中查看"><FileText size={12} /> 检查</button></span>
          <span><input className="web-remark-input" value={remarks[runtime.id] ?? ""} placeholder="—" onChange={(event) => setRemarks((current) => ({ ...current, [runtime.id]: event.target.value }))} /></span>
          <span className="web-muted">{new Date(websites.data.fetchedAt).toLocaleDateString()}</span>
          <div className="web-ops-cell">
            <button className="web-text-btn" onClick={() => setDetail(runtime)}>详情</button>
            <button className="web-icon-btn" disabled title="编辑由 1Panel 服务端管理"><Trash2 size={13} /></button>
          </div>
        </div>) : <div className="web-table-empty">暂无数据</div>}
      </div>
      <div className="web-table-pager"><Pager total={runtimes.length} page={page} pageSize={pageSize} pageSizes={[20, 50, 100]} showEmpty onPageChange={setPage} onPageSizeChange={(size) => { setPageSize(size); setPage(1); }} /></div>
    </div>}

    {phpOpen && <PhpInstallDialog key={serverId} serverId={serverId} open={phpOpen} onClose={() => setPhpOpen(false)} />}

    {detail && <div className="website-form-backdrop" onMouseDown={() => setDetail(null)}><section className="website-form-card website-group-card" onMouseDown={(event) => event.stopPropagation()}>
      <header><div><span className="section-kicker">PHP-FPM detail</span><h2>{detail.id}</h2><p>运行时二进制与 socket 信息。</p></div><button type="button" className="icon-control" onClick={() => setDetail(null)} aria-label="关闭">×</button></header>
      <div className="website-advanced-list">
        <div className="website-advanced-row"><div><strong>版本</strong><small>{detail.version ? `PHP ${detail.version}` : "未知"}</small></div></div>
        <div className="website-advanced-row"><div><strong>服务</strong><small>{detail.service ?? "—"}</small></div></div>
        <div className="website-advanced-row"><div><strong>二进制</strong><small className="mono">{detail.binary ?? "—"}</small></div></div>
        <div className="website-advanced-row"><div><strong>Socket</strong><small className="mono">{detail.socketPath ?? "—"}</small></div></div>
        <div className="website-advanced-row"><div><strong>状态</strong><small>{detail.running ? "运行中" : detail.installed ? "已停止" : "未安装"}</small></div></div>
        <div className="security-note"><Info size={16} /><span>运行时由客户端在远端安装时生成；卸载与版本管理请通过 1Panel 服务端操作。</span></div>
      </div>
    </section></div>}
  </section>;
}
