import * as Dialog from "@radix-ui/react-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Bot, Eraser, History, KeyRound, MessageSquare, Plug, Plus, Send, Settings2, Trash2, X } from "lucide-react";
import { useMemo, useState } from "react";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage, isAppError } from "../../lib/errors";
import type { AiConversation, AiProvider, McpServerConfig, McpProbeResult } from "../../types/server";
import { useCommandTaskStore } from "../tasks/taskStore";

type ChatMessage = { role: "system" | "user" | "assistant"; content: string };
type ProviderForm = { id?: string; name: string; baseUrl: string; model: string; apiKey: string; enabled: boolean; clearApiKey: boolean };
type McpForm = { id?: string; name: string; transport: "stdio" | "http"; command: string; args: string; url: string; authToken: string; clearAuthToken: boolean; enabled: boolean; allowWrite: boolean; timeoutSeconds: number };
type ChatRequest = { providerId: string; conversationId: string; messages: ChatMessage[]; taskId?: string };

/** 提供与 1Panel AI 对齐的供应商配置和真实模型聊天入口。 */
export function AiPage() {
  const queryClient = useQueryClient();
  const providers = useQuery({ queryKey: ["ai-providers"], queryFn: api.aiProviders });
  const mcpServers = useQuery({ queryKey: ["ai-mcp-servers"], queryFn: api.aiMcpServers });
  const servers = useQuery({ queryKey: ["servers"], queryFn: api.listServers });
  const [configOpen, setConfigOpen] = useState(false);
  const [mcpOpen, setMcpOpen] = useState(false);
  const [form, setForm] = useState<ProviderForm>({ name: "", baseUrl: "https://api.openai.com/v1", model: "gpt-4o-mini", apiKey: "", enabled: true, clearApiKey: false });
  const [selectedId, setSelectedId] = useState("");
  const [draft, setDraft] = useState<{ providerId: string; conversationId: string; messages: ChatMessage[] }>({ providerId: "", conversationId: "", messages: [] });
  const [input, setInput] = useState("");
  const [streamingText, setStreamingText] = useState("");
  const [chatTaskId, setChatTaskId] = useState<string | null>(null);
  const [agentMode, setAgentMode] = useState(false);
  const [agentServerId, setAgentServerId] = useState("");
  const [mcpEnabled, setMcpEnabled] = useState(false);
  const [mcpForm, setMcpForm] = useState<McpForm>({ name: "", transport: "stdio", command: "", args: "", url: "", authToken: "", clearAuthToken: false, enabled: true, allowWrite: false, timeoutSeconds: 15 });
  const [mcpProbe, setMcpProbe] = useState<McpProbeResult | null>(null);
  const addTask = useCommandTaskStore((state) => state.add);
  const markRunning = useCommandTaskStore((state) => state.running);
  const markSuccess = useCommandTaskStore((state) => state.success);
  const markFail = useCommandTaskStore((state) => state.fail);
  const markCancelled = useCommandTaskStore((state) => state.cancelled);
  const selected = useMemo(() => providers.data?.find((provider) => provider.id === selectedId) ?? providers.data?.[0], [providers.data, selectedId]);
  const conversations = useQuery({ queryKey: ["ai-conversations", selected?.id], queryFn: () => api.aiConversations(selected?.id), enabled: Boolean(selected?.id) });
  const draftMatchesProvider = draft.providerId === (selected?.id ?? "");
  const activeConversationId = draftMatchesProvider ? draft.conversationId : conversations.data?.[0]?.id ?? "";
  const messages = draftMatchesProvider ? draft.messages : conversations.data?.[0]?.messages ?? [];
  const save = useMutation({ mutationFn: api.saveAiProvider, onSuccess: async (provider) => { setConfigOpen(false); setForm({ id: undefined, name: "", baseUrl: "https://api.openai.com/v1", model: "gpt-4o-mini", apiKey: "", enabled: true, clearApiKey: false }); setSelectedId(provider.id); await queryClient.invalidateQueries({ queryKey: ["ai-providers"] }); } });
  const remove = useMutation({ mutationFn: api.deleteAiProvider, onSuccess: async () => { setSelectedId(""); await queryClient.invalidateQueries({ queryKey: ["ai-providers"] }); } });
  const modelProbe = useMutation({ mutationFn: api.aiModels });
  const saveMcp = useMutation({ mutationFn: api.saveAiMcpServer, onSuccess: async () => { setMcpOpen(false); setMcpForm({ name: "", transport: "stdio", command: "", args: "", url: "", authToken: "", clearAuthToken: false, enabled: true, allowWrite: false, timeoutSeconds: 15 }); setMcpProbe(null); await queryClient.invalidateQueries({ queryKey: ["ai-mcp-servers"] }); } });
  const removeMcp = useMutation({ mutationFn: api.deleteAiMcpServer, onSuccess: async () => { setMcpProbe(null); await queryClient.invalidateQueries({ queryKey: ["ai-mcp-servers"] }); } });
  const probeMcp = useMutation({ mutationFn: api.probeAiMcpServer, onSuccess: (result) => setMcpProbe(result) });
  const saveConversation = useMutation({ mutationFn: api.saveAiConversation, onSuccess: async (conversation) => { await queryClient.invalidateQueries({ queryKey: ["ai-conversations", conversation.providerId] }); } });
  const removeConversation = useMutation({ mutationFn: api.deleteAiConversation, onSuccess: async () => { setDraft({ providerId: selected?.id ?? "", conversationId: "", messages: [] }); await queryClient.invalidateQueries({ queryKey: ["ai-conversations", selected?.id] }); } });
  const clearConversations = useMutation({ mutationFn: () => api.clearAiConversations(selected?.id), onSuccess: async () => { setDraft({ providerId: selected?.id ?? "", conversationId: "", messages: [] }); await queryClient.invalidateQueries({ queryKey: ["ai-conversations", selected?.id] }); } });
  /** 保存当前消息列表；标题只取首条用户消息摘要，API key 永远不进入请求体。 */
  const persistConversation = (conversationId: string, providerId: string, next: ChatMessage[]) => {
    saveConversation.mutate({ id: conversationId, providerId, title: next.find((message) => message.role === "user")?.content.slice(0, 80) || "新对话", messages: next });
  };
  const chat = useMutation({
    mutationFn: (request: ChatRequest) => api.aiChatStream(request, (event) => {
      if (event.event === "delta") {
        if (request.taskId) markRunning(request.taskId);
        setStreamingText((current) => current + event.data.content);
      }
    }),
    onSuccess: (result, request) => { if (request.taskId) markSuccess(request.taskId); const next = [...request.messages, { role: "assistant" as const, content: result.content }]; setDraft({ providerId: request.providerId, conversationId: request.conversationId, messages: next }); persistConversation(request.conversationId, request.providerId, next); setStreamingText(""); },
    onError: (reason, request) => { if (request.taskId) { if (isAppError(reason) && reason.code === "CANCELLED") markCancelled(request.taskId); else markFail(request.taskId, errorMessage(reason)); } setStreamingText(""); },
    onSettled: (_result, _error, request) => { if (request.taskId) setChatTaskId(null); },
  });
  const agent = useMutation({ mutationFn: (request: ChatRequest & { serverId: string; mcpEnabled: boolean }) => api.aiAgent({ providerId: request.providerId, serverId: request.serverId, messages: request.messages, maxSteps: 4, mcpEnabled: request.mcpEnabled }), onSuccess: (result, request) => { const next = [...request.messages, { role: "assistant" as const, content: result.content }]; setDraft({ providerId: request.providerId, conversationId: request.conversationId, messages: next }); persistConversation(request.conversationId, request.providerId, next); } });

  /** 打开新增或编辑供应商弹窗，不把已经保存的 API key 回填到表单。 */
  const editProvider = (provider?: AiProvider) => {
    modelProbe.reset();
    setForm(provider ? { id: provider.id, name: provider.name, baseUrl: provider.baseUrl, model: provider.model, apiKey: "", enabled: provider.enabled, clearApiKey: false } : { id: undefined, name: "", baseUrl: "https://api.openai.com/v1", model: "gpt-4o-mini", apiKey: "", enabled: true, clearApiKey: false });
    setConfigOpen(true);
  };
  /** 打开 MCP 服务器编辑弹窗；远程认证令牌不会从后端回填。 */
  const editMcp = (server?: McpServerConfig) => {
    setMcpForm(server ? { id: server.id, name: server.name, transport: server.transport, command: server.command, args: server.args.join("\n"), url: server.url ?? "", authToken: "", clearAuthToken: false, enabled: server.enabled, allowWrite: server.allowWrite, timeoutSeconds: server.timeoutSeconds } : { name: "", transport: "stdio", command: "", args: "", url: "", authToken: "", clearAuthToken: false, enabled: true, allowWrite: false, timeoutSeconds: 15 });
    setMcpProbe(null);
    setMcpOpen(true);
  };
  /** 发送真实聊天请求，并先把用户消息放入本地对话状态。 */
  const send = () => {
    const content = input.trim();
    if (!content || !selected || chat.isPending || agent.isPending || (agentMode && !agentServerId)) return;
    setInput("");
    const conversationId = activeConversationId || crypto.randomUUID();
    const next = [...messages, { role: "user" as const, content }];
    setDraft({ providerId: selected.id, conversationId, messages: next });
    persistConversation(conversationId, selected.id, next);
    if (agentMode) agent.mutate({ providerId: selected.id, conversationId, serverId: agentServerId, messages: next, mcpEnabled }); else {
      const taskId = crypto.randomUUID();
      setChatTaskId(taskId);
      addTask({ id: taskId, type: "ai-chat", serverId: "", title: `AI 对话 · ${selected.name}`, status: "queued" });
      chat.mutate({ providerId: selected.id, conversationId, messages: next, taskId });
    }
  };
  /** 删除供应商及密钥前要求用户确认。 */
  const deleteProvider = (provider: AiProvider) => { if (window.confirm("删除 AI 供应商 " + provider.name + "？密钥也会从系统密钥链移除。")) remove.mutate(provider.id); };
  /** 切换到已保存的本地会话，消息只从当前供应商的历史列表恢复。 */
  const selectConversation = (conversation: AiConversation | undefined) => { setDraft({ providerId: selected?.id ?? "", conversationId: conversation?.id ?? "", messages: conversation?.messages ?? [] }); };
  /** 创建一个尚未落盘的新会话，首次发送消息时才写入 SQLite。 */
  const newConversation = () => { setDraft({ providerId: selected?.id ?? "", conversationId: "", messages: [] }); setStreamingText(""); };
  /** 删除当前会话前要求确认，避免误清理本地对话。 */
  const deleteConversation = () => { if (activeConversationId && window.confirm("删除当前本地 AI 对话？")) removeConversation.mutate(activeConversationId); };
  /** 清空当前供应商的全部本地对话历史。 */
  const clearConversationHistory = () => { if (selected && window.confirm(`清空 ${selected.name} 的全部本地对话？`)) clearConversations.mutate(); };

  return <section className="ai-page">
    <div className="workspace-header"><div><div className="breadcrumb">1Panel Client / <span>AI</span></div><h1>AI</h1><p>OpenAI-compatible 模型、Ollama、只读智能体与 MCP 工具</p></div><div className="workspace-header__actions"><Button variant="ghost" onClick={() => editMcp()}><Plug size={14} />MCP 工具</Button><Button variant="primary" onClick={() => editProvider()}><Plus size={14} />添加供应商</Button></div></div>
    {selected && <div className="ai-model-probe"><Button size="sm" variant="ghost" onClick={() => modelProbe.mutate(selected.id)} disabled={modelProbe.isPending}>{modelProbe.isPending ? "探测模型中…" : "探测可用模型"}</Button>{modelProbe.isError && <span className="form-error">{errorMessage(modelProbe.error)}</span>}{modelProbe.data && <span>{modelProbe.data.length ? `发现 ${modelProbe.data.length} 个模型：${modelProbe.data.slice(0, 8).map((model) => model.id).join("、")}` : "供应商未返回模型列表"}</span>}</div>}
    <div className="ai-layout">
      <aside className="ai-provider-panel"><header><div><span className="section-kicker">模型供应商</span><h2>Providers</h2></div><Settings2 size={18} /></header>{providers.isLoading && <div className="page-state">读取配置…</div>}{providers.error && <div className="form-error">{errorMessage(providers.error)}</div>}{(providers.data ?? []).map((provider) => <button className={"ai-provider-item " + (selected?.id === provider.id ? "is-active" : "")} key={provider.id} onClick={() => setSelectedId(provider.id)}><span className="ai-provider-item__icon"><Bot size={16} /></span><span><strong>{provider.name}</strong><small>{provider.model} · {provider.hasApiKey ? "已配置 key" : "无 key"}</small></span><i className={provider.enabled ? "is-enabled" : ""} /></button>)}{!providers.isLoading && !providers.data?.length && <div className="empty-panel empty-panel--small"><KeyRound size={20} /><span>添加第一个模型供应商。</span></div>}</aside>
    <section className="ai-chat-panel"><header><div><span className="section-kicker">Assistant</span><h2>{selected?.name ?? "未选择供应商"}</h2></div>{selected && <div className="ai-chat-meta"><span>{selected.model}</span><label className="ai-conversation-select"><History size={13} /><select aria-label="选择本地对话" value={activeConversationId} onChange={(event) => selectConversation(conversations.data?.find((conversation) => conversation.id === event.target.value))}><option value="">新对话</option>{(conversations.data ?? []).map((conversation) => <option value={conversation.id} key={conversation.id}>{conversation.title}</option>)}</select></label><Button size="sm" variant="ghost" onClick={newConversation}><Plus size={13} />新对话</Button><Button size="sm" variant="ghost" onClick={clearConversationHistory} disabled={clearConversations.isPending}><Eraser size={13} />清空</Button><Button size="sm" variant="ghost" onClick={() => editProvider(selected)}>配置</Button><Button size="sm" variant="danger" onClick={deleteConversation} disabled={!activeConversationId || removeConversation.isPending}><Trash2 size={13} /></Button><Button size="sm" variant="danger" onClick={() => deleteProvider(selected)}><Trash2 size={13} /></Button></div>}</header>{!selected ? <div className="empty-panel"><Bot size={30} /><h2>配置模型后开始对话</h2><p>API key 只保存在本机系统密钥链，聊天请求由 Rust 发出。</p><Button variant="primary" onClick={() => editProvider()}>配置供应商</Button></div> : <><div className="ai-messages">{conversations.isLoading && !messages.length && <div className="page-state">读取本地对话…</div>}{!messages.length && !conversations.isLoading && <div className="ai-welcome"><MessageSquare size={24} /><strong>准备好了</strong><span>输入问题，客户端会调用你配置的真实模型。消息会保存在本机，API key 不会落盘。</span></div>}{messages.map((message, index) => <div className={"ai-message ai-message--" + message.role} key={message.role + "-" + index}><span>{message.role === "user" ? "你" : message.role === "assistant" ? selected.name : "系统"}</span><p>{message.content}</p></div>)}{chat.error && <div className="form-error">{errorMessage(chat.error)}</div>}{agent.error && <div className="form-error">{errorMessage(agent.error)}</div>}{saveConversation.error && <div className="form-error">{errorMessage(saveConversation.error)}</div>}{conversations.error && <div className="form-error">{errorMessage(conversations.error)}</div>}{(chat.isPending || agent.isPending) && <div className="ai-message ai-message--assistant"><span>{selected.name}</span><p>{agent.isPending ? "智能体读取服务器状态并思考中…" : streamingText || "模型响应中…"}</p></div>}</div><div className="ai-composer"><div className="ai-composer__mode"><label className="check-field"><input type="checkbox" checked={agentMode} onChange={(event) => setAgentMode(event.target.checked)} /><span>只读服务器智能体</span></label>{agentMode && <select value={agentServerId} onChange={(event) => setAgentServerId(event.target.value)}><option value="">选择服务器</option>{(servers.data ?? []).map((server) => <option value={server.id} key={server.id}>{server.name} · {server.host}</option>)}</select>}{agentMode && mcpServers.data?.some((server) => server.enabled) && <label className="check-field"><input type="checkbox" checked={mcpEnabled} onChange={(event) => setMcpEnabled(event.target.checked)} /><span>启用 MCP 工具</span></label>}</div><textarea value={input} onChange={(event) => setInput(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); send(); } }} placeholder={agentMode ? "让智能体读取选定服务器概览…" : "输入消息，Enter 发送，Shift+Enter 换行"} rows={3} />{chat.isPending ? <Button variant="danger" onClick={() => chatTaskId && void api.cancelCommandTask(chatTaskId)}>取消响应</Button> : <Button variant="primary" onClick={send} disabled={!input.trim() || agent.isPending || (agentMode && !agentServerId)}><Send size={14} />{agentMode ? "运行智能体" : "发送"}</Button>}</div></>}
    </section></div>
    <Dialog.Root open={configOpen} onOpenChange={setConfigOpen}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow"><div className="dialog-header"><div><Dialog.Title>{form.id ? "编辑 AI 供应商" : "添加 AI 供应商"}</Dialog.Title><Dialog.Description>兼容 OpenAI Chat Completions 的模型都可以接入。</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="server-form"><label><span>显示名称</span><input value={form.name} onChange={(event) => setForm((current) => ({ ...current, name: event.target.value }))} placeholder="OpenAI / Ollama / DeepSeek" /></label><label><span>Base URL</span><input value={form.baseUrl} onChange={(event) => setForm((current) => ({ ...current, baseUrl: event.target.value }))} placeholder="https://api.openai.com/v1" /></label><label><span>模型名称</span><input value={form.model} onChange={(event) => setForm((current) => ({ ...current, model: event.target.value }))} placeholder="gpt-4o-mini" /></label><label><span>API key {form.id && "(留空则保留现有 key)"}</span><input type="password" autoComplete="new-password" value={form.apiKey} onChange={(event) => setForm((current) => ({ ...current, apiKey: event.target.value, clearApiKey: false }))} /></label>{form.id && <label className="check-field"><input type="checkbox" checked={form.clearApiKey} onChange={(event) => setForm((current) => ({ ...current, clearApiKey: event.target.checked, apiKey: "" }))} /><span>清除已保存的 key</span></label>}<label className="check-field"><input type="checkbox" checked={form.enabled} onChange={(event) => setForm((current) => ({ ...current, enabled: event.target.checked }))} /><span>启用该供应商</span></label>{save.error && <div className="form-error">{errorMessage(save.error)}</div>}<div className="security-note"><KeyRound size={18} /><span>API key 不会写入 SQLite、LocalStorage、审计日志或诊断包。</span></div><div className="dialog-actions"><Button variant="ghost" onClick={() => setConfigOpen(false)}>取消</Button><Button variant="primary" onClick={() => save.mutate({ id: form.id, name: form.name, baseUrl: form.baseUrl, model: form.model, enabled: form.enabled, apiKey: form.apiKey || undefined, clearApiKey: form.clearApiKey })} disabled={save.isPending || !form.name.trim() || !form.baseUrl.trim() || !form.model.trim()}>{save.isPending ? "保存中…" : "保存供应商"}</Button></div></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={mcpOpen} onOpenChange={setMcpOpen}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="dialog-content">
          <div className="dialog-header">
            <div>
              <Dialog.Title>{mcpForm.id ? "编辑 MCP 工具服务器" : "MCP 工具服务器"}</Dialog.Title>
              <Dialog.Description>支持本地 JSON-RPC stdio 和远程 HTTP/SSE MCP；默认只允许声明 readOnlyHint 的工具。</Dialog.Description>
            </div>
            <Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close>
          </div>
          <div className="server-form">
            <div className="settings-list">
              {(mcpServers.data ?? []).map((server) => <div className="settings-row" key={server.id}>
                <div><strong>{server.name}</strong><small>{server.transport === "http" ? (server.url ?? "远程 HTTP") : server.command} · {server.enabled ? "已启用" : "已停用"}{server.allowWrite ? " · 允许写入" : " · 只读策略"}{server.transport === "http" && server.authConfigured ? " · 已配置令牌" : ""}</small></div>
                <div className="workspace-header__actions"><Button size="sm" variant="ghost" onClick={() => probeMcp.mutate(server.id)} disabled={probeMcp.isPending}>{probeMcp.isPending ? "探测中…" : "探测"}</Button><Button size="sm" variant="ghost" onClick={() => editMcp(server)}>编辑</Button><Button size="sm" variant="danger" onClick={() => window.confirm("删除 MCP 服务器 " + server.name + "？") && removeMcp.mutate(server.id)}>删除</Button></div>
              </div>)}
            </div>
            <label><span>显示名称</span><input value={mcpForm.name} onChange={(event) => setMcpForm((current) => ({ ...current, name: event.target.value }))} placeholder="Filesystem MCP" /></label>
            <label><span>传输方式</span><select value={mcpForm.transport} onChange={(event) => setMcpForm((current) => ({ ...current, transport: event.target.value as "stdio" | "http", command: event.target.value === "http" ? "" : current.command, args: event.target.value === "http" ? "" : current.args }))}><option value="stdio">本地 stdio</option><option value="http">远程 HTTP / SSE</option></select></label>
            {mcpForm.transport === "stdio" ? <><label><span>启动命令</span><input value={mcpForm.command} onChange={(event) => setMcpForm((current) => ({ ...current, command: event.target.value }))} placeholder="npx" /></label><label><span>启动参数（每行一个，不经过 shell）</span><textarea rows={4} value={mcpForm.args} onChange={(event) => setMcpForm((current) => ({ ...current, args: event.target.value }))} placeholder="-y @modelcontextprotocol/server-filesystem" /></label></> : <><label><span>远程 MCP URL</span><input value={mcpForm.url} onChange={(event) => setMcpForm((current) => ({ ...current, url: event.target.value }))} placeholder="https://mcp.example.com/mcp" /></label><label><span>Bearer 令牌 {mcpForm.id && "（留空则保留现有令牌）"}</span><input type="password" autoComplete="new-password" value={mcpForm.authToken} onChange={(event) => setMcpForm((current) => ({ ...current, authToken: event.target.value, clearAuthToken: false }))} /></label>{mcpForm.id && <label className="check-field"><input type="checkbox" checked={mcpForm.clearAuthToken} onChange={(event) => setMcpForm((current) => ({ ...current, clearAuthToken: event.target.checked, authToken: "" }))} /><span>清除已保存的远程令牌</span></label>}</>}
            <div className="field-grid field-grid--2"><label><span>响应超时（秒）</span><input type="number" min={2} max={60} value={mcpForm.timeoutSeconds} onChange={(event) => setMcpForm((current) => ({ ...current, timeoutSeconds: Number(event.target.value) }))} /></label><label className="check-field"><input type="checkbox" checked={mcpForm.enabled} onChange={(event) => setMcpForm((current) => ({ ...current, enabled: event.target.checked }))} /><span>启用并提供给智能体</span></label></div>
            <label className="check-field"><input type="checkbox" checked={mcpForm.allowWrite} onChange={(event) => setMcpForm((current) => ({ ...current, allowWrite: event.target.checked }))} /><span>允许未标记只读的工具（可能产生远端写入）</span></label>
            {mcpProbe && <div className="security-note"><Plug size={18} /><span>已发现 {mcpProbe.tools.length} 个工具：{mcpProbe.tools.map((tool) => tool.name + (tool.readOnly ? "（只读）" : "")).join("、") || "无"}</span></div>}
            {probeMcp.error && <div className="form-error">{errorMessage(probeMcp.error)}</div>}{saveMcp.error && <div className="form-error">{errorMessage(saveMcp.error)}</div>}
            <div className="security-note"><Plug size={18} /><span>stdio 不会经过 shell；远程 Bearer 令牌只保存在本机系统密钥链，不会写入 SQLite、LocalStorage、审计日志或诊断包。</span></div>
            <div className="dialog-actions"><Button variant="ghost" onClick={() => setMcpOpen(false)}>关闭</Button><Button variant="primary" onClick={() => saveMcp.mutate({ id: mcpForm.id, name: mcpForm.name, transport: mcpForm.transport, command: mcpForm.transport === "stdio" ? mcpForm.command : "", args: mcpForm.transport === "stdio" ? mcpForm.args.split(/\r?\n/).map((value) => value.trim()).filter(Boolean) : [], url: mcpForm.transport === "http" ? mcpForm.url : undefined, authToken: mcpForm.authToken || undefined, clearAuthToken: mcpForm.clearAuthToken, enabled: mcpForm.enabled, allowWrite: mcpForm.allowWrite, timeoutSeconds: mcpForm.timeoutSeconds })} disabled={saveMcp.isPending || !mcpForm.name.trim() || (mcpForm.transport === "stdio" ? !mcpForm.command.trim() : !mcpForm.url.trim())}>{saveMcp.isPending ? "保存中…" : "保存 MCP 配置"}</Button></div>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  </section>;
}
