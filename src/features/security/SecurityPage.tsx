import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { KeyRound, LockKeyhole, Plus, RefreshCw, Save, Shield, ShieldCheck, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { NavLink, useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/errors";

type RootLogin = "yes" | "no" | "prohibit-password" | "forced-commands-only";
type FirewallForm = { protocol: "tcp" | "udp" | "any"; port: string; source: string; comment: string };
const initialFirewall: FirewallForm = { protocol: "tcp", port: "22", source: "any", comment: "" };

/** 展示真实 UFW/firewalld 与 sshd 配置，并把所有变更交给 Rust 安全边界。 */
export function SecurityPage() {
  const { serverId = "" } = useParams();
  const queryClient = useQueryClient();
  const security = useQuery({ queryKey: ["security", serverId], queryFn: () => api.security(serverId), enabled: Boolean(serverId) });
  const [firewallForm, setFirewallForm] = useState<FirewallForm>(initialFirewall);
  const [sshPort, setSshPort] = useState(22);
  const [passwordAuthentication, setPasswordAuthentication] = useState(true);
  const [pubkeyAuthentication, setPubkeyAuthentication] = useState(true);
  const [permitRootLogin, setPermitRootLogin] = useState<RootLogin>("prohibit-password");
  const firewall = useMutation({
    mutationFn: (input: { action: "add" | "delete"; protocol: FirewallForm["protocol"]; port: string; source?: string; comment?: string }) => api.firewallRuleAction({ serverId, ...input, confirmed: true }),
    onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["security", serverId] }); setFirewallForm(initialFirewall); },
  });
  const saveSsh = useMutation({
    mutationFn: () => api.saveSshSecurity({ serverId, port: sshPort, passwordAuthentication, pubkeyAuthentication, permitRootLogin, confirmed: true }),
    onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["security", serverId] }); },
  });

  useEffect(() => {
    const value = security.data?.ssh;
    if (!value) return;
    // Defer form hydration to the next paint so the query result synchronizes
    // the controlled inputs without cascading state updates in the effect body.
    const frame = window.requestAnimationFrame(() => {
      setSshPort(value.port);
      setPasswordAuthentication(value.passwordAuthentication ?? true);
      setPubkeyAuthentication(value.pubkeyAuthentication ?? true);
      if (value.permitRootLogin && ["yes", "no", "prohibit-password", "forced-commands-only"].includes(value.permitRootLogin)) {
        setPermitRootLogin(value.permitRootLogin as RootLogin);
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, [security.data?.ssh]);

  /** 提交一条防火墙允许规则前要求用户明确确认远端写入。 */
  const addRule = () => {
    if (!firewallForm.port.trim()) return;
    if (!window.confirm(`确认在 ${security.data?.firewall.backend ?? "远端防火墙"} 中放行 ${firewallForm.port}/${firewallForm.protocol}？`)) return;
    firewall.mutate({ action: "add", ...firewallForm, source: firewallForm.source.trim() || "any", comment: firewallForm.comment.trim() || undefined });
  };

  /** 删除一条规则前要求用户再次核对端口和来源，避免误封远程连接。 */
  const removeRule = (rule: { port: string; protocol: string; source: string }) => {
    if (!window.confirm(`确认删除 ${rule.port}/${rule.protocol}（来源：${rule.source}）？`)) return;
    const protocol = rule.protocol === "udp" ? "udp" : rule.protocol === "tcp" ? "tcp" : "any";
    firewall.mutate({ action: "delete", protocol, port: rule.port, source: rule.source });
  };

  /** 保存 sshd 安全配置并提醒端口变化可能导致新连接失败。 */
  const submitSsh = () => {
    const currentPort = security.data?.ssh.port;
    const warning = currentPort !== undefined && currentPort !== sshPort
      ? `SSH 端口将从 ${currentPort} 改为 ${sshPort}，请确认防火墙已放行新端口并保留当前会话。`
      : "确认保存 SSH 安全配置并 reload sshd？";
    if (window.confirm(warning)) saveSsh.mutate();
  };

  return <section className="security-page">
    <div className="workspace-header"><div><div className="breadcrumb">服务器 / <span>安全</span></div><h1>安全</h1><p>防火墙规则、SSH 登录策略与远程变更保护</p></div><div className="workspace-header__actions"><Button variant="secondary" onClick={() => security.refetch()} disabled={security.isFetching}><RefreshCw size={14} className={security.isFetching ? "spin" : ""} /> 刷新安全状态</Button></div></div>
    <nav className="workspace-tabs"><NavLink to={`/servers/${serverId}`}>概览</NavLink><NavLink to={`/servers/${serverId}/operations`}>系统</NavLink><NavLink className="active" to={`/servers/${serverId}/security`}>安全</NavLink><NavLink to={`/servers/${serverId}/services`}>服务</NavLink><NavLink to={`/servers/${serverId}/logs`}>日志</NavLink></nav>
    {security.isLoading && <div className="page-state">正在读取防火墙和 SSH 配置…</div>}
    {security.error && <div className="page-state page-state--error">{errorMessage(security.error)}</div>}
    {security.data && <>
      <div className="security-overview-grid"><article className="security-card"><header><div className="security-card__icon"><Shield size={19} /></div><div><span className="section-kicker">Firewall</span><h2>{security.data.firewall.backend === "none" ? "未安装防火墙" : security.data.firewall.backend}</h2></div><span className={security.data.firewall.enabled ? "status-chip status-chip--ok" : "status-chip status-chip--warn"}>{security.data.firewall.enabled ? "已启用" : "未启用"}</span></header><p>{security.data.firewall.defaultIncoming ? `默认入站：${security.data.firewall.defaultIncoming}` : "未读取到默认入站策略"} · {security.data.firewall.rules.length} 条规则</p></article><article className="security-card"><header><div className="security-card__icon"><KeyRound size={19} /></div><div><span className="section-kicker">SSH</span><h2>sshd 安全配置</h2></div><span className="status-chip status-chip--ok">端口 {security.data.ssh.port}</span></header><p>{security.data.ssh.passwordAuthentication === false ? "密码登录已关闭" : "密码登录已开启"} · {security.data.ssh.pubkeyAuthentication === false ? "公钥登录已关闭" : "公钥登录已开启"}</p></article></div>
      <div className="security-panels"><section className="security-panel"><header><div><span className="section-kicker">Firewall rules</span><h2>防火墙规则</h2><p>只改写检测到的 UFW 或 firewalld；nftables 当前提供只读摘要。</p></div><span>{security.data.firewall.rules.length} 条</span></header><div className="security-rule-form"><select value={firewallForm.protocol} onChange={(event) => setFirewallForm((value) => ({ ...value, protocol: event.target.value as FirewallForm["protocol"] }))}><option value="tcp">TCP</option><option value="udp">UDP</option><option value="any">TCP/UDP</option></select><input value={firewallForm.port} onChange={(event) => setFirewallForm((value) => ({ ...value, port: event.target.value }))} placeholder="端口或范围，如 80,443" /><input value={firewallForm.source} onChange={(event) => setFirewallForm((value) => ({ ...value, source: event.target.value }))} placeholder="来源，默认 any" /><input value={firewallForm.comment} onChange={(event) => setFirewallForm((value) => ({ ...value, comment: event.target.value }))} placeholder="备注（可选）" /><Button variant="primary" onClick={addRule} disabled={!security.data.firewall.installed || firewall.isPending}><Plus size={14} />放行</Button></div>{firewall.error && <div className="form-error">{errorMessage(firewall.error)}</div>}{security.data.firewall.rules.length ? <div className="security-rule-table"><div className="ops-head"><span>端口</span><span>协议</span><span>来源</span><span>动作</span><span /></div>{security.data.firewall.rules.map((rule) => <div className="ops-row" key={`${rule.id}-${rule.raw}`}><strong className="mono">{rule.port}</strong><span>{rule.protocol}</span><span className="mono">{rule.source}</span><span className="status-chip status-chip--ok">{rule.action}</span><Button variant="danger" size="sm" onClick={() => removeRule(rule)} disabled={firewall.isPending}><Trash2 size={13} />删除</Button></div>)}</div> : <div className="empty-panel empty-panel--small"><ShieldCheck size={21} /><span>没有解析到可管理规则。</span></div>}</section>
        <section className="security-panel"><header><div><span className="section-kicker">SSH hardening</span><h2>SSH 登录策略</h2><p>保存前执行 sshd -t，失败自动恢复原配置。</p></div><LockKeyhole size={20} /></header><div className="security-ssh-form"><label><span>SSH 端口</span><input type="number" min={1} max={65535} value={sshPort} onChange={(event) => setSshPort(Number(event.target.value))} /></label><label><span>Root 登录策略</span><select value={permitRootLogin} onChange={(event) => setPermitRootLogin(event.target.value as RootLogin)}><option value="prohibit-password">仅允许公钥</option><option value="no">禁止 root</option><option value="yes">允许密码和公钥</option><option value="forced-commands-only">仅强制命令</option></select></label><label className="check-field"><input type="checkbox" checked={passwordAuthentication} onChange={(event) => setPasswordAuthentication(event.target.checked)} /><span>允许密码登录</span></label><label className="check-field"><input type="checkbox" checked={pubkeyAuthentication} onChange={(event) => setPubkeyAuthentication(event.target.checked)} /><span>允许公钥登录</span></label><div className="security-note"><LockKeyhole size={17} /><span>保留当前 SSH 会话；修改端口或关闭密码前，先确认备用登录方式和防火墙规则。</span></div><Button variant="primary" onClick={submitSsh} disabled={saveSsh.isPending}><Save size={14} />{saveSsh.isPending ? "校验并 reload 中…" : "保存 SSH 配置"}</Button>{saveSsh.error && <div className="form-error">{errorMessage(saveSsh.error)}</div>}</div></section></div>
      {security.data.warnings.length > 0 && <div className="warning-panel"><ShieldCheck size={16} /><span>{security.data.warnings.join(" · ")}</span></div>}
    </>}
  </section>;
}
