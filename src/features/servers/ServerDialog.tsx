import * as Dialog from "@radix-ui/react-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Eye, EyeOff, ShieldCheck, X } from "lucide-react";
import { type FormEvent, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import type { AuthType, SaveServerInput, ServerProfile, SudoMode } from "../../types/server";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  profile?: ServerProfile | null;
}

const initial: SaveServerInput = {
  name: "", description: "", host: "", port: 22, username: "root",
  authType: "password", sudoMode: "none", tags: [], favorite: false,
};

/** 将持久化的服务器档案映射为编辑表单，并保留可选 ProxyJump 引用。 */
function formFor(profile?: ServerProfile | null): SaveServerInput {
  if (!profile) return initial;
  return {
    id: profile.id,
    name: profile.name,
    description: profile.description,
    host: profile.host,
    port: profile.port,
    username: profile.username,
    authType: profile.authType,
    privateKeyPath: profile.privateKeyPath ?? undefined,
    sudoMode: profile.sudoMode,
    groupId: profile.groupId ?? undefined,
    proxyJumpId: profile.proxyJumpId ?? undefined,
    tags: profile.tags,
    favorite: profile.favorite,
  };
}

/** 沿本地服务器档案解析跳板链，供表单展示多级路径并标记循环候选。 */
function proxyJumpPath(serverId: string, servers: ServerProfile[]): ServerProfile[] {
  const byId = new Map(servers.map((server) => [server.id, server]));
  const visited = new Set<string>();
  const path: ServerProfile[] = [];
  let current: string | null = serverId;
  while (current && !visited.has(current)) {
    visited.add(current);
    const server = byId.get(current);
    if (!server) break;
    path.push(server);
    current = server.proxyJumpId;
  }
  return path;
}

/** 编辑服务器连接参数、分组和跳板关系，并提交到本地安全存储。 */
export function ServerDialog({ open, onOpenChange, profile }: Props) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const groups = useQuery({ queryKey: ["server-groups"], queryFn: api.listServerGroups });
  const servers = useQuery({ queryKey: ["servers"], queryFn: api.listServers });
  const [form, setForm] = useState(() => formFor(profile));
  const [showSecret, setShowSecret] = useState(false);
  const serverOptions = servers.data ?? [];
  const selectedProxyPath = form.proxyJumpId ? proxyJumpPath(form.proxyJumpId, serverOptions) : [];
  const mutation = useMutation({
    mutationFn: api.saveServer,
    onSuccess: async (server) => {
      await queryClient.invalidateQueries({ queryKey: ["servers"] });
      setForm(formFor());
      onOpenChange(false);
      navigate(`/servers/${server.id}`);
    },
  });
  const set = <K extends keyof SaveServerInput>(key: K, value: SaveServerInput[K]) =>
    setForm((current) => ({ ...current, [key]: value }));
  const submit = (event: FormEvent) => {
    event.preventDefault();
    mutation.mutate({ ...form, name: form.name.trim(), host: form.host.trim(), username: form.username.trim() });
  };

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="dialog-content">
          <div className="dialog-header">
            <div><Dialog.Title>{profile ? "编辑 SSH 服务器" : "添加 SSH 服务器"}</Dialog.Title><Dialog.Description>连接信息保存在本机，密码只进入系统安全存储。</Dialog.Description></div>
            <Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close>
          </div>
          <form onSubmit={submit} className="server-form">
            <div className="field-grid field-grid--2">
              <label><span>显示名称</span><input required value={form.name} onChange={(e) => set("name", e.target.value)} placeholder="例如：生产 Web 01" /></label>
              <label><span>标签</span><input value={form.tags.join(",")} onChange={(e) => set("tags", e.target.value.split(",").map((v) => v.trim()).filter(Boolean))} placeholder="生产, 华东" /></label>
            </div>
            <div className="field-grid field-grid--host">
              <label><span>主机地址</span><input required value={form.host} onChange={(e) => set("host", e.target.value)} placeholder="192.0.2.10" /></label>
              <label><span>端口</span><input required type="number" min={1} max={65535} value={form.port} onChange={(e) => set("port", Number(e.target.value))} /></label>
            </div>
            <div className="field-grid field-grid--2">
              <label><span>用户名</span><input required value={form.username} onChange={(e) => set("username", e.target.value)} /></label>
              <label><span>认证方式</span><select value={form.authType} onChange={(e) => set("authType", e.target.value as AuthType)}><option value="password">密码</option><option value="private_key">私钥文件</option><option value="ssh_agent">SSH Agent / Pageant</option></select></label>
            </div>
            {form.authType === "password" && (
              <label><span>SSH 密码{profile ? "（留空则不修改）" : ""}</span><div className="secret-input"><input required={!profile} type={showSecret ? "text" : "password"} value={form.password ?? ""} onChange={(e) => set("password", e.target.value)} autoComplete="new-password" /><button type="button" onClick={() => setShowSecret((v) => !v)}>{showSecret ? <EyeOff size={16} /> : <Eye size={16} />}</button></div></label>
            )}
            {form.authType === "private_key" && <div className="field-grid field-grid--2"><label><span>私钥路径</span><input required value={form.privateKeyPath ?? ""} onChange={(e) => set("privateKeyPath", e.target.value)} placeholder="C:\\Users\\me\\.ssh\\id_ed25519" /></label><label><span>私钥口令{profile ? "（留空则不修改）" : ""}</span><input type="password" value={form.privateKeyPassphrase ?? ""} onChange={(e) => set("privateKeyPassphrase", e.target.value)} autoComplete="new-password" /></label></div>}
            {form.authType === "ssh_agent" && <div className="security-note"><ShieldCheck size={18} /><span>连接时读取本机 SSH Agent / Pageant 中的身份；请先运行 <code>ssh-add</code>，密钥不会复制到应用。</span></div>}
            <div className="field-grid field-grid--2">
              <label><span>sudo 模式</span><select value={form.sudoMode} onChange={(e) => set("sudoMode", e.target.value as SudoMode)}><option value="none">不使用 sudo</option><option value="passwordless">免密 sudo</option><option value="password">使用 sudo 密码</option></select></label>
              {form.sudoMode === "password" && <label><span>sudo 密码</span><input type="password" value={form.sudoPassword ?? ""} onChange={(e) => set("sudoPassword", e.target.value)} autoComplete="new-password" /></label>}
            </div>
            <label><span>服务器分组</span><select value={form.groupId ?? ""} onChange={(e) => set("groupId", e.target.value || undefined)}><option value="">未分组</option>{groups.data?.map((group) => <option key={group.id} value={group.id}>{group.name}</option>)}</select></label>
            <label><span>ProxyJump 跳板服务器</span><select value={form.proxyJumpId ?? ""} onChange={(e) => set("proxyJumpId", e.target.value || undefined)}><option value="">不使用跳板</option>{serverOptions.filter((server) => server.id !== profile?.id).map((server) => { const path = proxyJumpPath(server.id, serverOptions); const createsCycle = Boolean(profile && path.some((item) => item.id === profile.id)); return <option key={server.id} value={server.id} disabled={createsCycle}>{server.name} · {server.host}{path.length > 1 ? ` · 链路 ${path.map((item) => item.name).join(" → ")}` : ""}{createsCycle ? "（会形成循环）" : ""}</option>; })}</select><small>支持多级跳板；连接前会再次校验链路并拒绝循环引用。</small>{selectedProxyPath.length > 0 && <small>当前链路：{selectedProxyPath.map((server) => server.name).join(" → ")}</small>}</label>
            <label className="check-field"><input type="checkbox" checked={form.favorite} onChange={(event) => set("favorite", event.target.checked)} /><span>加入收藏并置顶显示</span></label>
            <div className="security-note"><ShieldCheck size={18} /><span><strong>Host Key 校验默认开启</strong>首次连接会显示指纹，只有确认信任后才会认证。</span></div>
            {mutation.error && <div className="form-error">{errorMessage(mutation.error)}</div>}
            <div className="dialog-actions"><Dialog.Close asChild><Button type="button" variant="ghost">取消</Button></Dialog.Close><Button type="submit" variant="primary" disabled={mutation.isPending}>{mutation.isPending ? "保存中…" : profile ? "保存修改" : "保存并继续"}</Button></div>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
