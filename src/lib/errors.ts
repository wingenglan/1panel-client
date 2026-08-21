import type { AppError } from "../types/server";

const errorCodeLabels: Record<string, string> = {
  SSH_NOT_CONNECTED: "未连接服务器",
  SSH_CONNECTION_LOST: "SSH 连接已断开",
  SSH_CONNECT_IN_PROGRESS: "正在连接服务器",
  NETWORK_TIMEOUT: "网络连接超时",
  SSH_AUTH_FAILED: "SSH 身份验证失败",
  HOST_KEY_CHANGED: "服务器身份指纹已变化",
  HOST_KEY_UNKNOWN: "服务器身份尚未确认",
  SFTP_FAILED: "文件通道操作失败",
  TERMINAL_NOT_FOUND: "终端会话已结束",
  REMOTE_COMMAND_FAILED: "远程命令执行失败",
  COMMAND_TIMEOUT: "远程命令超时",
  PERMISSION_DENIED: "权限不足",
  CANCELLED: "操作已取消",
};

export function isAppError(value: unknown): value is AppError {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<AppError>;
  return typeof candidate.code === "string" && typeof candidate.message === "string";
}

/** 将后端结构化错误转换为不暴露内部代码的中文提示。 */
export function errorMessage(value: unknown): string {
  if (isAppError(value)) return value.message || errorCodeLabels[value.code] || "操作失败";
  if (value instanceof Error) return value.message;
  return typeof value === "string" ? value : "发生未知错误";
}

/** 返回适合日志或诊断面板展示的中文错误标题，内部代码不进入普通界面。 */
export function errorLabel(code: string) {
  return errorCodeLabels[code] ?? "操作失败";
}
