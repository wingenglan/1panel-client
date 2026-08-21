import { create } from "zustand";
import type { TransferEvent } from "../../lib/api";

export interface TransferTask {
  id: string;
  label: string;
  direction: "upload" | "download";
  status: "queued" | "running" | "success" | "failed" | "cancelled";
  transferredBytes: number;
  totalBytes: number | null;
  bytesPerSecond: number;
  currentPath: string;
  error?: string;
  retry?: () => void;
}

interface TransferState {
  tasks: TransferTask[];
  add: (task: TransferTask) => void;
  event: (event: TransferEvent) => void;
  fail: (id: string, error: string) => void;
  clearFinished: () => void;
}

export const useTransferStore = create<TransferState>((set) => ({
  tasks: [],
  add: (task) => set((state) => ({ tasks: [task, ...state.tasks] })),
  event: (event) => set((state) => ({
    tasks: state.tasks.map((task) => {
      if (task.id !== event.data.transferId) return task;
      if (event.event === "started") return { ...task, status: "running", totalBytes: event.data.totalBytes };
      if (event.event === "progress") return { ...task, status: "running", transferredBytes: event.data.transferredBytes, totalBytes: event.data.totalBytes, bytesPerSecond: event.data.bytesPerSecond, currentPath: event.data.currentPath };
      if (event.event === "completed") return { ...task, status: "success", transferredBytes: event.data.transferredBytes, bytesPerSecond: 0 };
      return { ...task, status: "cancelled", transferredBytes: event.data.transferredBytes, bytesPerSecond: 0 };
    }),
  })),
  fail: (id, error) => set((state) => ({ tasks: state.tasks.map((task) => task.id === id ? { ...task, status: "failed", error } : task) })),
  clearFinished: () => set((state) => ({ tasks: state.tasks.filter((task) => task.status === "queued" || task.status === "running") })),
}));
