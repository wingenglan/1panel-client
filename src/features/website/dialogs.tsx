import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ShieldAlert } from "lucide-react";
import { useState } from "react";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import type { CertificateActionResult, WebsiteSnapshot } from "../../types/server";

type WebsiteForm = { domain: string; kind: "static" | "proxy"; listenPort: number; rootPath: string; phpRuntime: string; phpSocket: string; upstreamScheme: "http" | "https"; upstreamHost: string; upstreamPort: number; enableHttps: boolean; httpsPort: number; certificatePath: string; certificateKeyPath: string };
type CertificateForm = { domain: string; email: string; webroot: string; action: "issue" | "renew"; challenge: "http01" | "dns01"; dnsProvider: "cloudflare" | "aliyun" | "dnspod" | "tencent" | "aws"; dnsApiToken: string };

const initialWebsiteForm: WebsiteForm = { domain: "demo.example.com", kind: "static", listenPort: 80, rootPath: "", phpRuntime: "", phpSocket: "", upstreamScheme: "http", upstreamHost: "127.0.0.1", upstreamPort: 8080, enableHttps: false, httpsPort: 443, certificatePath: "", certificateKeyPath: "" };

/** 站点或证书变更后刷新站点快照和续期计划。 */
function invalidateWebsiteQueries(queryClient: ReturnType<typeof useQueryClient>, serverId: string) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: ["websites", serverId] }),
    queryClient.invalidateQueries({ queryKey: ["certificate-renewal-plan"] }),
  ]);
}

/** 创建网站弹窗：由父页在打开时挂载，初始化独立草稿，保存后测试并 reload。 */
export function WebsiteCreateDialog({ serverId, snapshot, open, onClose }: { serverId: string; snapshot?: WebsiteSnapshot; open: boolean; onClose: () => void }) {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<WebsiteForm>(initialWebsiteForm);
  const save = useMutation({
    mutationFn: () => api.saveWebsite({ serverId, domain: form.domain, kind: form.kind, listenPort: form.listenPort, rootPath: form.kind === "static" ? form.rootPath || undefined : undefined, phpRuntime: form.kind === "static" ? form.phpRuntime || undefined : undefined, phpSocket: form.kind === "static" ? form.phpSocket || undefined : undefined, upstreamScheme: form.kind === "proxy" ? form.upstreamScheme : undefined, upstreamHost: form.kind === "proxy" ? form.upstreamHost : undefined, upstreamPort: form.kind === "proxy" ? form.upstreamPort : undefined, enableHttps: form.enableHttps, httpsPort: form.httpsPort, certificatePath: form.certificatePath || undefined, certificateKeyPath: form.certificateKeyPath || undefined, confirmed: true }),
    onSuccess: async () => { onClose(); await invalidateWebsiteQueries(queryClient, serverId); },
  });
  if (!open) return null;
  const submit = () => { if (window.confirm(`确认保存网站 ${form.domain} 并 reload Nginx/OpenResty？`)) save.mutate(); };
  return <div className="website-form-backdrop" onMouseDown={onClose}><section className="website-form-card" onMouseDown={(event) => event.stopPropagation()}><header><div><span className="section-kicker">New website</span><h2>创建网站</h2><p>写入独立 site-*.conf，测试通过后 reload。</p></div><button type="button" className="icon-control" onClick={onClose} aria-label="关闭">×</button></header>
    <div className="website-form-grid"><label><span>域名</span><input value={form.domain} onChange={(event) => setForm((value) => ({ ...value, domain: event.target.value }))} placeholder="example.com" /></label><label><span>类型</span><select value={form.kind} onChange={(event) => setForm((value) => ({ ...value, kind: event.target.value as WebsiteForm["kind"] }))}><option value="static">静态站点</option><option value="proxy">反向代理</option></select></label><label><span>HTTP 端口</span><input type="number" min={1} max={65535} value={form.listenPort} onChange={(event) => setForm((value) => ({ ...value, listenPort: Number(event.target.value) }))} /></label>
    {form.kind === "static" ? <><label className="website-form-wide"><span>站点根目录（可留空使用默认映射）</span><input value={form.rootPath} onChange={(event) => setForm((value) => ({ ...value, rootPath: event.target.value }))} placeholder="/www/sites/example.com" /></label><label className="website-form-wide"><span>PHP-FPM 运行时（可选）</span><select value={form.phpRuntime} onChange={(event) => setForm((value) => ({ ...value, phpRuntime: event.target.value }))}><option value="">不绑定 PHP-FPM</option>{snapshot?.phpRuntimes.filter((runtime) => runtime.service).map((runtime) => <option key={runtime.id} value={runtime.id}>{runtime.id}{runtime.version ? ` · PHP ${runtime.version}` : ""}{runtime.running ? " · 运行中" : " · 已停止"}</option>)}</select><small>保存后生成 FastCGI 配置；服务端会重新探测 socket。</small></label><label className="website-form-wide"><span>容器内 PHP socket（可选，覆盖自动探测）</span><input value={form.phpSocket} onChange={(event) => setForm((value) => ({ ...value, phpSocket: event.target.value }))} placeholder="/tmp/php-cgi/app.sock" /><small>容器化 OpenResty 无法直连宿主机 socket 时，填写 Nginx 容器内可见的 FastCGI 路径，服务端会校验并直接使用。</small></label></> : <><label><span>上游协议</span><select value={form.upstreamScheme} onChange={(event) => setForm((value) => ({ ...value, upstreamScheme: event.target.value as WebsiteForm["upstreamScheme"] }))}><option value="http">http</option><option value="https">https</option></select></label><label><span>上游主机</span><input value={form.upstreamHost} onChange={(event) => setForm((value) => ({ ...value, upstreamHost: event.target.value }))} /></label><label><span>上游端口</span><input type="number" min={1} max={65535} value={form.upstreamPort} onChange={(event) => setForm((value) => ({ ...value, upstreamPort: Number(event.target.value) }))} /></label></>}
    <label className="check-field website-form-wide"><input type="checkbox" checked={form.enableHttps} onChange={(event) => setForm((value) => ({ ...value, enableHttps: event.target.checked }))} /><span>启用 HTTPS（证书必须已存在）</span></label>
    {form.enableHttps && <><label><span>HTTPS 端口</span><input type="number" min={1} max={65535} value={form.httpsPort} onChange={(event) => setForm((value) => ({ ...value, httpsPort: Number(event.target.value) }))} /></label><label><span>证书路径</span><input value={form.certificatePath} onChange={(event) => setForm((value) => ({ ...value, certificatePath: event.target.value }))} placeholder="/www/sites/example/cert/fullchain.pem" /></label><label><span>私钥路径</span><input value={form.certificateKeyPath} onChange={(event) => setForm((value) => ({ ...value, certificateKeyPath: event.target.value }))} placeholder="/www/sites/example/cert/privkey.pem" /></label></>}</div>
    {save.error && <div className="form-error">{errorMessage(save.error)}</div>}
    <footer><Button variant="ghost" onClick={onClose}>取消</Button><Button variant="primary" onClick={submit} disabled={save.isPending || !form.domain.trim()}>{save.isPending ? "测试并 reload 中…" : "保存网站"}</Button></footer>
  </section></div>;
}

/** 证书弹窗仅在打开时挂载；初始化申请/续期草稿，查询刷新不覆盖用户输入。 */
export function SslCertificateDialog({ serverId, snapshot, open, onClose, defaults }: { serverId: string; snapshot?: WebsiteSnapshot; open: boolean; onClose: () => void; defaults?: { domain?: string; action?: "issue" | "renew" } }) {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<CertificateForm>(() => ({ domain: defaults?.domain ?? snapshot?.websites[0]?.domain ?? "demo.example.com", email: "", webroot: snapshot?.runtimeRoot ?? snapshot?.hostRoot ?? "/var/www", action: defaults?.action ?? "issue", challenge: "http01", dnsProvider: "cloudflare", dnsApiToken: "" }));
  const [autoBind, setAutoBind] = useState(false);
  const bind = useMutation({ mutationFn: (input: { domain: string; certificatePath: string; certificateKeyPath: string }) => api.bindWebsiteCertificate({ serverId, ...input, confirmed: true }), onSuccess: async () => { onClose(); await invalidateWebsiteQueries(queryClient, serverId); } });
  const certificate = useMutation({ mutationFn: () => api.certificateAction({ serverId, ...form, dnsProvider: form.challenge === "dns01" ? form.dnsProvider : undefined, dnsApiToken: form.challenge === "dns01" ? form.dnsApiToken : undefined, confirmed: true }), onSuccess: async (result: CertificateActionResult) => {
    const target = snapshot?.websites.find((website) => website.domain === result.domain && website.enabled);
    if (autoBind && target) { bind.mutate({ domain: result.domain, certificatePath: result.certificatePath, certificateKeyPath: result.certificateKeyPath }); return; }
    onClose();
    await invalidateWebsiteQueries(queryClient, serverId);
    if (autoBind) window.alert(`证书已生成（${result.certificatePath}），未找到同域启用的受控站点，请在「创建网站」中绑定。`);
  } });
  if (!open) return null;
  const submit = () => { if (window.confirm(`确认对 ${form.domain} 执行 ACME ${form.action === "issue" ? "申请" : "续期"}${autoBind ? "并自动绑定同域站点" : ""}？`)) certificate.mutate(); };
  return <div className="website-form-backdrop" onMouseDown={onClose}><section className="website-form-card" onMouseDown={(event) => event.stopPropagation()}><header><div><span className="section-kicker">ACME / HTTP-01 / DNS-01</span><h2>SSL 证书</h2><p>远端会调用 certbot 或 acme.sh，完成后自动回填 HTTPS 路径。</p></div><button type="button" className="icon-control" onClick={onClose} aria-label="关闭">×</button></header>
    <div className="website-form-grid"><label><span>域名</span><input value={form.domain} onChange={(event) => setForm((value) => ({ ...value, domain: event.target.value }))} placeholder="example.com" /></label><label><span>邮箱</span><input type="email" value={form.email} onChange={(event) => setForm((value) => ({ ...value, email: event.target.value }))} placeholder="ops@example.com" /></label><label><span>操作</span><select value={form.action} onChange={(event) => setForm((value) => ({ ...value, action: event.target.value as CertificateForm["action"] }))}><option value="issue">申请新证书</option><option value="renew">续期证书</option></select></label><label><span>验证方式</span><select value={form.challenge} onChange={(event) => setForm((value) => ({ ...value, challenge: event.target.value as CertificateForm["challenge"] }))}><option value="http01">HTTP-01</option><option value="dns01">DNS-01（Cloudflare/阿里云/DNSPod/腾讯云/AWS）</option></select></label>
    {form.challenge === "http01" ? <label className="website-form-wide"><span>HTTP-01 webroot</span><input value={form.webroot} onChange={(event) => setForm((value) => ({ ...value, webroot: event.target.value }))} placeholder="/www/sites/example.com" /></label> : <><label><span>DNS provider</span><select value={form.dnsProvider} onChange={(event) => setForm((value) => ({ ...value, dnsProvider: event.target.value as CertificateForm["dnsProvider"] }))}><option value="cloudflare">Cloudflare（certbot/acme.sh）</option><option value="aliyun">阿里云（acme.sh）</option><option value="dnspod">DNSPod（acme.sh）</option><option value="tencent">腾讯云（acme.sh）</option><option value="aws">AWS Route 53（acme.sh）</option></select></label><label><span>{form.dnsProvider === "aliyun" ? "阿里云 AccessKeyId:AccessKeySecret" : form.dnsProvider === "dnspod" ? "DNSPod ID:Token" : form.dnsProvider === "tencent" ? "腾讯云 SecretId:SecretKey" : form.dnsProvider === "aws" ? "AWS AccessKeyId:SecretAccessKey" : "Cloudflare API Token"}</span><input type="password" autoComplete="new-password" value={form.dnsApiToken} onChange={(event) => setForm((value) => ({ ...value, dnsApiToken: event.target.value }))} placeholder={form.dnsProvider === "aliyun" ? "仅本次操作使用，例如 LTAI...:..." : form.dnsProvider === "dnspod" ? "仅本次操作使用，例如 123456:Token" : form.dnsProvider === "tencent" ? "仅本次操作使用，例如 AKID...:..." : form.dnsProvider === "aws" ? "仅本次操作使用，例如 AKIA...:..." : "仅本次操作使用"} /></label></>}
    <label className="check-field website-form-wide"><input type="checkbox" checked={autoBind} onChange={(event) => setAutoBind(event.target.checked)} /><span>成功后自动绑定到同域的已启用受控站点（没有目标时仅生成证书）</span></label></div>
    {certificate.error && <div className="form-error">{errorMessage(certificate.error)}</div>}{bind.error && <div className="form-error">{errorMessage(bind.error)}</div>}
    <div className="security-note"><span>证书私钥只写入远端证书目录；DNS token 通过 SFTP 写入远端 0600 临时文件，操作结束立即删除，不会保存到本地。</span></div>
    <footer><Button variant="ghost" onClick={onClose}>取消</Button><Button variant="primary" onClick={submit} disabled={certificate.isPending || bind.isPending || !form.domain.trim() || !form.email.trim() || (form.challenge === "dns01" && !form.dnsApiToken.trim())}>{certificate.isPending ? "ACME 操作中…" : bind.isPending ? "绑定证书中…" : "执行证书操作"}</Button></footer>
  </section></div>;
}

/** 打开时查询当前节点的 PHP 安装计划，展示真实候选后由用户确认安装。 */
export function PhpInstallDialog({ serverId, open, onClose }: { serverId: string; open: boolean; onClose: () => void }) {
  const queryClient = useQueryClient();
  const planQuery = useQuery({ queryKey: ["php-install-plan", serverId], queryFn: () => api.phpInstallPlan(serverId), enabled: open && !!serverId });
  const plan = planQuery.data;
  const loading = planQuery.isLoading;
  const install = useMutation({ mutationFn: () => api.installPhp({ serverId, confirmed: true }), onSuccess: async () => { onClose(); await invalidateWebsiteQueries(queryClient, serverId); } });
  if (!open) return null;
  const submit = () => { if (window.confirm("确认安装 PHP-FPM 及常用扩展？")) install.mutate(); };
  return <div className="website-form-backdrop" onMouseDown={onClose}><section className="website-form-card" onMouseDown={(event) => event.stopPropagation()}><header><div><span className="section-kicker">PHP-FPM runtime</span><h2>安装 PHP-FPM</h2><p>先查看远端包管理器和服务候选，再执行安装。</p></div><button type="button" className="icon-control" onClick={onClose} aria-label="关闭">×</button></header>
    {loading && <div className="page-state">正在读取远端 PHP 安装计划…</div>}
    {planQuery.error && <div className="form-error">{errorMessage(planQuery.error)}</div>}
    {!loading && !plan && <div className="page-state">暂无安装计划，点击右上角重新读取。</div>}
    {plan && <div className="server-form"><div className="field-grid field-grid--2"><div><span className="field-label">包管理器</span><strong>{plan.packageManager}</strong></div><div><span className="field-label">服务候选</span><strong>{plan.services.join("、")}</strong></div></div><label><span>软件包</span><input readOnly value={plan.packages.join(", ")} /></label><label><span>远端命令</span><textarea readOnly rows={4} value={plan.command} /></label><div className="security-note"><ShieldAlert size={18} /><span>{plan.risk}</span></div>{install.error && <div className="form-error">{errorMessage(install.error)}</div>}<footer><Button variant="ghost" onClick={onClose}>取消</Button><Button variant="primary" onClick={submit} disabled={install.isPending}>{install.isPending ? "安装中…" : "确认安装"}</Button></footer></div>}
  </section></div>;
}
