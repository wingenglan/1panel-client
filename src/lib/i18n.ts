export type Locale = "zh-CN" | "en-US";

const localeStorageKey = "1panel-client.locale";

/** 读取本地语言偏好，未知值统一回退到简体中文。 */
export function readLocale(): Locale {
  const value = localStorage.getItem(localeStorageKey);
  return value === "en-US" ? "en-US" : "zh-CN";
}

/** 保存语言偏好，供后续翻译资源按同一键加载。 */
export function saveLocale(locale: Locale) {
  localStorage.setItem(localeStorageKey, locale);
}

/** 更新文档语言属性，改善系统辅助功能和后续翻译资源识别。 */
export function applyLocale(locale: Locale) {
  document.documentElement.lang = locale;
}

/** 将连接状态转换为用户可读的中文文案，状态值本身只用于程序判断。 */
export function connectionStatusLabel(status: string | undefined) {
  switch (status) {
    case "online": return "已连接";
    case "connecting": return "连接中";
    case "error": return "连接异常";
    default: return "未连接";
  }
}

/** 将 Docker CLI 的英文状态行转换为中文，同时保留版本、时间等上下文。 */
export function dockerStatusLabel(status: string) {
  return status
    .replace(/\bUp\b/gi, "运行中")
    .replace(/\bExited\b/gi, "已退出")
    .replace(/\bCreated\b/gi, "已创建")
    .replace(/\bPaused\b/gi, "已暂停")
    .replace(/\bRestarting\b/gi, "重启中")
    .replace(/\bDead\b/gi, "异常");
}
