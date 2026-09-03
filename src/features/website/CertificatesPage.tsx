import { useQuery } from "@tanstack/react-query";
import { RefreshCw, Search, Settings2, ChevronDown, Download, Trash2, Info, LockKeyhole } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { Pager } from "../../components/ui/Pager";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { pushNotice } from "../../lib/noticeStore";
import { SslCertificateDialog } from "./dialogs";
import { useWebsiteSnapshot } from "./useWebsiteSnapshot";

const RENEW_KEY = "1panel-client.cert-autorenew";
const REMARKS_KEY = "1panel-client.cert-remarks";

function readRecord(key: string): Record<string, string> {
  try { const parsed = JSON.parse(localStorage.getItem(key) ?? "{}") as unknown; return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed as Record<string, string> : {}; } catch { return {}; }
}

type CertRow = { domain: string; issueMethod: string; acmeAccount: string; status: string; statusCls: "is-ok" | "is-warn" | "is-down"; expiresAt: string | null; certificatePath: string | null; certificateKeyPath: string | null; action: "issue" | "renew"; reason: "missing" | "expiring" | "ok" };

/** 展示证书和续期入口；到期状态以远端快照时间计算，确保渲染结果稳定。 */
export function CertificatesPage() {
  const { serverId = "" } = useParams();
  const websites = useWebsiteSnapshot(serverId);
  const renewals = useQuery({ queryKey: ["certificate-renewal-plan", serverId, 30], queryFn: () => api.certificateRenewalPlan(serverId, 30), enabled: Boolean(serverId && websites.data?.supported) });
  const [certOpen, setCertOpen] = useState(false);
  const [certDefaults, setCertDefaults] = useState<{ domain?: string; action?: "issue" | "renew" }>({});
  const [detail, setDetail] = useState<CertRow | null>(null);
  const [query, setQuery] = useState("");
  const [moreMenu, setMoreMenu] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [autorenew, setAutorenew] = useState<Record<string, string>>(() => readRecord(RENEW_KEY));
  const [remarks, setRemarks] = useState<Record<string, string>>(() => readRecord(REMARKS_KEY));
  useEffect(() => localStorage.setItem(RENEW_KEY, JSON.stringify(autorenew)), [autorenew]);
  useEffect(() => localStorage.setItem(REMARKS_KEY, JSON.stringify(remarks)), [remarks]);

  const rows = useMemo<CertRow[]>(() => {
    const snapshot = websites.data;
    const list: CertRow[] = [];
    for (const website of snapshot?.websites ?? []) {
      if (!website.ssl) continue;
      const days = website.expiresAt && snapshot ? Math.ceil((new Date(website.expiresAt).getTime() - new Date(snapshot.fetchedAt).getTime()) / 86_400_000) : null;
      list.push({
        domain: website.domain,
        issueMethod: "网站SSL",
        acmeAccount: "—",
        status: days === null ? "正常" : days < 0 ? "已过期" : days <= 30 ? "即将过期" : "正常",
        statusCls: days === null ? "is-ok" : days < 0 ? "is-down" : days <= 30 ? "is-warn" : "is-ok",
        expiresAt: website.expiresAt,
        certificatePath: website.certificatePath,
        certificateKeyPath: null,
        action: days !== null && days <= 30 ? "renew" : "issue",
        reason: days === null ? "ok" : days < 0 ? "expiring" : days <= 30 ? "expiring" : "ok",
      });
    }
    for (const plan of renewals.data ?? []) {
      if (list.some((row) => row.domain === plan.domain)) continue;
      list.push({ domain: plan.domain, issueMethod: "ACME 策略", acmeAccount: "—", status: "待签发", statusCls: "is-warn", expiresAt: plan.expiresAt, certificatePath: plan.certificatePath, certificateKeyPath: null, action: plan.action, reason: plan.reason });
    }
    list.sort((a, b) => a.domain.localeCompare(b.domain));
    return list;
  }, [websites.data, renewals.data]);

  const rowsFiltered = useMemo(() => {
    const keyword = query.trim().toLowerCase();
    if (!keyword) return rows;
    return rows.filter((row) => row.domain.toLowerCase().includes(keyword) || row.issueMethod.toLowerCase().includes(keyword));
  }, [rows, query]);

  const openCertificate = (defaults?: { domain?: string; action?: "issue" | "renew" }) => { setCertDefaults(defaults ?? {}); setCertOpen(true); };
  const serverManagedHint = "由 1Panel 服务端管理，可在 Web 面板中操作";

  return <section className="website-page">
    <div className="web-toolbar">
      <div className="web-toolbar__left">
        <Button variant="primary" onClick={() => openCertificate()} disabled={!websites.data?.supported}>申请证书</Button>
        <Button variant="primary" onClick={() => pushNotice("info", "证书上传由 1Panel 服务端管理，可在 Web 面板中使用。")}>上传证书</Button>
        <Button variant="secondary" onClick={() => pushNotice("info", "自签证书由 1Panel 服务端管理，可在 Web 面板中使用。")}>自签证书</Button>
        <Button variant="secondary" onClick={() => pushNotice("info", `ACME 账号由 1Panel 服务端管理。客户端当前 ACME 工具：${websites.data?.certificateTools.certbot ? "certbot" : websites.data?.certificateTools.acmeSh ? "acme.sh" : "未检测到"}。`)}>Acme 账户</Button>
        <Button variant="secondary" onClick={() => pushNotice("info", "DNS 账户由 1Panel 服务端管理，可在 Web 面板中使用。")}>DNS 账户</Button>
        <Button variant="secondary" disabled title={serverManagedHint}>删除</Button>
      </div>
      <div className="web-toolbar__right">
        <label className="web-search"><Search size={13} /><input placeholder="搜索" value={query} onChange={(event) => setQuery(event.target.value)} /></label>
        <button className="icon-control" onClick={() => void Promise.all([websites.refetch(), renewals.refetch()])} disabled={websites.isFetching || renewals.isFetching} title="刷新"><RefreshCw size={15} className={websites.isFetching || renewals.isFetching ? "spin" : ""} /></button>
        <button className="icon-control" disabled title={serverManagedHint}><Settings2 size={15} /></button>
      </div>
    </div>

    {websites.isLoading && <div className="page-state">正在读取证书与站点信息…</div>}
    {websites.error && <div className="page-state page-state--error">{errorMessage(websites.error)}</div>}
    {renewals.error && <div className="page-state page-state--error">{errorMessage(renewals.error)}</div>}

    {websites.data && <div className="web-table-wrap">
      <div className="web-table">
        <div className="ops-head cert-grid">
          <span>ID</span><span>域名</span><span>其他域名</span><span>申请方式</span><span>Acme 账号</span><span>状态</span><span>日志</span><span>颁发组织</span><span>备注</span><span>自动续签</span><span>过期时间</span><span className="web-ops-cell">操作</span>
        </div>
        {rowsFiltered.length > 0 ? rowsFiltered.map((row, index) => <div className="ops-row cert-grid" key={row.domain}>
          <span className="cert-id">{index + 1}</span>
          <span className="web-kind">{row.domain}</span>
          <span className="web-muted">—</span>
          <span>{row.issueMethod}</span>
          <span className="web-muted">{row.acmeAccount}</span>
          <span><span className={`web-ssl-tag ${row.statusCls}`}>{row.status}</span></span>
          <span><button className="web-text-btn" disabled title={serverManagedHint}>查看</button></span>
          <span className="web-muted">—</span>
          <span><input className="web-remark-input" value={remarks[row.domain] ?? ""} placeholder="—" onChange={(event) => setRemarks((current) => ({ ...current, [row.domain]: event.target.value }))} /></span>
          <span><label className="web-switch"><input type="checkbox" checked={(autorenew[row.domain] ?? "1") === "1"} onChange={(event) => setAutorenew((current) => ({ ...current, [row.domain]: event.target.checked ? "1" : "0" }))} /><i /></label></span>
          <span className={row.expiresAt ? "" : "web-muted"}>{row.expiresAt ? new Date(row.expiresAt).toLocaleDateString() : "—"}</span>
          <div className="web-ops-cell">
            <button className="web-text-btn" onClick={() => setDetail(row)}>详情</button>
            <button className="web-text-btn" onClick={() => openCertificate({ domain: row.domain, action: row.action })}>{row.action === "issue" ? "申请" : "续期"}</button>
            <button className="web-text-btn" onClick={() => pushNotice("info", "证书编辑由 1Panel 服务端管理，可在 Web 面板中进行。")}>编辑</button>
            <span className="web-more-anchor">
              <button className="web-text-btn web-more-btn" onClick={() => setMoreMenu((current) => current === row.domain ? null : row.domain)}>更多 <ChevronDown size={12} /></button>
              {moreMenu === row.domain && <div className="web-row-menu" style={{ left: -150, top: 26 }} onMouseDown={(event) => event.stopPropagation()}>
                <button onClick={() => pushNotice("info", "证书下载由 1Panel 服务端管理，可在 Web 面板中使用。")}><Download size={13} /> 下载</button>
                <button onClick={() => pushNotice("info", "证书删除由 1Panel 服务端管理，可在 Web 面板中使用。")}><Trash2 size={13} /> 删除</button>
              </div>}
            </span>
          </div>
        </div>) : <div className="web-table-empty">暂无数据</div>}
      </div>
      <div className="web-table-pager"><Pager total={rowsFiltered.length} page={page} pageSize={pageSize} pageSizes={[20, 50, 100]} showEmpty onPageChange={setPage} onPageSizeChange={(size) => { setPageSize(size); setPage(1); }} /></div>
    </div>}

    {websites.data?.warnings.map((warning) => <div className="warning-panel" key={warning}><RefreshCw size={16} /><span>{warning}</span></div>)}

    {certOpen && <SslCertificateDialog key={serverId} serverId={serverId} snapshot={websites.data} open={certOpen} onClose={() => setCertOpen(false)} defaults={certDefaults} />}

    {detail && <div className="website-form-backdrop" onMouseDown={() => setDetail(null)}><section className="website-form-card website-group-card" onMouseDown={(event) => event.stopPropagation()}>
      <header><div><span className="section-kicker">Certificate detail</span><h2>{detail.domain}</h2><p>由客户端受控配置检测到的证书摘要。</p></div><button type="button" className="icon-control" onClick={() => setDetail(null)} aria-label="关闭">×</button></header>
      <div className="website-advanced-list">
        <div className="website-advanced-row"><div><strong>状态</strong><small>{detail.status}{detail.expiresAt ? ` · 到期 ${new Date(detail.expiresAt).toLocaleString()}` : " · 无到期信息"}</small></div></div>
        <div className="website-advanced-row"><div><strong>申请方式</strong><small>{detail.issueMethod}</small></div></div>
        <div className="website-advanced-row"><div><strong>ACME 工具</strong><small>{websites.data?.certificateTools.certbot ? "certbot" : websites.data?.certificateTools.acmeSh ? "acme.sh" : "未检测到"}</small></div></div>
        <div className="website-advanced-row"><div><strong>证书路径</strong><small className="mono">{detail.certificatePath ?? "—"}</small></div></div>
        <div className="website-advanced-row"><div><strong>私钥路径</strong><small className="mono">{detail.certificateKeyPath ?? "保存在远端证书目录"}</small></div></div>
        <div className="security-note"><Info size={16} /><span>证书私钥只存在于远端；客户端仅记录路径与到期信息。</span></div>
      </div>
      <footer><Button variant="secondary" size="sm" onClick={() => openCertificate({ domain: detail.domain, action: detail.action })}><LockKeyhole size={13} /> 续期证书</Button><Button variant="ghost" size="sm" onClick={() => setDetail(null)}>关闭</Button></footer>
    </section></div>}
  </section>;
}
