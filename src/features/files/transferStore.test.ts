import { beforeEach, describe, expect, it } from "vitest";
import { useTransferStore } from "./transferStore";

const task = {
  id: "transfer-1",
  label: "archive.tar",
  direction: "upload" as const,
  status: "queued" as const,
  transferredBytes: 0,
  totalBytes: null,
  bytesPerSecond: 0,
  currentPath: "/tmp",
};

describe("transfer store", () => {
  beforeEach(() => useTransferStore.setState({ tasks: [] }));

  it("tracks streamed progress through completion", () => {
    useTransferStore.getState().add(task);
    useTransferStore.getState().event({
      event: "progress",
      data: {
        transferId: task.id,
        transferredBytes: 512,
        totalBytes: 1024,
        bytesPerSecond: 256,
        currentPath: "/tmp/archive.tar",
      },
    });
    expect(useTransferStore.getState().tasks[0]).toMatchObject({
      status: "running",
      transferredBytes: 512,
      totalBytes: 1024,
    });
    useTransferStore.getState().event({
      event: "completed",
      data: { transferId: task.id, transferredBytes: 1024 },
    });
    expect(useTransferStore.getState().tasks[0].status).toBe("success");
  });

  it("preserves failed task details until explicitly cleared", () => {
    useTransferStore.getState().add(task);
    useTransferStore.getState().fail(task.id, "permission denied");
    expect(useTransferStore.getState().tasks[0]).toMatchObject({
      status: "failed",
      error: "permission denied",
    });
    useTransferStore.getState().clearFinished();
    expect(useTransferStore.getState().tasks).toHaveLength(0);
  });
});
