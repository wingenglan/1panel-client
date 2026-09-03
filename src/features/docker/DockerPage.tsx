import * as Dialog from "@radix-ui/react-dialog";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import Editor from "@monaco-editor/react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "../../lib/monaco";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Activity, Box, ChevronDown, Code2, Container, Copy, Database, Download, FileText, ListTree, Pause, Pin, Play, RefreshCw, RotateCw, Search, Settings2, ShieldAlert, Square, SquareTerminal, Trash2, Upload, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import { useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { Pager } from "../../components/ui/Pager";
import { api, type TerminalEvent } from "../../lib/api";
import { errorMessage, isAppError } from "../../lib/errors";
import type { DockerComposeProject, DockerContainerInfo, DockerEventsResult, DockerLogs, DockerSnapshot, DockerTextResult, RemoteTextFile } from "../../types/server";
import { useCommandTaskStore } from "../tasks/taskStore";

type PendingAction = { container: DockerContainerInfo; action: "start" | "stop" | "restart" | "pause" | "unpause" | "kill" | "remove" | "rename"; newName?: string };
type DockerSection = "overview" | "containers" | "compose" | "images" | "networks" | "volumes" | "registry" | "templates" | "config";
type DetailKind = "inspect" | "stats" | "top";
type DockerDetail = DockerTextResult & { title: string };
type RunForm = { image: string; name: string; ports: string; environment: string; network: string; restartPolicy: string; autoRemove: boolean; privileged: boolean };
type BuildForm = { contextPath: string; dockerfilePath: string; image: string; buildArgs: string };
const initialRunForm: RunForm = { image: "nginx:latest", name: "", ports: "", environment: "", network: "", restartPolicy: "unless-stopped", autoRemove: false, privileged: false };
const initialBuildForm: BuildForm = { contextPath: "/opt/1panel-client/build-context", dockerfilePath: "", image: "1panel-client:latest", buildArgs: "" };

/** 按用户查询过滤当前容器日志，仅改变前端展示，不修改远端日志。 */
function filterLogOutput(output: string, query: string) {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return output;
  return output.split("\n").filter((line) => line.toLocaleLowerCase().includes(needle)).join("\n");
}

/** 将 Docker inspect/stats JSON 统一格式化，普通文本结果保持原样。 */
function formatDockerDetail(output: string) {
  try {
    return JSON.stringify(JSON.parse(output), null, 2);
  } catch {
    return output;
  }
}

/** 将 Docker 日志导出为浏览器下载文件，不把日志写入服务器。 */
function downloadText(filename: string, content: string) {
  const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

/** 从 Docker 端口映射文本中提取第一个宿主端口，供本机浏览器打开。 */
function firstPublishedPort(ports: string) {
  const port = ports.match(/(\d+)->/)?.[1];
  return port ? Number(port) : null;
}

/** 按服务器地址打开容器发布端口，IPv6 地址会自动补方括号。 */
function openPublishedPort(host: string, port: number) {
  const normalizedHost = host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
  window.open(`http://${normalizedHost}:${port}`, "_blank", "noopener,noreferrer");
}

/** 从 docker ps 的 State/Status 推断容器生命周期状态，供筛选、批量和 pill 使用。 */
function containerState(container: DockerContainerInfo) {
  if (container.state) return container.state;
  const status = container.status.toLocaleLowerCase();
  if (status.startsWith("up")) return "running";
  if (status.includes("paused")) return "paused";
  return "exited";
}

/** 对齐 Web 1Panel 状态 pill 的中文文案。 */
function stateLabel(state: string) {
  return state === "running" ? "运行中"
    : state === "paused" ? "已暂停"
    : state === "exited" ? "已停止"
    : state === "created" ? "已创建"
    : state === "dead" ? "已死亡"
    : state === "restarting" ? "重启中"
    : state === "removing" ? "移除中"
    : state || "未知";
}

type DockerDiskRow = { Type: string; Total: number; Active: number; Size: string; Reclaimable: string };
type PruneKind = "images" | "containers" | "volumes" | "networks" | "builders";
const DISK_COLUMNS: Array<{ type: string; label: string; pruneKind: PruneKind }> = [
  { type: "Images", label: "镜像", pruneKind: "images" },
  { type: "Containers", label: "容器", pruneKind: "containers" },
  { type: "Local Volumes", label: "本地存储卷", pruneKind: "volumes" },
  { type: "Build Cache", label: "构建缓存", pruneKind: "builders" },
];
const PRUNE_COMMANDS: Record<PruneKind, string> = {
  images: "docker image prune -f",
  containers: "docker container prune -f",
  volumes: "docker volume prune -f",
  networks: "docker network prune -f",
  builders: "docker builder prune -f",
};
const MIRROR_ACCELERATORS = ["https://docker.1panel.live", "https://docker.1panel.dev", "https://docker.1ms.run"];

/** 解析后端数组化的 docker system df JSON；单行或空内容返回空列表。 */
function parseDiskUsage(usage: string | null): DockerDiskRow[] {
  if (!usage) return [];
  try {
    const parsed: unknown = JSON.parse(usage);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((row): row is DockerDiskRow => typeof row === "object" && row !== null && typeof (row as DockerDiskRow).Type === "string");
  } catch {
    return [];
  }
}

/** 去掉 docker system df 括号中的总量，保留可释放的纯大小。 */
function cleanSize(value: string | undefined) {
  return (value ?? "0B").replace(/\s*\(.*\)\s*$/, "").trim();
}

/** 按服务器挂载独立 Docker 工作区，切换节点时清空上一节点的编辑状态。 */
export function DockerPage() {
  const { serverId = "" } = useParams();
  return <DockerWorkspace key={serverId} serverId={serverId} />;
}

/** 展示远程 Docker CLI 数据；仅在确认后执行动作，并同步失效的选择项。 */
function DockerWorkspace({ serverId }: { serverId: string }) {
  const queryClient = useQueryClient();
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<"all" | "running" | "stopped">("all");
  const [composeFilter, setComposeFilter] = useState("all");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [refreshSeconds, setRefreshSeconds] = useState(0);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [batchSelect, setBatchSelect] = useState("");
  const [batchPending, setBatchPending] = useState<{ action: PendingAction["action"]; containers: DockerContainerInfo[] } | null>(null);
  const [section, setSection] = useState<DockerSection>("overview");
  const [pending, setPending] = useState<PendingAction | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [useSudo, setUseSudo] = useState(false);
  const [forceDelete, setForceDelete] = useState(false);
  const [logs, setLogs] = useState<DockerLogs | null>(null);
  const [logQuery, setLogQuery] = useState("");
  const [logTail, setLogTail] = useState(200);
  const [logsCleared, setLogsCleared] = useState(false);
  const [logsPaused, setLogsPaused] = useState(false);
  const [followOutput, setFollowOutput] = useState("");
  const [followTaskId, setFollowTaskId] = useState<string | null>(null);
  const followTaskRef = useRef<string | null>(null);
  const [terminalTarget, setTerminalTarget] = useState<DockerContainerInfo | null>(null);
  const [resourceTarget, setResourceTarget] = useState<{ kind: "volume" | "network" | "image" | "compose"; name: string; workingDir?: string; driver?: string; action: "create" | "remove" | "up" | "down" | "start" | "stop" | "restart" | "pull" | "build" | "cleanup" } | null>(null);
  const [resourceBatch, setResourceBatch] = useState<{ kind: "volume" | "network" | "image"; names: string[] } | null>(null);
  const [resourceDraft, setResourceDraft] = useState({ name: "", driver: "" });
  const [selectedVolumes, setSelectedVolumes] = useState<Set<string>>(new Set());
  const [selectedNetworks, setSelectedNetworks] = useState<Set<string>>(new Set());
  const [selectedImages, setSelectedImages] = useState<Set<string>>(new Set());
  const [detail, setDetail] = useState<DockerDetail | null>(null);
  const [detailQuery, setDetailQuery] = useState("");
  const [statsHistory, setStatsHistory] = useState<Array<{ at: string; output: string }>>([]);
  const [composeProject, setComposeProject] = useState<Pick<DockerComposeProject, "name" | "workingDir"> | null>(null);
  const [composeRawFile, setComposeRawFile] = useState<RemoteTextFile | null>(null);
  const [composeRawDraft, setComposeRawDraft] = useState("");
  const [composeEditing, setComposeEditing] = useState(false);
  const [composeLogs, setComposeLogs] = useState<DockerLogs | null>(null);
  const [composeEnv, setComposeEnv] = useState<RemoteTextFile | null>(null);
  const [composeEnvDraft, setComposeEnvDraft] = useState("");
  const [composeSearch, setComposeSearch] = useState("");
  const [composeTab, setComposeTab] = useState<"compose" | "log" | "env">("compose");
  const [composeCreateOpen, setComposeCreateOpen] = useState(false);
  const [composeCreateForm, setComposeCreateForm] = useState({ name: "", content: "", workingDir: "", forcePull: false });
  const [composePinned, setComposePinned] = useState<string[]>(() => {
    try {
      const saved: unknown = JSON.parse(window.localStorage.getItem(`wg:compose-pinned:${serverId}`) ?? "[]");
      return Array.isArray(saved) ? saved.filter((name): name is string => typeof name === "string") : [];
    } catch { return []; }
  });
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [eventsOpen, setEventsOpen] = useState(false);
  const [eventsSince, setEventsSince] = useState(300);
  const [runOpen, setRunOpen] = useState(false);
  const [runForm, setRunForm] = useState<RunForm>(initialRunForm);
  const [buildOpen, setBuildOpen] = useState(false);
  const [buildForm, setBuildForm] = useState<BuildForm>(initialBuildForm);
  const [buildOutput, setBuildOutput] = useState("");
  const [buildTaskId, setBuildTaskId] = useState<string | null>(null);
  const [pruneTarget, setPruneTarget] = useState<{ kind: PruneKind; label: string } | null>(null);
  const [pruneOutput, setPruneOutput] = useState("");
  const [pullImage, setPullImage] = useState("");
  const [pullOutput, setPullOutput] = useState("");
  const [pullTaskId, setPullTaskId] = useState<string | null>(null);
  const addTask = useCommandTaskStore((state) => state.add);
  const markRunning = useCommandTaskStore((state) => state.running);
  const markSuccess = useCommandTaskStore((state) => state.success);
  const markFail = useCommandTaskStore((state) => state.fail);
  const markCancelled = useCommandTaskStore((state) => state.cancelled);
  const resourceTaskRef = useRef<string | null>(null);
  const docker = useQuery({ queryKey: ["docker", serverId, useSudo], queryFn: () => api.docker(serverId, useSudo), enabled: !!serverId });
  const eventsMutation = useMutation<DockerEventsResult, unknown>({ mutationFn: () => api.dockerEvents(serverId, eventsSince, useSudo) });
  const profile = useQuery({ queryKey: ["server", serverId], queryFn: () => api.getServer(serverId), enabled: !!serverId });
  const composeDetails = useQuery({ queryKey: ["docker-compose", serverId, composeProject?.name, composeProject?.workingDir, useSudo], queryFn: () => api.dockerComposeDetails(serverId, composeProject!.name, composeProject!.workingDir || undefined, useSudo), enabled: !!composeProject });
  const action = useMutation({ mutationFn: () => api.dockerContainerAction({ serverId, containerId: pending!.container.id, action: pending!.action, newName: pending!.action === "rename" ? renameDraft : pending!.newName, force: pending!.action === "remove" && forceDelete, sudo: useSudo, confirmed: true }), onSuccess: async () => { setSelectedIds((current) => { const next = new Set(current); next.delete(pending!.container.id); return next; }); setPending(null); setUseSudo(false); setForceDelete(false); await queryClient.invalidateQueries({ queryKey: ["docker", serverId] }); } });
  const batchMutation = useMutation({ mutationFn: async () => { for (const container of batchPending!.containers) { await api.dockerContainerAction({ serverId, containerId: container.id, action: batchPending!.action, sudo: useSudo, confirmed: true }); } }, onSuccess: async () => { setBatchPending(null); setBatchSelect(""); setSelectedIds(new Set()); await queryClient.invalidateQueries({ queryKey: ["docker", serverId] }); } });
  const logsMutation = useMutation({ mutationFn: (containerId: string) => api.dockerContainerLogs(serverId, containerId, logTail, useSudo), onSuccess: (value) => { setLogs(value); setLogQuery(""); setLogsCleared(false); setLogsPaused(false); } });
  const followMutation = useMutation({ mutationFn: (containerId: string) => { const taskId = crypto.randomUUID(); followTaskRef.current = taskId; setFollowTaskId(taskId); addTask({ id: taskId, type: "docker-follow", serverId, title: `跟随日志 ${containerId}`, status: "queued" }); return api.dockerContainerFollowLogs(serverId, containerId, 200, useSudo, taskId, (event) => { if (event.event === "output") { markRunning(taskId); setFollowOutput((current) => current + event.data.data); } if (event.event === "cancelled") markCancelled(taskId); }); }, onSuccess: (value) => { if (followTaskRef.current) markSuccess(followTaskRef.current); setLogs(value); }, onError: (reason) => { if (followTaskRef.current) { if (isAppError(reason) && reason.code === "CANCELLED") markCancelled(followTaskRef.current); else markFail(followTaskRef.current, errorMessage(reason)); } }, onSettled: () => { followTaskRef.current = null; setFollowTaskId(null); } });
  const resourceMutation = useMutation({ mutationFn: () => resourceTarget?.kind === "compose" ? api.dockerComposeAction({ serverId, project: resourceTarget.name, workingDir: resourceTarget.workingDir, action: resourceTarget.action as "up" | "down" | "start" | "stop" | "restart" | "pull" | "build" | "cleanup", sudo: useSudo, confirmed: true }) : resourceTarget?.kind === "image" ? api.dockerImageAction({ serverId, image: resourceTarget.name, action: "remove", force: false, sudo: useSudo, confirmed: true }) : api.dockerResourceAction({ serverId, kind: resourceTarget!.kind, name: resourceTarget!.action === "create" ? resourceDraft.name.trim() : resourceTarget!.name, action: resourceTarget!.action as "create" | "remove", driver: resourceTarget!.action === "create" && resourceDraft.driver.trim() ? resourceDraft.driver.trim() : undefined, sudo: useSudo, confirmed: true }), onMutate: () => { const taskId = crypto.randomUUID(); resourceTaskRef.current = taskId; addTask({ id: taskId, type: "docker-compose", serverId, title: `${resourceTarget?.kind ?? "Docker"} ${resourceTarget?.action ?? "操作"} · ${resourceTarget?.name ?? ""}`, status: "queued", cancelSupported: false }); markRunning(taskId); }, onSuccess: async () => { if (resourceTaskRef.current) markSuccess(resourceTaskRef.current); setResourceTarget(null); await queryClient.invalidateQueries({ queryKey: ["docker", serverId] }); await composeDetails.refetch(); }, onError: (reason) => { if (resourceTaskRef.current) markFail(resourceTaskRef.current, errorMessage(reason)); }, onSettled: () => { resourceTaskRef.current = null; } });
  const batchResourceMutation = useMutation({ mutationFn: async () => { for (const name of resourceBatch!.names) { if (resourceBatch!.kind === "image") { await api.dockerImageAction({ serverId, image: name, action: "remove", force: false, sudo: useSudo, confirmed: true }); } else { await api.dockerResourceAction({ serverId, kind: resourceBatch!.kind, name, action: "remove", sudo: useSudo, confirmed: true }); } } }, onSuccess: async () => { setResourceBatch(null); setSelectedVolumes(new Set()); setSelectedNetworks(new Set()); setSelectedImages(new Set()); await queryClient.invalidateQueries({ queryKey: ["docker", serverId] }); } });
  const resourceInspectMutation = useMutation({ mutationFn: (input: { kind: "volume" | "network"; name: string }) => api.dockerResourceInspect({ serverId, ...input, sudo: useSudo }), onSuccess: (value, input) => setDetail({ ...value, title: `资源检查 · ${input.name}` }) });
  const composeLogsMutation = useMutation({ mutationFn: (service: string | undefined) => api.dockerComposeLogs(serverId, composeProject!.name, composeProject!.workingDir || undefined, service, 200, useSudo), onSuccess: setComposeLogs });
  const composeReadMutation = useMutation({ mutationFn: () => api.readText(serverId, composeDetails.data!.configPath!), onSuccess: (file) => { setComposeRawFile(file); setComposeRawDraft(file.content); setComposeEditing(true); } });
  const composeSaveMutation = useMutation({ mutationFn: () => api.dockerComposeSaveYaml({ serverId, project: composeProject!.name, workingDir: composeProject!.workingDir || undefined, configPath: composeRawFile!.path, content: composeRawDraft, expectedSize: composeRawFile!.size, expectedModifiedAt: composeRawFile!.modifiedAt, sudo: useSudo, confirmed: true }), onSuccess: async (file) => { setComposeRawFile(file); setComposeRawDraft(file.content); setComposeEditing(false); await composeDetails.refetch(); } });
  const composeEnvPath = useMemo(() => { const path = composeDetails.data?.configPath; return path ? `${path.replace(/\/[^/]+$/, "")}/.env` : null; }, [composeDetails.data?.configPath]);
  const composeEnvReadMutation = useMutation({ mutationFn: () => { if (!composeEnvPath) return Promise.reject(new Error("未知配置文件路径")); return api.readText(serverId, composeEnvPath); }, onSuccess: (file) => { setComposeEnv(file); setComposeEnvDraft(file.content); } });
  const composeEnvSaveMutation = useMutation({ mutationFn: () => { if (!composeEnv) return Promise.reject(new Error("尚未读取 .env 文件")); const input = { serverId, path: composeEnv.path, content: composeEnvDraft, expectedSize: composeEnv.size, expectedModifiedAt: composeEnv.modifiedAt, force: false }; return useSudo ? api.saveTextPrivileged(input) : api.saveText(input); }, onSuccess: (file) => { setComposeEnv(file); setComposeEnvDraft(file.content); } });
  const composeCreateMutation = useMutation({ mutationFn: () => api.dockerComposeCreate({ serverId, name: composeCreateForm.name.trim(), content: composeCreateForm.content, workingDir: composeCreateForm.workingDir.trim() || undefined, forcePull: composeCreateForm.forcePull, sudo: useSudo, confirmed: true }), onSuccess: async () => { setComposeCreateOpen(false); await queryClient.invalidateQueries({ queryKey: ["docker", serverId] }); } });
  const togglePinned = (name: string) => setComposePinned((current) => { const next = current.includes(name) ? current.filter((item) => item !== name) : [...current, name]; try { window.localStorage.setItem(`wg:compose-pinned:${serverId}`, JSON.stringify(next)); } catch { /* 本地存储不可用时仅影响本次会话 */ } return next; });
  // 查询确认项目已不存在时，在渲染阶段关闭其编辑状态。
  if (composeProject && docker.data && !docker.data.composeProjects.some((project) => project.name === composeProject.name)) {
    setComposeProject(null);
    setComposeRawFile(null);
    setComposeEditing(false);
  }
  const onImportComposeFile = (event: ChangeEvent<HTMLInputElement>) => { const file = event.target.files?.[0]; event.target.value = ""; if (!file) return; const reader = new FileReader(); reader.onload = () => { setComposeCreateForm({ name: file.name.replace(/\.(ya?ml)$/i, ""), content: String(reader.result ?? ""), workingDir: "", forcePull: false }); }; reader.onerror = () => undefined; reader.readAsText(file); };
  const detailMutation = useMutation({ mutationFn: ({ kind, containerId }: { kind: DetailKind; containerId: string }) => kind === "inspect" ? api.dockerContainerInspect(serverId, containerId, useSudo) : kind === "stats" ? api.dockerContainerStats(serverId, containerId, useSudo) : api.dockerContainerTop(serverId, containerId, useSudo), onSuccess: (value, variables) => { const output = formatDockerDetail(value.output); setDetail({ ...value, output, title: `${variables.kind === "inspect" ? "检查详情" : variables.kind === "stats" ? "资源统计" : "进程列表"} · ${variables.containerId}` }); setDetailQuery(""); if (variables.kind === "stats") setStatsHistory((current) => [...current, { at: new Date().toLocaleTimeString(), output }].slice(-12)); } });
  const pullMutation = useMutation({ mutationFn: (taskId: string) => { addTask({ id: taskId, type: "docker-pull", serverId, title: `拉取镜像 ${pullImage.trim()}`, status: "queued" }); return api.dockerPullImage({ serverId, image: pullImage.trim(), taskId, sudo: useSudo }, (event) => { if (event.event === "output") { markRunning(taskId); setPullOutput((current) => current + event.data.data); } if (event.event === "cancelled") markCancelled(taskId); }); }, onSuccess: async (_value, taskId) => { markSuccess(taskId); setPullImage(""); await queryClient.invalidateQueries({ queryKey: ["docker", serverId] }); }, onError: (reason, taskId) => { if (isAppError(reason) && reason.code === "CANCELLED") markCancelled(taskId); else markFail(taskId, errorMessage(reason)); }, onSettled: () => setPullTaskId(null) });
  const buildMutation = useMutation({ mutationFn: (taskId: string) => { const buildArgs = buildForm.buildArgs.split(/\n|,/).map((value) => value.trim()).filter(Boolean); addTask({ id: taskId, type: "docker-build", serverId, title: `构建镜像 ${buildForm.image.trim()}`, status: "queued" }); return api.dockerBuildImage({ serverId, contextPath: buildForm.contextPath.trim(), dockerfilePath: buildForm.dockerfilePath.trim() || undefined, image: buildForm.image.trim(), buildArgs, taskId, sudo: useSudo }, (event) => { if (event.event === "output") { markRunning(taskId); setBuildOutput((current) => current + event.data.data); } if (event.event === "cancelled") markCancelled(taskId); }); }, onSuccess: async (_value, taskId) => { markSuccess(taskId); setBuildOpen(false); setBuildForm(initialBuildForm); await queryClient.invalidateQueries({ queryKey: ["docker", serverId] }); }, onError: (reason, taskId) => { if (isAppError(reason) && reason.code === "CANCELLED") markCancelled(taskId); else markFail(taskId, errorMessage(reason)); }, onSettled: () => setBuildTaskId(null) });
  const runMutation = useMutation({ mutationFn: () => api.dockerRunContainer({ serverId, image: runForm.image.trim(), name: runForm.name.trim() || undefined, ports: runForm.ports.split(/[,\n]/).map((value) => value.trim()).filter(Boolean), environment: runForm.environment.split(/\n/).map((value) => value.trim()).filter(Boolean), network: runForm.network.trim() || undefined, restartPolicy: runForm.restartPolicy || undefined, autoRemove: runForm.autoRemove, privileged: runForm.privileged, sudo: useSudo }), onSuccess: async () => { setRunOpen(false); setRunForm(initialRunForm); await queryClient.invalidateQueries({ queryKey: ["docker", serverId] }); } });
  const pruneMutation = useMutation({ mutationFn: () => api.dockerPrune({ serverId, kind: pruneTarget!.kind, sudo: useSudo, confirmed: true }), onSuccess: async (value) => { setPruneOutput(value.output); await queryClient.invalidateQueries({ queryKey: ["docker", serverId] }); } });
  const filteredLogOutput = useMemo(() => filterLogOutput(logs?.output ?? "", logQuery), [logQuery, logs?.output]);
  const visibleLogOutput = logsCleared ? "" : filteredLogOutput;
  const filteredDetailOutput = useMemo(() => filterLogOutput(detail?.output ?? "", detailQuery), [detail?.output, detailQuery]);
  const containers = useMemo(() => docker.data?.containers.filter((container) => { const haystack = [container.name, container.id, container.image, container.status, container.ports, container.composeProject, container.ipAddresses].some((value) => (value ?? "").toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())); const state = containerState(container); const statusMatches = statusFilter === "all" || (statusFilter === "running" && state === "running") || (statusFilter === "stopped" && state !== "running" && state !== "paused"); const composeMatches = composeFilter === "all" || container.composeProject === composeFilter; return haystack && statusMatches && composeMatches; }) ?? [], [composeFilter, docker.data, query, statusFilter]);
  const composeProjects = useMemo(() => Array.from(new Set((docker.data?.containers ?? []).map((container) => container.composeProject).filter((value): value is string => !!value))).sort(), [docker.data]);
  const usedImages = useMemo(() => new Set((docker.data?.containers ?? []).map((container) => container.image)), [docker.data]);
  const imageGroups = useMemo(() => { const groups = new Map<string, { id: string; size: string; created: string; tags: string[] }>(); for (const image of docker.data?.images ?? []) { const key = image.id || `${image.repository}:${image.tag}`; const tags = [`${image.repository}:${image.tag}`]; const existing = groups.get(key); if (existing) existing.tags.push(...tags); else groups.set(key, { id: image.id, size: image.size, created: image.created, tags }); } return Array.from(groups.values()); }, [docker.data]);
  const composeRows = useMemo(() => { const containers = docker.data?.containers ?? []; return (docker.data?.composeProjects ?? []).map((project) => { const own = containers.filter((container) => container.composeProject === project.name); const runningCount = own.filter((container) => containerState(container) === "running").length; const createdAt = own.length ? own.reduce((oldest, container) => (container.created < oldest ? container.created : oldest), own[0].created) : ""; return { ...project, containerCount: own.length, runningCount, createdAt: createdAt.slice(0, 19).replace("T", " ") }; }).sort((a, b) => { const aPinned = composePinned.includes(a.name) ? 0 : 1; const bPinned = composePinned.includes(b.name) ? 0 : 1; return aPinned - bPinned || a.name.localeCompare(b.name); }); }, [composePinned, docker.data]);
  const runningCount = useMemo(() => (docker.data?.containers ?? []).filter((container) => containerState(container) === "running").length, [docker.data]);
  const stoppedCount = useMemo(() => (docker.data?.containers ?? []).filter((container) => { const state = containerState(container); return state !== "running" && state !== "paused"; }).length, [docker.data]);
  const pagedContainers = useMemo(() => containers.slice((page - 1) * pageSize, page * pageSize), [containers, page, pageSize]);
  const selectedContainers = useMemo(() => containers.filter((container) => selectedIds.has(container.id)), [containers, selectedIds]);
  const selectedStates = useMemo(() => selectedContainers.map(containerState), [selectedContainers]);
  const batchDisabled = (action: PendingAction["action"]) => {
    if (!selectedContainers.length) return true;
    if (action === "start") return selectedStates.includes("running");
    if (action === "stop") return selectedStates.some((state) => state === "exited" || state === "created" || state === "dead");
    if (action === "pause") return selectedStates.some((state) => state === "paused" || state === "exited");
    if (action === "unpause") return selectedStates.some((state) => state !== "paused");
    return false;
  };
  const openBatch = (action: PendingAction["action"]) => {
    if (!selectedContainers.length || batchDisabled(action)) return;
    setBatchPending({ action, containers: selectedContainers });
  };

  useEffect(() => {
    if (!detail?.title.startsWith("stats")) return;
    const timer = window.setInterval(() => detailMutation.mutate({ kind: "stats", containerId: detail.containerId }), 5000);
    return () => window.clearInterval(timer);
  }, [detail?.containerId, detail?.title, detailMutation, useSudo]);

  useEffect(() => {
    if (!refreshSeconds) return;
    const timer = window.setInterval(() => { void docker.refetch(); }, refreshSeconds * 1000);
    return () => window.clearInterval(timer);
  }, [refreshSeconds, docker]);

  const knownIds = new Set((docker.data?.containers ?? []).map((container) => container.id));
  if (docker.data && [...selectedIds].some((id) => !knownIds.has(id))) {
    setSelectedIds(new Set([...selectedIds].filter((id) => knownIds.has(id))));
  }

  return <section className="docker-page">
    <div className="page-tabbar"><nav className="docker-tabs"><button className={section === "overview" ? "is-active" : ""} onClick={() => setSection("overview")}>概览</button><button className={section === "containers" ? "is-active" : ""} onClick={() => setSection("containers")}>容器</button><button className={section === "compose" ? "is-active" : ""} onClick={() => setSection("compose")}>编排</button><button className={section === "images" ? "is-active" : ""} onClick={() => setSection("images")}>镜像</button><button className={section === "networks" ? "is-active" : ""} onClick={() => setSection("networks")}>网络</button><button className={section === "volumes" ? "is-active" : ""} onClick={() => setSection("volumes")}>存储卷</button><button className={section === "registry" ? "is-active" : ""} onClick={() => setSection("registry")}>仓库</button><button className={section === "templates" ? "is-active" : ""} onClick={() => setSection("templates")}>编排模版</button><button className={section === "config" ? "is-active" : ""} onClick={() => setSection("config")}>配置</button></nav><div className="page-tabbar__actions"><Button variant="secondary" onClick={() => { setEventsOpen(true); eventsMutation.mutate(); }} disabled={eventsMutation.isPending}><Activity size={14} />事件</Button><Button variant="secondary" onClick={() => docker.refetch()} disabled={docker.isFetching}><RefreshCw className={docker.isFetching ? "spin" : ""} size={14} /> 刷新</Button></div></div>
    {followTaskId && <div className="page-state"><span>正在跟随容器日志，最多 30 秒。</span><Button size="sm" variant="danger" onClick={() => void api.cancelCommandTask(followTaskId)}>取消远端跟随</Button></div>}
    {docker.isLoading && <div className="page-state">正在读取 Docker 引擎、容器和镜像…</div>}
    {docker.error && <div className="page-state page-state--error">{errorMessage(docker.error)}</div>}
    {logsMutation.error && <div className="page-state page-state--error">{errorMessage(logsMutation.error)}</div>}
    {detailMutation.error && <div className="page-state page-state--error">{errorMessage(detailMutation.error)}</div>}
    {resourceInspectMutation.error && <div className="page-state page-state--error">{errorMessage(resourceInspectMutation.error)}</div>}
    {pullMutation.error && <div className="page-state page-state--error">{errorMessage(pullMutation.error)}</div>}
    {buildMutation.isError && <div className="page-state page-state--error">{errorMessage(buildMutation.error)}</div>}
    {eventsMutation.isError && <div className="page-state page-state--error">{errorMessage(eventsMutation.error)}</div>}
    {docker.data && (!docker.data.installed ? <div className="empty-panel"><Container size={27} /><h2>尚未安装 Docker</h2><p>请在工具中心查看 Docker 安装计划；本应用不会开启 Docker TCP API。</p></div> : <>
      {section === "overview" && <DockerOverview data={docker.data} onOpen={setSection} onPrune={(kind, label) => { setPruneTarget({ kind, label }); setPruneOutput(""); }} />}
      {section === "containers" && <>
      <div className="docker-toolbar docker-toolbar--containers">
        <Button variant="primary" size="sm" onClick={() => setRunOpen(true)}><Play size={13} /> 创建</Button>
        <Button variant="ghost" size="sm" onClick={() => { setPruneTarget({ kind: "containers", label: "容器" }); setPruneOutput(""); }}><Trash2 size={13} /> 清理容器</Button>
        <span className="docker-toolbar-spacer" />
        <label className="docker-refresh"><span>刷新频率</span><select value={refreshSeconds} onChange={(event) => setRefreshSeconds(Number(event.target.value))}><option value={0}>关闭</option><option value={5}>5 秒</option><option value={10}>10 秒</option><option value={30}>30 秒</option><option value={60}>60 秒</option><option value={120}>120 秒</option><option value={300}>300 秒</option></select></label>
        <label className="docker-search"><Search size={14} /><input value={query} onChange={(event) => { setQuery(event.target.value); setPage(1); }} placeholder="搜索容器名称、镜像、IP" /></label>
        <label className="hidden-toggle">Compose<select value={composeFilter} onChange={(event) => { setComposeFilter(event.target.value); setPage(1); }}><option value="all">全部项目</option>{composeProjects.map((project) => <option key={project} value={project}>{project}</option>)}</select></label>
        {profile.data && profile.data.sudoMode !== "none" && <label className="force-toggle"><input type="checkbox" checked={useSudo} onChange={(event) => setUseSudo(event.target.checked)} /> sudo</label>}
        <span className="docker-toolbar-count">{containers.length} / {docker.data.containers.length}</span>
      </div>
      <section className="docker-section docker-section--containers">
        <div className="docker-status-tabs">
          <button className={statusFilter === "all" ? "is-active" : ""} onClick={() => { setStatusFilter("all"); setPage(1); }}>所有 <b>{docker.data.containers.length}</b></button>
          <button className={statusFilter === "running" ? "is-active" : ""} onClick={() => { setStatusFilter("running"); setPage(1); }}>已启动 <b>{runningCount}</b></button>
          <button className={statusFilter === "stopped" ? "is-active" : ""} onClick={() => { setStatusFilter("stopped"); setPage(1); }}>已停止 <b>{stoppedCount}</b></button>
        </div>
        {pagedContainers.length ? <div className="docker-table docker-container-table">
          <div className="ops-head docker-container-head">
            <span className="docker-head-check"><input type="checkbox" checked={pagedContainers.length > 0 && pagedContainers.every((container) => selectedIds.has(container.id))} onChange={(event) => setSelectedIds((current) => { const next = new Set(current); for (const container of pagedContainers) { if (event.target.checked) next.add(container.id); else next.delete(container.id); } return next; })} /></span>
            <span>名称</span><span>镜像</span><span>状态</span><span>资源使用率</span><span>IP</span><span>端口</span><span>运行时长</span><span>操作</span>
          </div>
          {pagedContainers.map((container) => <ContainerTableRow key={container.id} container={container} selected={selectedIds.has(container.id)} onToggle={(id) => setSelectedIds((current) => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next; })} onAction={(actionName) => { setPending({ container, action: actionName }); setRenameDraft(container.name); setForceDelete(false); }} onOpenPort={(port) => profile.data && openPublishedPort(profile.data.host, port)} onLogs={() => logsMutation.mutate(container.id)} onFollow={() => { setFollowOutput(""); followMutation.mutate(container.id); }} onExec={() => setTerminalTarget(container)} onDetail={(kind) => detailMutation.mutate({ kind, containerId: container.id })} />)}
        </div> : <div className="empty-panel empty-panel--small">没有匹配的容器。</div>}
        <div className="docker-table-footer">
          <Button size="sm" variant="ghost" disabled={!selectedContainers.length}>批量操作</Button>
          <select className="docker-batch-select" value={batchSelect} onChange={(event) => { const value = event.target.value as PendingAction["action"]; if (!value) return; setBatchSelect(""); openBatch(value); }} disabled={!selectedContainers.length}>
            <option value="">请选择</option><option value="start">启动</option><option value="stop">停止</option><option value="restart">重启</option><option value="kill">终止</option><option value="pause">暂停</option><option value="unpause">恢复</option><option value="remove">删除</option>
          </select>
          <Pager total={containers.length} page={page} pageSize={pageSize} pageSizes={[10, 20, 50, 100]} onPageChange={setPage} onPageSizeChange={(size) => { setPageSize(size); setPage(1); }} />
        </div>
      </section>
      </>}
      {section === "images" && <section className="docker-section"><header><Box size={15} /><strong>镜像</strong><span>{imageGroups.length}</span><div className="docker-image-tools"><input value={pullImage} onChange={(event) => setPullImage(event.target.value)} placeholder="nginx:latest" />{pullMutation.isPending ? <Button size="sm" variant="danger" onClick={() => pullTaskId && void api.cancelCommandTask(pullTaskId)}>取消拉取</Button> : <Button size="sm" variant="secondary" onClick={() => { const taskId = crypto.randomUUID(); setPullTaskId(taskId); setPullOutput(""); pullMutation.mutate(taskId); }} disabled={!pullImage.trim()}>拉取</Button>}<Button size="sm" variant="ghost" onClick={() => { setBuildOutput(""); setBuildOpen(true); }}>构建</Button><Button size="sm" variant="ghost" onClick={() => setRunOpen(true)}>运行</Button><Button size="sm" variant="danger" onClick={() => { setPruneOutput(""); setPruneTarget({ kind: "images", label: "镜像" }); }} disabled={!docker.data.images.length}>释放</Button><Button size="sm" variant="danger" disabled={!selectedImages.size} onClick={() => setResourceBatch({ kind: "image", names: Array.from(selectedImages) })}>批量删除</Button></div></header><div className="docker-table"><div className="ops-head docker-images-head"><span>ID</span><span>状态</span><span>标签</span><span>大小</span><span>创建时间</span><span>操作</span></div>{imageGroups.map((image) => <div className="ops-row docker-images-row" key={image.id || image.tags.join(",")}><span className="mono">{image.id.replace(/^sha256:/, "").slice(0, 12)}</span><span><span className={`status-pill ${image.tags.some((tag) => usedImages.has(tag) || usedImages.has(tag.split(":")[0])) ? "is-up" : "is-down"}`}>{image.tags.some((tag) => usedImages.has(tag) || usedImages.has(tag.split(":")[0])) ? "使用中" : "未使用"}</span></span><span className="docker-image-tags">{image.tags.map((tag) => <span className="tag" key={tag}>{tag}</span>)}</span><span>{image.size}</span><span>{image.created}</span><span className="docker-actions"><input type="checkbox" checked={selectedImages.has(image.id)} onChange={() => setSelectedImages((current) => { const next = new Set(current); if (next.has(image.id)) next.delete(image.id); else next.add(image.id); return next; })} title="选择后批量删除" /><Button size="sm" variant="danger" onClick={() => setResourceTarget({ kind: "image", name: image.tags.find((tag) => !tag.startsWith("<none>")) ?? image.id, action: "remove" })}>删除</Button></span></div>)}</div>{pullOutput && <pre className="install-output docker-pull-output">{pullOutput}</pre>}</section>}
      {section === "volumes" && <section className="docker-section"><header><Activity size={15} /><strong>存储卷</strong><span>{docker.data.volumes.length}</span><div className="docker-image-tools"><Button size="sm" variant="primary" onClick={() => { setResourceDraft({ name: "", driver: "" }); setResourceTarget({ kind: "volume", name: "", action: "create" }); }}>创建</Button><Button size="sm" variant="ghost" onClick={() => { setPruneOutput(""); setPruneTarget({ kind: "volumes", label: "存储卷" }); }}>清理</Button><Button size="sm" variant="danger" disabled={!selectedVolumes.size} onClick={() => setResourceBatch({ kind: "volume", names: Array.from(selectedVolumes) })}>批量删除</Button></div></header><div className="docker-table"><div className="ops-head docker-volumes-head"><span className="docker-res-check"><input type="checkbox" checked={docker.data.volumes.length > 0 && docker.data.volumes.every((volume) => selectedVolumes.has(volume.name))} onChange={(event) => setSelectedVolumes(event.target.checked ? new Set(docker.data.volumes.map((volume) => volume.name)) : new Set())} /></span><span>名称</span><span>驱动</span><span>挂载点</span><span>操作</span></div>{docker.data.volumes.map((volume) => <div className="ops-row docker-volumes-row" key={volume.name}><span className="docker-res-check"><input type="checkbox" checked={selectedVolumes.has(volume.name)} onChange={() => setSelectedVolumes((current) => { const next = new Set(current); if (next.has(volume.name)) next.delete(volume.name); else next.add(volume.name); return next; })} /></span><span><button type="button" className="text-link" onClick={() => resourceInspectMutation.mutate({ kind: "volume", name: volume.name })}><strong>{volume.name}</strong></button></span><span className="mono">{volume.driver}</span><span className="mono docker-res-mount">{volume.mountpoint}</span><span className="docker-actions"><Button size="sm" variant="ghost" onClick={() => resourceInspectMutation.mutate({ kind: "volume", name: volume.name })}>检查</Button><Button size="sm" variant="danger" onClick={() => setResourceTarget({ kind: "volume", name: volume.name, action: "remove" })}>删除</Button></span></div>)}</div></section>}
      {section === "networks" && <section className="docker-section"><header><Activity size={15} /><strong>网络</strong><span>{docker.data.networks.length}</span><div className="docker-image-tools"><Button size="sm" variant="primary" onClick={() => { setResourceDraft({ name: "", driver: "" }); setResourceTarget({ kind: "network", name: "", action: "create" }); }}>创建</Button><Button size="sm" variant="ghost" onClick={() => { setPruneOutput(""); setPruneTarget({ kind: "networks", label: "网络" }); }}>清理</Button><Button size="sm" variant="danger" disabled={!selectedNetworks.size} onClick={() => setResourceBatch({ kind: "network", names: Array.from(selectedNetworks) })}>批量删除</Button></div></header><div className="docker-table"><div className="ops-head docker-networks-head"><span className="docker-res-check"><input type="checkbox" checked={docker.data.networks.length > 0 && docker.data.networks.every((network) => selectedNetworks.has(network.name))} onChange={(event) => setSelectedNetworks(event.target.checked ? new Set(docker.data.networks.map((network) => network.name)) : new Set())} /></span><span>名称</span><span>驱动</span><span>作用域</span><span>操作</span></div>{docker.data.networks.map((network) => <div className="ops-row docker-networks-row" key={network.id}><span className="docker-res-check"><input type="checkbox" checked={selectedNetworks.has(network.name)} onChange={() => setSelectedNetworks((current) => { const next = new Set(current); if (next.has(network.name)) next.delete(network.name); else next.add(network.name); return next; })} /></span><span><button type="button" className="text-link" onClick={() => resourceInspectMutation.mutate({ kind: "network", name: network.name })}><strong>{network.name}</strong></button></span><span className="mono">{network.driver}</span><span className="mono">{network.scope}</span><span className="docker-actions"><Button size="sm" variant="ghost" onClick={() => resourceInspectMutation.mutate({ kind: "network", name: network.name })}>检查</Button><Button size="sm" variant="danger" onClick={() => setResourceTarget({ kind: "network", name: network.name, action: "remove" })}>删除</Button></span></div>)}</div></section>}
      {section === "compose" && <section className="docker-section compose-section">
        <header className="compose-toolbar">
          <div className="compose-toolbar__group">
            <Button size="sm" variant="primary" onClick={() => { setComposeCreateForm({ name: "", content: "", workingDir: "", forcePull: false }); setComposeCreateOpen(true); }}>创建</Button>
            <Button size="sm" variant="ghost" onClick={() => fileInputRef.current?.click()}><Upload size={14} /> 导入</Button>
            <input ref={fileInputRef} type="file" accept=".yaml,.yml" className="compose-file-input" onChange={onImportComposeFile} />
            <Button size="sm" variant="ghost" onClick={() => docker.refetch()} disabled={docker.isFetching}><RefreshCw size={13} className={docker.isFetching ? "spin" : ""} /> 刷新</Button>
          </div>
          <div className="compose-toolbar__group">
            <label className="compose-search"><Search size={14} /><input value={composeSearch} onChange={(event) => setComposeSearch(event.target.value)} placeholder="搜索项目" /></label>
          </div>
        </header>
        <div className="compose-layout">
          <div className="compose-list">
            {composeRows.filter((row) => row.name.toLocaleLowerCase().includes(composeSearch.trim().toLocaleLowerCase())).map((row) => {
              const allRunning = row.containerCount === row.runningCount && row.runningCount > 0;
              const stateLabel = row.containerCount === 0 ? "已退出" : "运行";
              const stateClass = row.containerCount === 0 ? "is-down" : allRunning ? "is-up" : "is-warn";
              return (
                <div key={row.name} className={`compose-item ${composeProject?.name === row.name ? "is-active" : ""}`} onClick={() => { setComposeProject({ name: row.name, workingDir: row.workingDir }); setComposeRawFile(null); setComposeEditing(false); setComposeEnv(null); setComposeEnvDraft(""); setComposeTab("compose"); }}>
                  <div className="compose-item__title"><span className="compose-item__name">{row.name}</span><Button size="sm" variant="ghost" className={`compose-pin ${composePinned.includes(row.name) ? "is-pinned" : ""}`} title={composePinned.includes(row.name) ? "取消置顶" : "置顶"} onClick={(event) => { event.stopPropagation(); togglePinned(row.name); }}><Pin size={13} /></Button></div>
                  <div className="compose-item__meta"><span className="tag">本地</span><span>{row.createdAt || "—"}</span><span className={`compose-item__state ${stateClass}`}>{stateLabel}{row.containerCount > 0 ? ` ${row.runningCount}/${row.containerCount}` : ""}</span></div>
                  <div className="compose-item__actions" onClick={(event) => event.stopPropagation()}>
                    <DropdownMenu.Root>
                      <DropdownMenu.Trigger asChild><button type="button" className="compose-status-trigger" title="启动 / 停止 / 重启 / 重建"><span className={`status-pill ${stateClass}`}>{stateLabel}</span></button></DropdownMenu.Trigger>
                      <DropdownMenu.Portal>
                        <DropdownMenu.Content className="context-menu" sideOffset={4} align="end">
                          <DropdownMenu.Item className="context-menu-item" disabled={allRunning} onSelect={() => setResourceTarget({ kind: "compose", name: row.name, workingDir: row.workingDir || undefined, action: "up" })}><Play size={14} /> 启动</DropdownMenu.Item>
                          <DropdownMenu.Item className="context-menu-item" disabled={row.runningCount === 0} onSelect={() => setResourceTarget({ kind: "compose", name: row.name, workingDir: row.workingDir || undefined, action: "stop" })}><Square size={14} /> 停止</DropdownMenu.Item>
                          <DropdownMenu.Item className="context-menu-item" onSelect={() => setResourceTarget({ kind: "compose", name: row.name, workingDir: row.workingDir || undefined, action: "restart" })}><RotateCw size={14} /> 重启</DropdownMenu.Item>
                          <DropdownMenu.Item className="context-menu-item" onSelect={() => setResourceTarget({ kind: "compose", name: row.name, workingDir: row.workingDir || undefined, action: "build" })}><Code2 size={14} /> 重建</DropdownMenu.Item>
                        </DropdownMenu.Content>
                      </DropdownMenu.Portal>
                    </DropdownMenu.Root>
                    <Button size="sm" variant="ghost" disabled={!row.workingDir} title={row.workingDir ? `配置目录：${row.workingDir}` : "工作目录未知"} onClick={() => { setComposeProject({ name: row.name, workingDir: row.workingDir }); setComposeTab("compose"); }}>目录</Button>
                    <Button size="sm" variant="danger" onClick={() => setResourceTarget({ kind: "compose", name: row.name, workingDir: row.workingDir || undefined, action: "cleanup" })}>删除</Button>
                  </div>
                </div>
              );
            })}
            {!composeRows.filter((row) => row.name.toLocaleLowerCase().includes(composeSearch.trim().toLocaleLowerCase())).length && <div className="empty-panel empty-panel--small"><FileText size={20} /><span>{composeSearch ? "没有匹配的 Compose 项目。" : "未发现 Compose v2 项目。"}</span></div>}
          </div>
          <div className="compose-detail">
            {!composeProject ? <div className="empty-panel empty-panel--small"><Activity size={20} /><span>点击左侧项目查看详情。</span></div> : composeDetails.isLoading ? <div className="page-state">正在读取 Compose 服务和配置…</div> : composeDetails.error ? <div className="page-state page-state--error">{errorMessage(composeDetails.error)}</div> : composeDetails.data ? (() => {
              const current = composeRows.find((row) => row.name === composeProject?.name);
              const allRunning = current ? current.containerCount === current.runningCount && current.runningCount > 0 : false;
              const stateLabel = current ? (current.containerCount === 0 ? "已退出" : `运行 ${current.runningCount}/${current.containerCount}`) : "退出";
              const stateClass = current ? (current.containerCount === 0 ? "is-down" : allRunning ? "is-up" : "is-warn") : "is-down";
              return (
                <div className="compose-detail__inner">
                  <div className="compose-detail-head">
                    <div className="compose-detail-head__meta"><strong>{composeProject.name}</strong><span className="tag">本地</span><span>{current?.createdAt || "—"}</span></div>
                    <span className={`status-pill ${stateClass}`}>{stateLabel}</span>
                  </div>
                  <section>
                    <div className="compose-details__heading"><strong>服务</strong><span>{composeDetails.data.services.length}</span><Button size="sm" variant="ghost" onClick={() => composeLogsMutation.mutate(undefined)}>项目日志</Button></div>
                    <div className="compose-service-table">
                      {composeDetails.data.services.map((service) => {
                        const serviceContainer = docker.data?.containers.find((container) => container.composeProject === composeProject.name && (container.name.startsWith(`${composeProject.name}-${service.name}`) || container.name.includes(`-${service.name}-`)));
                        return <div className="compose-service-row" key={service.name}><span className="compose-service-name"><strong>{service.service || service.name}</strong><small className="mono">{service.image}</small></span><span><span className={`status-pill ${service.state === "running" ? "is-up" : "is-down"}`}>{service.state || "未知"}</span></span><span className="mono">{service.ports || "—"}</span><span className="compose-service-actions"><Button size="sm" variant="ghost" disabled={!serviceContainer} onClick={() => serviceContainer && setTerminalTarget(serviceContainer)}>终端</Button><Button size="sm" variant="ghost" onClick={() => composeLogsMutation.mutate(service.service || service.name)}>日志</Button></span></div>;
                      })}
                      {!composeDetails.data.services.length && <small>没有返回服务。</small>}
                    </div>
                  </section>
                  <section>
                    <div className="compose-detail-tabs">
                      <div className="compose-tabs"><Button size="sm" variant={composeTab === "compose" ? "primary" : "ghost"} onClick={() => setComposeTab("compose")}>compose.yaml</Button><Button size="sm" variant={composeTab === "log" ? "primary" : "ghost"} onClick={() => { setComposeTab("log"); if (!composeLogs) composeLogsMutation.mutate(undefined); }}>日志</Button><Button size="sm" variant={composeTab === "env" ? "primary" : "ghost"} onClick={() => { setComposeTab("env"); if (!composeEnv) composeEnvReadMutation.mutate(); }}>配置</Button></div>
                      <div className="compose-tab-actions">{composeTab === "compose" ? (composeEditing ? <><Button size="sm" variant="ghost" onClick={() => { setComposeEditing(false); setComposeRawFile(null); }}>取消</Button><Button size="sm" variant="primary" onClick={() => void composeSaveMutation.mutate()} disabled={composeSaveMutation.isPending || !composeRawFile}>{composeSaveMutation.isPending ? "校验并保存中…" : "保存并校验"}</Button></> : <Button size="sm" variant="ghost" onClick={() => void composeReadMutation.mutate()} disabled={!composeDetails.data.configPath || composeReadMutation.isPending}>{composeReadMutation.isPending ? "读取中…" : "编辑原始 YAML"}</Button>) : composeTab === "log" ? <Button size="sm" variant="ghost" onClick={() => composeLogsMutation.mutate(undefined)} disabled={composeLogsMutation.isPending}><RefreshCw size={13} className={composeLogsMutation.isPending ? "spin" : ""} /> 刷新</Button> : <Button size="sm" variant="primary" onClick={() => void composeEnvSaveMutation.mutate()} disabled={!composeEnv || composeEnvDraft === composeEnv.content || composeEnvSaveMutation.isPending}>{composeEnvSaveMutation.isPending ? "保存中…" : "保存"}</Button>}</div>
                    </div>
                    {composeSaveMutation.error && <div className="form-error">{errorMessage(composeSaveMutation.error)}</div>}
                    {composeTab === "log" ? <div className="compose-log-block">{composeLogsMutation.error && <div className="form-error">{errorMessage(composeLogsMutation.error)}</div>}<pre className="docker-logs compose-log-output">{composeLogs?.output || (composeLogsMutation.isPending ? "正在读取日志…" : "没有输出")}</pre></div> : composeTab === "env" ? <div className="compose-log-block">{composeEnvSaveMutation.error && <div className="form-error">{errorMessage(composeEnvSaveMutation.error)}</div>}{composeEnvReadMutation.isPending && <div className="page-state">正在读取 .env…</div>}{composeEnvReadMutation.error && <div className="page-state page-state--error">{errorMessage(composeEnvReadMutation.error)}</div>}{!composeEnv && !composeEnvReadMutation.isPending && !composeEnvReadMutation.error && <div className="empty-panel empty-panel--small"><span>未读取 .env 文件。</span><Button size="sm" variant="ghost" onClick={() => composeEnvReadMutation.mutate()}>读取</Button></div>}{composeEnv && <textarea className="compose-env-editor" rows={16} value={composeEnvDraft} onChange={(event) => setComposeEnvDraft(event.target.value)} placeholder="key=value" />}</div> : <div className="compose-yaml-block"><div className="compose-details__heading"><strong>compose.yaml</strong><span>{composeRawFile?.path ?? composeDetails.data.configPath ?? "docker compose config"}</span></div>{composeEditing && <div className="security-note"><ShieldAlert size={16} /><span>保存前会执行 <code>docker compose config -q</code>；失败自动恢复原文件。敏感值不会在默认脱敏配置中显示。</span></div>}<Editor height="320px" defaultLanguage="yaml" value={composeEditing ? composeRawDraft : composeDetails.data.config} onChange={(next) => composeEditing && setComposeRawDraft(next ?? "")} theme="vs-dark" options={{ readOnly: !composeEditing, minimap: { enabled: false }, wordWrap: "on", automaticLayout: true, scrollBeyondLastLine: false }} /></div>}
                  </section>
                  <section className="compose-cleanup-preview">
                    <div className="compose-details__heading"><strong>清理预览（只读）</strong><span>{composeDetails.data.cleanupVolumes.length + composeDetails.data.cleanupNetworks.length + composeDetails.data.orphanContainers.length}</span></div>
                    <small>以下资源来自远端 Docker 的 Compose project label；只读预览不会执行删除。</small>
                    {composeDetails.data.cleanupVolumes.map((name) => <div className="docker-resource-row" key={`cleanup-volume-${name}`}><span>卷 · <span className="mono">{name}</span></span></div>)}
                    {composeDetails.data.cleanupNetworks.map((name) => <div className="docker-resource-row" key={`cleanup-network-${name}`}><span>网络 · <span className="mono">{name}</span></span></div>)}
                    {composeDetails.data.orphanContainers.map((name) => <div className="docker-resource-row" key={`cleanup-orphan-${name}`}><span className="text-warn">孤儿容器 · <span className="mono">{name}</span></span></div>)}
                    {!composeDetails.data.cleanupVolumes.length && !composeDetails.data.cleanupNetworks.length && !composeDetails.data.orphanContainers.length && <small>没有发现带 project label 的可清理资源。</small>}
                    {composeDetails.data.cleanupWarnings.map((warning) => <div className="warning-panel" key={warning}><ShieldAlert size={14} /><span>{warning}</span></div>)}
                  </section>
                </div>
              );
            })() : null}
          </div>
        </div>
      </section>}
      {section === "registry" && <section className="docker-section"><header><Database size={15} /><strong>镜像仓库</strong><span>0</span></header><div className="empty-panel empty-panel--small"><Database size={20} /><span>暂无镜像仓库。仓库配置由 1Panel 服务端管理，可在 Web 面板中维护。</span></div></section>}
      {section === "templates" && <section className="docker-section"><header><FileText size={15} /><strong>编排模版</strong><span>0</span></header><div className="empty-panel empty-panel--small"><FileText size={20} /><span>还没有编排模版。</span></div></section>}
      {section === "config" && <section className="docker-section"><header><Settings2 size={15} /><strong>配置</strong></header><div className="docker-config-panel"><div className="docker-config-row"><span>Socket 路径</span><code>{docker.data.socketPath ?? "unix:///var/run/docker.sock"}</code></div><div className="docker-config-row docker-config-row--accel"><span>镜像加速</span><div className="docker-accel-list">{MIRROR_ACCELERATORS.map((mirror) => <code key={mirror}>{mirror}</code>)}</div></div><div className="docker-engine-meta"><div><span>引擎版本</span><strong>{docker.data.version ?? "未知"}</strong></div><div><span>API 版本</span><strong>{docker.data.apiVersion ?? "未知"}</strong></div><div><span>系统</span><strong>{docker.data.os ?? "未知"} / {docker.data.architecture ?? "未知"}</strong></div><div><span>存储驱动</span><strong>{docker.data.storageDriver ?? "未知"}</strong></div><div><span>数据根目录</span><strong className="mono">{docker.data.rootDir ?? "未返回"}</strong></div></div></div></section>}
      </>)}
    <Dialog.Root open={eventsOpen} onOpenChange={setEventsOpen}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content docker-logs-dialog"><div className="dialog-header"><div><Dialog.Title>Docker 事件</Dialog.Title><Dialog.Description>读取最近一段 daemon 事件，仅展示有限字段，不保存原始 actor 属性。</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="docker-log-toolbar"><label><span>时间范围</span><select value={eventsSince} onChange={(event) => setEventsSince(Number(event.target.value))}><option value={60}>最近 1 分钟</option><option value={300}>最近 5 分钟</option><option value={900}>最近 15 分钟</option><option value={3600}>最近 1 小时</option></select></label><Button size="sm" variant="ghost" onClick={() => eventsMutation.mutate()} disabled={eventsMutation.isPending}><RefreshCw size={13} className={eventsMutation.isPending ? "spin" : ""} />刷新</Button></div>{eventsMutation.isPending && <div className="page-state">正在读取 Docker daemon 事件…</div>}{eventsMutation.data ? <div className="docker-events-list">{eventsMutation.data.events.length ? eventsMutation.data.events.slice().reverse().map((event, index) => <div className="docker-event-row" key={`${event.timestamp ?? "none"}-${event.action}-${index}`}><span className="mono">{event.timestamp ? new Date(event.timestamp * 1000).toLocaleString() : "—"}</span><strong>{event.eventType}</strong><span>{event.action}</span><small>{event.actorName || event.actorId || "未知对象"}</small></div>) : <div className="empty-panel empty-panel--small"><Activity size={20} /><span>时间范围内没有 Docker 事件。</span></div>}<small className="docker-events-footnote">获取于 {new Date(eventsMutation.data.fetchedAt).toLocaleString()} · 最多显示 200 条</small></div> : !eventsMutation.isPending && <div className="empty-panel empty-panel--small"><Activity size={20} /><span>点击刷新读取事件。</span></div>}</Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={buildOpen} onOpenChange={(open) => { setBuildOpen(open); if (!open && !buildMutation.isPending) setBuildOutput(""); }}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content docker-logs-dialog"><div className="dialog-header"><div><Dialog.Title>构建 Docker 镜像</Dialog.Title><Dialog.Description>在目标服务器上执行 docker build；上下文目录必须是远端绝对路径，输出仅在本页和任务中心显示。</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭" disabled={buildMutation.isPending}><X size={17} /></button></Dialog.Close></div><form className="server-form" onSubmit={(event) => { event.preventDefault(); const taskId = crypto.randomUUID(); setBuildTaskId(taskId); setBuildOutput(""); buildMutation.mutate(taskId); }}><div className="field-grid field-grid--2"><label><span>构建上下文目录</span><input required value={buildForm.contextPath} onChange={(event) => setBuildForm((current) => ({ ...current, contextPath: event.target.value }))} placeholder="/opt/app" /></label><label><span>镜像标签</span><input required value={buildForm.image} onChange={(event) => setBuildForm((current) => ({ ...current, image: event.target.value }))} placeholder="example/app:latest" /></label></div><label><span>Dockerfile 路径（可选，远端路径）</span><input value={buildForm.dockerfilePath} onChange={(event) => setBuildForm((current) => ({ ...current, dockerfilePath: event.target.value }))} placeholder="Dockerfile 或 /opt/app/Dockerfile.prod" /></label><label><span>构建参数（每行 KEY=VALUE）</span><textarea rows={4} value={buildForm.buildArgs} onChange={(event) => setBuildForm((current) => ({ ...current, buildArgs: event.target.value }))} placeholder="NODE_ENV=production" /></label>{buildMutation.error && <div className="form-error">{errorMessage(buildMutation.error)}</div>}<div className="dialog-actions"><Button type="button" variant="ghost" onClick={() => setBuildOpen(false)} disabled={buildMutation.isPending}>关闭</Button>{buildMutation.isPending ? <Button type="button" variant="danger" onClick={() => buildTaskId && void api.cancelCommandTask(buildTaskId)}>取消构建</Button> : <Button type="submit" variant="primary" disabled={!buildForm.contextPath.trim() || !buildForm.image.trim()}>开始构建</Button>}</div></form>{buildOutput && <pre className="docker-logs docker-build-output">{buildOutput}</pre>}</Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={runOpen} onOpenChange={setRunOpen}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content"><div className="dialog-header"><div><Dialog.Title>Run Container</Dialog.Title><Dialog.Description>执行前只生成受控 docker run 参数，成功后 inspect 验证。</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><form className="server-form" onSubmit={(event) => { event.preventDefault(); runMutation.mutate(); }}><div className="field-grid field-grid--2"><label><span>镜像</span><input required value={runForm.image} onChange={(event) => setRunForm((current) => ({ ...current, image: event.target.value }))} /></label><label><span>名称</span><input value={runForm.name} onChange={(event) => setRunForm((current) => ({ ...current, name: event.target.value }))} placeholder="可选" /></label></div><div className="field-grid field-grid--2"><label><span>端口映射</span><input value={runForm.ports} onChange={(event) => setRunForm((current) => ({ ...current, ports: event.target.value }))} placeholder="8080:80, 8443:443" /></label><label><span>网络</span><input value={runForm.network} onChange={(event) => setRunForm((current) => ({ ...current, network: event.target.value }))} placeholder="可选" /></label></div><label><span>环境变量（每行 KEY=VALUE）</span><textarea rows={4} value={runForm.environment} onChange={(event) => setRunForm((current) => ({ ...current, environment: event.target.value }))} /></label><div className="field-grid field-grid--2"><label><span>重启策略</span><select value={runForm.restartPolicy} onChange={(event) => setRunForm((current) => ({ ...current, restartPolicy: event.target.value }))}><option value="no">no</option><option value="unless-stopped">unless-stopped</option><option value="always">always</option><option value="on-failure:5">on-failure:5</option></select></label><div className="run-checks"><label className="check-field"><input type="checkbox" checked={runForm.autoRemove} onChange={(event) => setRunForm((current) => ({ ...current, autoRemove: event.target.checked }))} /><span>容器退出后自动删除</span></label><label className="check-field"><input type="checkbox" checked={runForm.privileged} onChange={() => setRunForm((current) => ({ ...current, privileged: !current.privileged }))} /><span>以 privileged 运行（高风险）</span></label></div></div>{runMutation.error && <div className="form-error">{errorMessage(runMutation.error)}</div>}<div className="dialog-actions"><Button type="button" variant="ghost" onClick={() => setRunOpen(false)}>取消</Button><Button type="submit" variant={runForm.privileged ? "danger" : "primary"} disabled={runMutation.isPending}>{runMutation.isPending ? "创建并验证中…" : runForm.privileged ? "确认高风险 Run" : "创建容器"}</Button></div></form></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={!!pending} onOpenChange={(open) => { if (!open) { setPending(null); setUseSudo(false); setForceDelete(false); } }}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow confirm-dialog"><Dialog.Title>{pending?.action === "remove" ? "删除容器" : pending?.action === "rename" ? "重命名容器" : `确认 ${pending?.action}`}</Dialog.Title><Dialog.Description>将对容器 <strong>{pending?.container.name}</strong>（{pending?.container.id}）执行 {pending?.action}，随后重新读取状态。</Dialog.Description>{pending?.action === "rename" && <label className="server-form"><span>新名称</span><input value={renameDraft} onChange={(event) => setRenameDraft(event.target.value)} autoFocus /></label>}{pending?.action === "remove" && <label className="force-toggle"><input type="checkbox" checked={forceDelete} onChange={(event) => setForceDelete(event.target.checked)} /> 强制删除（运行中的容器也会被停止）</label>}{profile.data && profile.data.sudoMode !== "none" && <label className="force-toggle"><input type="checkbox" checked={useSudo} onChange={(event) => setUseSudo(event.target.checked)} /> 使用已配置的 sudo</label>}{action.error && <div className="form-error">{errorMessage(action.error)}</div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => { setPending(null); setUseSudo(false); setForceDelete(false); }}>取消</Button><Button variant={pending?.action === "remove" || pending?.action === "kill" ? "danger" : "primary"} onClick={() => action.mutate()} disabled={action.isPending || (pending?.action === "rename" && (!renameDraft.trim() || renameDraft.trim() === pending.container.name))}>{action.isPending ? "执行并验证中…" : pending?.action === "remove" && forceDelete ? "确认强制删除" : pending?.action === "rename" ? "确认重命名" : "确认执行"}</Button></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={!!batchPending} onOpenChange={(open) => !open && setBatchPending(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow confirm-dialog"><Dialog.Title>批量{batchPending?.action === "remove" ? "删除" : batchPending?.action === "stop" ? "停止" : batchPending?.action === "kill" ? "终止" : batchPending?.action === "unpause" ? "恢复" : batchPending?.action === "pause" ? "暂停" : batchPending?.action === "start" ? "启动" : "重启"}容器</Dialog.Title><Dialog.Description>将对选中的 <strong>{batchPending?.containers.length}</strong> 个容器依次执行 <code>{batchPending?.action}</code>，全部完成后再重新读取状态。</Dialog.Description><div className="batch-pending-list">{batchPending?.containers.slice(0, 8).map((container) => <span className="mono" key={container.id}>{container.name}</span>)}{batchPending && batchPending.containers.length > 8 && <span>… 等 {batchPending.containers.length} 个</span>}</div>{profile.data && profile.data.sudoMode !== "none" && <label className="force-toggle"><input type="checkbox" checked={useSudo} onChange={(event) => setUseSudo(event.target.checked)} /> 使用已配置的 sudo</label>}{batchMutation.error && <div className="form-error">{errorMessage(batchMutation.error)}</div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => setBatchPending(null)}>取消</Button><Button variant={batchPending?.action === "remove" || batchPending?.action === "kill" ? "danger" : "primary"} onClick={() => batchMutation.mutate()} disabled={batchMutation.isPending}>{batchMutation.isPending ? "按顺序执行中…" : "确认执行"}</Button></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={!!logs} onOpenChange={(open) => !open && setLogs(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content docker-logs-dialog"><div className="dialog-header"><div><Dialog.Title>容器日志</Dialog.Title><Dialog.Description>{logs?.containerId} · 最近 {logTail} 行{logsPaused ? " · 已暂停刷新" : ""}</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="docker-log-toolbar"><label><Search size={14} /><input value={logQuery} onChange={(event) => setLogQuery(event.target.value)} placeholder="筛选日志文本" /></label><label className="docker-log-tail"><span>行数</span><select value={logTail} onChange={(event) => setLogTail(Number(event.target.value))}><option value={100}>100</option><option value={200}>200</option><option value={500}>500</option><option value={1000}>1000</option></select></label><Button size="sm" variant="ghost" onClick={() => setLogsPaused((paused) => !paused)}><Pause size={13} /> {logsPaused ? "继续" : "暂停"}</Button><Button size="sm" variant="ghost" onClick={() => setLogsCleared(true)} disabled={logsCleared}><Trash2 size={13} /> 清空视图</Button><Button size="sm" variant="ghost" onClick={() => logs && logsMutation.mutate(logs.containerId)} disabled={!logs || logsMutation.isPending}><RefreshCw size={13} className={logsMutation.isPending ? "spin" : ""} /> 刷新</Button><Button size="sm" variant="ghost" onClick={() => void navigator.clipboard?.writeText(visibleLogOutput)}><Copy size={13} /> 复制</Button><Button size="sm" variant="ghost" onClick={() => downloadText(`docker-${logs?.containerId ?? "logs"}.log`, visibleLogOutput)}><Download size={13} /> 下载</Button></div><pre className="docker-logs">{visibleLogOutput || "没有匹配的日志。"}</pre></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={!!followOutput || followMutation.isPending} onOpenChange={() => undefined}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content docker-logs-dialog"><div className="dialog-header"><div><Dialog.Title>实时日志（最多 30 秒）</Dialog.Title><Dialog.Description>日志跟随会自动结束，避免遗留远程任务</Dialog.Description></div><Button variant="ghost" size="sm" onClick={() => setFollowOutput("")}>关闭</Button></div><pre className="docker-logs">{followOutput || (followMutation.isPending ? "正在等待日志…" : "没有输出")}</pre></Dialog.Content></Dialog.Portal></Dialog.Root>
    <ContainerTerminalDialog serverId={serverId} container={terminalTarget} onClose={() => setTerminalTarget(null)} />
    <Dialog.Root open={!!resourceTarget} onOpenChange={(open) => !open && setResourceTarget(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow confirm-dialog">{resourceTarget?.action === "create" && <div className="destructive-icon"><Container size={22} /></div>}{resourceTarget?.action !== "create" && <div className="destructive-icon"><ShieldAlert size={22} /></div>}<Dialog.Title>{resourceTarget?.action === "create" ? `创建 Docker ${resourceTarget?.kind === "volume" ? "存储卷" : "网络"}` : "确认 Docker 资源操作"}</Dialog.Title><Dialog.Description>{resourceTarget?.action === "create" ? "远端执行 docker volume/network create，创建成功后重新读取并验证。" : resourceTarget?.action === "remove" ? "该操作会删除远端 Docker 资源；执行后会重新读取并验证结果。" : resourceTarget?.action === "down" || resourceTarget?.action === "stop" ? "该操作会停止 Compose 项目中的服务。" : resourceTarget?.action === "cleanup" ? "这会执行 Compose down --remove-orphans --volumes，停止项目并删除声明的卷。" : "将对远端 Docker 资源执行操作。"} 目标：{resourceTarget?.action === "create" ? resourceDraft.name || "（待填写）" : resourceTarget?.name}</Dialog.Description>{resourceTarget?.action === "create" && <div className="server-form"><div className="field-grid field-grid--2"><label><span>名称</span><input autoFocus value={resourceDraft.name} onChange={(event) => setResourceDraft((current) => ({ ...current, name: event.target.value }))} placeholder="myapp-data" /></label><label><span>驱动（可选）</span><input value={resourceDraft.driver} onChange={(event) => setResourceDraft((current) => ({ ...current, driver: event.target.value }))} placeholder={resourceTarget.kind === "volume" ? "local" : "bridge"} /></label></div></div>}{resourceMutation.error && <div className="form-error">{errorMessage(resourceMutation.error)}</div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => setResourceTarget(null)}>取消</Button><Button variant={resourceTarget?.action === "remove" || resourceTarget?.action === "down" || resourceTarget?.action === "stop" || resourceTarget?.action === "cleanup" ? "danger" : "primary"} onClick={() => resourceMutation.mutate()} disabled={resourceMutation.isPending || (resourceTarget?.action === "create" && !resourceDraft.name.trim())}>{resourceMutation.isPending ? "执行并验证中…" : "确认执行"}</Button></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={!!resourceBatch} onOpenChange={(open) => !open && setResourceBatch(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow confirm-dialog"><div className="destructive-icon"><ShieldAlert size={22} /></div><Dialog.Title>批量删除{resourceBatch?.kind === "volume" ? "存储卷" : resourceBatch?.kind === "network" ? "网络" : "镜像"}</Dialog.Title><Dialog.Description>将对选中的 <strong>{resourceBatch?.names.length}</strong> 个{resourceBatch?.kind === "image" ? "镜像" : resourceBatch?.kind === "network" ? "网络" : "存储卷"}依次执行删除，全部完成后再重新读取状态。</Dialog.Description><div className="batch-pending-list">{resourceBatch?.names.slice(0, 8).map((name) => <span className="mono" key={name}>{name}</span>)}{resourceBatch && resourceBatch.names.length > 8 && <span>… 等 {resourceBatch.names.length} 个</span>}</div>{profile.data && profile.data.sudoMode !== "none" && <label className="force-toggle"><input type="checkbox" checked={useSudo} onChange={(event) => setUseSudo(event.target.checked)} /> 使用已配置的 sudo</label>}{batchResourceMutation.error && <div className="form-error">{errorMessage(batchResourceMutation.error)}</div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => setResourceBatch(null)}>取消</Button><Button variant="danger" onClick={() => batchResourceMutation.mutate()} disabled={batchResourceMutation.isPending}>{batchResourceMutation.isPending ? "按顺序删除中…" : "确认删除"}</Button></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={!!detail} onOpenChange={(open) => !open && setDetail(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content docker-logs-dialog"><div className="dialog-header"><div><Dialog.Title>{detail?.title}</Dialog.Title><Dialog.Description>{detail?.title.startsWith("stats") ? `只读 Docker CLI 结果 · ${statsHistory.length} 个本页采样` : "只读 Docker CLI 结果"}</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="docker-log-toolbar"><label><Search size={14} /><input value={detailQuery} onChange={(event) => setDetailQuery(event.target.value)} placeholder="搜索 JSON / 输出" /></label><Button size="sm" variant="ghost" onClick={() => void navigator.clipboard?.writeText(filteredDetailOutput)}><Copy size={13} /> 复制</Button>{detail?.title.startsWith("stats") && <Button size="sm" variant="ghost" onClick={() => detailMutation.mutate({ kind: "stats", containerId: detail.containerId })} disabled={detailMutation.isPending}><RefreshCw size={13} className={detailMutation.isPending ? "spin" : ""} /> 采样</Button>}</div><pre className="docker-logs">{filteredDetailOutput || "没有匹配的结果。"}</pre>{detail?.title.startsWith("stats") && statsHistory.length > 1 && <div className="docker-stats-history">{statsHistory.slice().reverse().map((sample) => <div key={sample.at}><span>{sample.at}</span><code>{sample.output.slice(0, 160)}</code></div>)}</div>}</Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={composeCreateOpen} onOpenChange={(open) => setComposeCreateOpen(open)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--wide"><div className="dialog-header"><div><Dialog.Title>创建 Compose 项目</Dialog.Title><Dialog.Description>在远端目录写入 compose.yaml 并执行 docker compose up -d；同名 compose.yaml 已存在时拒绝覆盖。</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><form className="server-form" onSubmit={(event) => { event.preventDefault(); composeCreateMutation.mutate(); }}><div className="field-grid field-grid--2"><label><span>项目名称</span><input required value={composeCreateForm.name} onChange={(event) => setComposeCreateForm((current) => ({ ...current, name: event.target.value }))} placeholder="myapp" /></label><label><span>目标目录（可选，默认 /opt/1panel/compose/名称）</span><input value={composeCreateForm.workingDir} onChange={(event) => setComposeCreateForm((current) => ({ ...current, workingDir: event.target.value }))} placeholder="/opt/1panel/compose/myapp" /></label></div><label className="force-toggle"><input type="checkbox" checked={composeCreateForm.forcePull} onChange={(event) => setComposeCreateForm((current) => ({ ...current, forcePull: event.target.checked }))} /> 强制拉取新镜像（up --pull always）</label>{profile.data && profile.data.sudoMode !== "none" && <label className="force-toggle"><input type="checkbox" checked={useSudo} onChange={(event) => setUseSudo(event.target.checked)} /> 使用已配置的 sudo（目录权限不足时）</label>}<label><span>compose.yaml 内容</span><Editor height="280px" defaultLanguage="yaml" value={composeCreateForm.content} onChange={(next) => setComposeCreateForm((current) => ({ ...current, content: next ?? "" }))} theme="vs-dark" options={{ minimap: { enabled: false }, wordWrap: "on", automaticLayout: true, scrollBeyondLastLine: false }} /></label>{composeCreateMutation.error && <div className="form-error">{errorMessage(composeCreateMutation.error)}</div>}<div className="dialog-actions"><Button type="button" variant="ghost" onClick={() => setComposeCreateOpen(false)}>取消</Button><Button type="submit" variant="primary" disabled={composeCreateMutation.isPending || !composeCreateForm.name.trim() || !composeCreateForm.content.trim()}>{composeCreateMutation.isPending ? "创建并启动中…" : "创建并启动"}</Button></div></form></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={!!pruneTarget} onOpenChange={(open) => { if (!open) { setPruneTarget(null); setPruneOutput(""); setUseSudo(false); } }}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow confirm-dialog"><div className="destructive-icon"><ShieldAlert size={22} /></div><Dialog.Title>释放{pruneTarget?.label}空间</Dialog.Title><Dialog.Description>将执行 <code>{pruneTarget ? PRUNE_COMMANDS[pruneTarget.kind] : ""}</code>，删除远端 Docker 的未使用资源；执行后重新读取磁盘占用。建议确认确认不再需要之后再释放。</Dialog.Description>{profile.data && profile.data.sudoMode !== "none" && <label className="force-toggle"><input type="checkbox" checked={useSudo} onChange={(event) => setUseSudo(event.target.checked)} /> 使用已配置的 sudo</label>}{pruneMutation.error && <div className="form-error">{errorMessage(pruneMutation.error)}</div>}{pruneOutput && <pre className="docker-logs docker-prune-output">{pruneOutput}</pre>}<div className="dialog-actions"><Button variant="ghost" onClick={() => { setPruneTarget(null); setPruneOutput(""); setUseSudo(false); }}>取消</Button><Button variant="danger" onClick={() => { if (pruneOutput) { setPruneTarget(null); setPruneOutput(""); setUseSudo(false); } else { pruneMutation.mutate(); } }} disabled={pruneMutation.isPending}>{pruneMutation.isPending ? "释放中…" : pruneOutput ? "完成" : "确认释放"}</Button></div></Dialog.Content></Dialog.Portal></Dialog.Root>
  </section>;
}

/** 容器概览：整行容器计数卡 + 3×2 统计格 + 磁盘占用表 + 配置表（对齐 Web 1Panel）。 */
function DockerOverview({ data, onOpen, onPrune }: { data: DockerSnapshot; onOpen: (section: DockerSection) => void; onPrune: (kind: PruneKind, label: string) => void }) {
  const total = data.containers.length;
  const running = data.containers.filter((container) => container.status.toLocaleLowerCase().startsWith("up")).length;
  const diskRows = useMemo(() => parseDiskUsage(data.diskUsage), [data.diskUsage]);
  const stats: Array<{ label: string; value: number; section: DockerSection }> = [
    { label: "编排", value: data.composeProjects.length, section: "compose" },
    { label: "编排模版", value: 0, section: "templates" },
    { label: "镜像", value: data.images.length, section: "images" },
    { label: "镜像仓库", value: 0, section: "registry" },
    { label: "网络", value: data.networks.length, section: "networks" },
    { label: "存储卷", value: data.volumes.length, section: "volumes" },
  ];
  return <div className="docker-overview">
    <section className="docker-overview-card">
      <header><span className="docker-overview-card__title">容器</span><span className="docker-overview-badges"><span className="docker-badge">所有 * {total}</span><span className="docker-badge">已启动 * {running}</span><span className="docker-badge">已停止 * {total - running}</span></span></header>
      <div className="docker-overview-count">{total}</div>
    </section>
    <section className="docker-overview-stats">{stats.map((stat) => <button className="docker-stat-card" key={stat.label} onClick={() => onOpen(stat.section)}><span className="docker-stat-card__title">{stat.label}</span><em>{stat.value}</em></button>)}</section>
    <section className="docker-overview-card">
      <header><span className="docker-overview-card__title">磁盘占用</span></header>
      <div className="docker-overview-card__body"><div className="docker-disk-row docker-disk-row--head">{DISK_COLUMNS.map((column) => <div key={column.type}>{column.label}</div>)}</div><div className="docker-disk-row">{DISK_COLUMNS.map((column) => { const row = diskRows.find((entry) => entry.Type === column.type); const reclaimable = row ? cleanSize(row.Reclaimable) : "0B"; return <div className="docker-disk-cell" key={column.type}><span>已占用：{row ? cleanSize(row.Size) : "0B"}，可释放：{reclaimable}</span>{reclaimable !== "0B" && <Button size="sm" variant="ghost" className="docker-disk-release" onClick={() => onPrune(column.pruneKind, column.label)}>释放</Button>}</div>; })}</div></div>
    </section>
    <section className="docker-overview-card">
      <header><span className="docker-overview-card__title">配置</span></header>
      <div className="docker-overview-card__body docker-config-table">
        <div className="docker-config-row"><span>Socket 路径</span><div className="docker-config-value"><code>{data.socketPath ?? "unix:///var/run/docker.sock"}</code></div></div>
        <div className="docker-config-row"><span>镜像加速</span><div className="docker-config-value">{MIRROR_ACCELERATORS.map((mirror) => <button className="docker-accel-link" key={mirror} onClick={() => onOpen("config")}>{mirror}</button>)}</div></div>
      </div>
    </section>
  </div>;
}

/** Web 1Panel 风格容器表行：选择框/名称/镜像/状态 pill/资源/IP/端口/运行时长/操作(终端-日志-更多)。 */
function ContainerTableRow({ container, selected, onToggle, onAction, onOpenPort, onLogs, onFollow, onExec, onDetail }: { container: DockerContainerInfo; selected: boolean; onToggle: (id: string) => void; onAction: (action: PendingAction["action"]) => void; onOpenPort: (port: number) => void; onLogs: () => void; onFollow: () => void; onExec: () => void; onDetail: (kind: DetailKind) => void }) {
  const state = containerState(container);
  const running = state === "running";
  const paused = state === "paused";
  const publishedPort = firstPublishedPort(container.ports);
  const runningLabel = running ? "" : "docker-row-cell--muted";
  const [moreOpen, setMoreOpen] = useState(false);
  const moreCloseTimer = useRef<number | null>(null);
  const moreSuppressUntil = useRef(0);
  const openMore = () => {
    // 菜单覆盖指针时，Escape 关闭会触发 Chromium 重新命中目标并再次 pointerenter，
    // 若不加保护菜单会被立即重开；关闭后 250ms 内忽略悬停打开。
    if (Date.now() < moreSuppressUntil.current) return;
    if (moreCloseTimer.current) window.clearTimeout(moreCloseTimer.current);
    setMoreOpen(true);
  };
  // 菜单内容在 Portal 中，CDP/真实键盘的 Escape 事件无法经由 React onKeyDown 送达；
  // 在 document 捕获阶段监听，确保 Escape 必定关闭菜单。
  useEffect(() => {
    if (!moreOpen) return;
    const onDocumentKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        moreSuppressUntil.current = Date.now() + 250;
        setMoreOpen(false);
      }
    };
    document.addEventListener("keydown", onDocumentKeyDown, true);
    return () => document.removeEventListener("keydown", onDocumentKeyDown, true);
  }, [moreOpen]);
  const closeMoreSoon = () => {
    if (moreCloseTimer.current) window.clearTimeout(moreCloseTimer.current);
    moreCloseTimer.current = window.setTimeout(() => setMoreOpen(false), 180);
  };
  return <div className="ops-row docker-container-row">
    <span className="docker-row-check"><input type="checkbox" checked={selected} onChange={() => onToggle(container.id)} /></span>
    <span className="docker-row-name"><button type="button" className="text-link" onClick={() => onDetail("inspect")}><strong>{container.name}</strong></button><small className="mono">{container.id}</small></span>
    <span className="mono docker-row-image">{container.image}</span>
    <span><span className={`status-pill ${running ? "is-up" : paused ? "is-paused" : "is-down"}`}>{stateLabel(state)}</span>{container.composeProject && <small className="docker-row-sub">{container.composeProject}</small>}</span>
    <span className={`mono ${runningLabel}`}>CPU {container.cpuPercent ? container.cpuPercent.toFixed(2) : "—"}% / 内存 {container.memoryPercent ? container.memoryPercent.toFixed(2) : "—"}%</span>
    <span className={`mono ${runningLabel}`}>{container.ipAddresses || "—"}</span>
    <span>{publishedPort ? <button type="button" className="text-link mono" onClick={() => onOpenPort(publishedPort)} title="点击在本机浏览器打开">{container.ports}</button> : <span className={`mono ${runningLabel}`}>{container.ports || "—"}</span>}</span>
    <span className={`mono ${runningLabel}`}>{container.status}</span>
    <span className="docker-row-actions">
      <Button size="sm" variant="ghost" disabled={!running} onClick={onExec}>终端</Button>
      <Button size="sm" variant="ghost" onClick={onLogs}>日志</Button>
      <DropdownMenu.Root open={moreOpen} onOpenChange={setMoreOpen}>
        <DropdownMenu.Trigger asChild><Button size="sm" variant="ghost" className="docker-more-trigger" onClick={openMore} onPointerEnter={openMore} onPointerLeave={closeMoreSoon}>更多<ChevronDown size={12} /></Button></DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content className="context-menu" align="end" sideOffset={4} onPointerEnter={openMore} onPointerLeave={closeMoreSoon} onKeyDown={(event) => { if (event.key === "Escape") { event.preventDefault(); setMoreOpen(false); } }}>
            <DropdownMenu.Item className="context-menu-item" disabled={!running} onSelect={() => onDetail("stats")}><Activity size={14} /> 监控</DropdownMenu.Item>
            <DropdownMenu.Item className="context-menu-item" disabled={!!container.composeProject} onSelect={() => onAction("rename")}><RotateCw size={14} /> 重命名</DropdownMenu.Item>
            <DropdownMenu.Separator className="context-menu-sep" />
            <DropdownMenu.Item className="context-menu-item" disabled={running} onSelect={() => onAction("start")}><Play size={14} /> 启动</DropdownMenu.Item>
            <DropdownMenu.Item className="context-menu-item" disabled={!running} onSelect={() => onAction("stop")}><Square size={14} /> 停止</DropdownMenu.Item>
            <DropdownMenu.Item className="context-menu-item" disabled={!running} onSelect={() => onAction("restart")}><RotateCw size={14} /> 重启</DropdownMenu.Item>
            <DropdownMenu.Item className="context-menu-item" disabled={!running} onSelect={() => onAction("kill")}><SquareTerminal size={14} /> 终止</DropdownMenu.Item>
            <DropdownMenu.Item className="context-menu-item" disabled={!running || paused} onSelect={() => onAction("pause")}><Pause size={14} /> 暂停</DropdownMenu.Item>
            <DropdownMenu.Item className="context-menu-item" disabled={!paused} onSelect={() => onAction("unpause")}><Play size={14} /> 恢复</DropdownMenu.Item>
            <DropdownMenu.Separator className="context-menu-sep" />
            <DropdownMenu.Item className="context-menu-item" onSelect={onFollow}><RefreshCw size={14} /> 跟随日志</DropdownMenu.Item>
            <DropdownMenu.Item className="context-menu-item" onSelect={() => onDetail("inspect")}><Code2 size={14} /> 检查详情</DropdownMenu.Item>
            <DropdownMenu.Item className="context-menu-item" onSelect={() => onDetail("top")}><ListTree size={14} /> 进程列表</DropdownMenu.Item>
            <DropdownMenu.Item className="context-menu-item" onSelect={() => void navigator.clipboard?.writeText(container.id)}><Copy size={14} /> 复制 ID</DropdownMenu.Item>
            <DropdownMenu.Item className="context-menu-item" onSelect={() => void navigator.clipboard?.writeText(container.name)}><Copy size={14} /> 复制名称</DropdownMenu.Item>
            <DropdownMenu.Separator className="context-menu-sep" />
            <DropdownMenu.Item className="context-menu-item context-menu-item--danger" onSelect={() => onAction("remove")}><Trash2 size={14} /> 删除</DropdownMenu.Item>
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
    </span>
  </div>;
}

/** 将远程终端输出的 base64 数据解码为 UTF-8 字节流。 */
function decodeTerminalData(value: string): Uint8Array {
  const binary = atob(value);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

/** 交互式容器终端：通过 SSH exec 启动 docker exec -it，接入 xterm 输出。 */
function ContainerTerminalDialog({ serverId, container, onClose }: { serverId: string; container: DockerContainerInfo | null; onClose: () => void }) {
  return (
    <Dialog.Root open={!!container} onOpenChange={(open) => !open && onClose()}>
      <Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content docker-logs-dialog">
        {container && <ContainerTerminalSurface key={container.id} serverId={serverId} container={container} />}
      </Dialog.Content></Dialog.Portal>
    </Dialog.Root>
  );
}

/**
 * 渲染对话框内容并持有 host 节点：xterm 必须与它的宿主动画在同一提交中初始化。
 * 若把宿主 div 放在 Dialog.Portal 外层组件里，Radix Presence 会让内容晚一个提交挂载，
 * 外层 useEffect 早退后依赖不再变化而永不重跑，xterm 就永远不初始化。
 */
function ContainerTerminalSurface({ serverId, container }: { serverId: string; container: DockerContainerInfo }) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const terminalIdRef = useRef<string | null>(null);
  const [status, setStatus] = useState<"connecting" | "online" | "error" | "closed">("connecting");
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    const terminal = new Terminal({
      cursorBlink: true, cursorStyle: "bar", fontFamily: '"Cascadia Mono", "JetBrains Mono", monospace',
      fontSize: 13, lineHeight: 1.25, scrollback: 10_000, allowProposedApi: false,
      theme: { background: "#090d0e", foreground: "#d9e2df", cursor: "#c7f36b", selectionBackground: "#40502b", black: "#111718", brightBlack: "#65716e", green: "#b7dc62", brightGreen: "#c7f36b", red: "#ff735c", brightRed: "#ff9180", yellow: "#e8b85e", brightYellow: "#f3ca7c", blue: "#6fa8dc", brightBlue: "#8ebfe9", magenta: "#b28ad5", brightMagenta: "#c8a4e6", cyan: "#66b8ad", brightCyan: "#82d0c5", white: "#c8d0ce", brightWhite: "#f2f6f4" },
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(hostRef.current!);
    fit.fit();
    const handleEvent = (event: TerminalEvent) => {
      if (disposed) return;
      if (event.event === "data") terminal.write(decodeTerminalData(event.data.data));
      if (event.event === "exit") terminal.writeln(`\r\n\x1b[33m[容器命令已退出: ${event.data.exitStatus}]\x1b[0m`);
      if (event.event === "closed") { setStatus("closed"); terminal.writeln("\r\n\x1b[31m[容器终端已断开]\x1b[0m"); }
    };
    api.openTerminal(serverId, terminal.cols, terminal.rows, handleEvent, `docker exec -it ${container.id} /bin/sh`).then((terminalId) => {
      if (disposed) { void api.closeTerminal(terminalId); return; }
      terminalIdRef.current = terminalId;
      setStatus("online");
      terminal.focus();
    }).catch((reason) => { if (!disposed) { setStatus("error"); setError(errorMessage(reason)); } });
    const dataDisposable = terminal.onData((data) => {
      if (terminalIdRef.current) void api.writeTerminal(terminalIdRef.current, new TextEncoder().encode(data));
    });
    const resizeObserver = new ResizeObserver(() => {
      fit.fit();
      if (terminalIdRef.current) void api.resizeTerminal(terminalIdRef.current, terminal.cols, terminal.rows);
    });
    resizeObserver.observe(hostRef.current!);
    return () => {
      disposed = true;
      resizeObserver.disconnect();
      dataDisposable.dispose();
      terminal.dispose();
      if (terminalIdRef.current) void api.closeTerminal(terminalIdRef.current);
      terminalIdRef.current = null;
    };
  }, [container.id, serverId]);
  return (
    <>
      <div className="dialog-header">
        <div><Dialog.Title>容器终端 · {container.name}</Dialog.Title><Dialog.Description>docker exec -it /bin/sh · 交互式控制台{status === "closed" ? " · 已断开" : ""}</Dialog.Description></div>
        <Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close>
      </div>
      {status === "error" && <div className="form-error">{error}</div>}
      {status === "connecting" && <div className="page-state">正在创建容器终端…</div>}
      <div ref={hostRef} className="docker-terminal-host" />
    </>
  );
}
