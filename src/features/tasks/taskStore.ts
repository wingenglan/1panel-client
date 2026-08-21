import { create } from "zustand";
import { api } from "../../lib/api";
import type { TaskRecord } from "../../types/server";

export type CommandTaskType = "tool-install" | "database-install" | "docker-pull" | "docker-build" | "docker-follow" | "docker-compose" | "ai-chat" | "log-follow";
export type TaskStatus = "queued" | "running" | "success" | "failed" | "cancelled" | "interrupted";

export interface CommandTask {
  id: string;
  type: CommandTaskType;
  serverId: string;
  title: string;
  status: TaskStatus;
  progress: number | null;
  bytesTransferred: number;
  totalBytes: number | null;
  startedAt: number;
  finishedAt?: number;
  error?: string;
  cancelSupported: boolean;
  retryPayloadJson?: string;
  retry?: () => void;
}

interface CommandTaskState {
  tasks: CommandTask[];
  add: (task: Omit<CommandTask, "startedAt" | "cancelSupported" | "progress" | "bytesTransferred" | "totalBytes"> & { cancelSupported?: boolean; progress?: number | null; bytesTransferred?: number; totalBytes?: number | null }) => void;
  hydrate: (records: TaskRecord[]) => void;
  running: (id: string) => void;
  success: (id: string) => void;
  fail: (id: string, error: string) => void;
  cancelled: (id: string) => void;
  clearFinished: () => void;
}

/** Maintains the visible task list and mirrors safe metadata to SQLite without command output or secrets. */
export const useCommandTaskStore = create<CommandTaskState>((set, get) => {
  /** Persists one sanitized task snapshot; persistence failure must not hide the remote task result. */
  const persist = (task: CommandTask) => { void api.saveTask(toSaveInput(task)).catch(() => undefined); };
  /** Updates one task, then persists the resulting metadata snapshot. */
  const update = (id: string, updater: (task: CommandTask) => CommandTask) => {
    set((state) => ({ tasks: state.tasks.map((task) => task.id === id ? updater(task) : task) }));
    const task = get().tasks.find((value) => value.id === id);
    if (task) persist(task);
  };
  return {
    tasks: [],
    /** Registers a new remote task and immediately writes its initial state. */
    add: (task) => {
      const next = { ...task, startedAt: Date.now(), cancelSupported: task.cancelSupported ?? true, progress: task.progress ?? null, bytesTransferred: task.bytesTransferred ?? 0, totalBytes: task.totalBytes ?? null };
      set((state) => ({ tasks: [next, ...state.tasks.filter((current) => current.id !== task.id)] }));
      persist(next);
    },
    /** Restores persisted task metadata after the backend has marked stale running tasks interrupted. */
    hydrate: (records) => set((state) => ({ tasks: [...records.filter((record) => !state.tasks.some((task) => task.id === record.id)).map(fromRecord), ...state.tasks] })),
    /** Marks a task as executing after the remote channel starts producing work. */
    running: (id) => update(id, (task) => ({ ...task, status: "running" })),
    /** Marks a remote task successful and records its completion time. */
    success: (id) => update(id, (task) => ({ ...task, status: "success", finishedAt: Date.now() })),
    /** Marks a task failed with bounded UI-safe error text. */
    fail: (id, error) => update(id, (task) => ({ ...task, status: "failed", finishedAt: Date.now(), error: sanitizeError(error) })),
    /** Marks a task cancelled after the user stopped its remote channel. */
    cancelled: (id) => update(id, (task) => ({ ...task, status: "cancelled", finishedAt: Date.now() })),
    /** Removes terminal task records locally and in SQLite; only active rows remain visible. */
    clearFinished: () => { set((state) => ({ tasks: state.tasks.filter((task) => task.status === "queued" || task.status === "running") })); void api.clearFinishedTasks().catch(() => undefined); },
  };
});

/** Maps the persisted record to the task shape consumed by the existing task center. */
function fromRecord(record: TaskRecord): CommandTask {
  return {
    id: record.id,
    type: isTaskType(record.taskType) ? record.taskType : "docker-compose",
    serverId: record.serverId ?? "—",
    title: record.title,
    status: record.status,
    progress: record.progress,
    bytesTransferred: record.bytesTransferred,
    totalBytes: record.totalBytes,
    startedAt: Date.parse(record.startedAt),
    finishedAt: record.finishedAt ? Date.parse(record.finishedAt) : undefined,
    error: record.errorMessage ?? undefined,
    cancelSupported: record.cancelSupported,
    retryPayloadJson: record.retryPayloadJson ?? undefined,
  };
}

/** Converts one in-memory task to the non-sensitive SQLite input contract. */
function toSaveInput(task: CommandTask) {
  return {
    id: task.id,
    taskType: task.type,
    serverId: task.serverId || undefined,
    title: task.title.slice(0, 180),
    status: task.status,
    progress: task.progress,
    bytesTransferred: task.bytesTransferred,
    totalBytes: task.totalBytes,
    startedAt: new Date(task.startedAt).toISOString(),
    finishedAt: task.finishedAt ? new Date(task.finishedAt).toISOString() : null,
    errorMessage: task.error ? sanitizeError(task.error) : null,
    cancelSupported: task.cancelSupported,
    retryPayloadJson: task.retryPayloadJson ?? null,
  };
}

/** Restricts task type restoration to known UI labels instead of rendering arbitrary database text as a command type. */
function isTaskType(value: string): value is CommandTaskType {
  return ["tool-install", "database-install", "docker-pull", "docker-build", "docker-follow", "docker-compose", "ai-chat", "log-follow"].includes(value);
}

/** Removes line breaks and caps error text so remote stderr cannot flood local task metadata. */
function sanitizeError(value: string) {
  return value.replace(/[\r\n]+/g, " ").slice(0, 500);
}
