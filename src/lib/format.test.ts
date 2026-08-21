import { describe, expect, it } from "vitest";
import { formatBytes, formatDuration } from "./format";

describe("human readable formatters", () => {
  it("formats byte values with binary units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1024)).toBe("1 KB");
    expect(formatBytes(5 * 1024 ** 3)).toBe("5.0 GB");
  });

  it("formats uptime without inventing precision", () => {
    expect(formatDuration(2 * 86400 + 3 * 3600)).toBe("2 天 3 小时");
    expect(formatDuration(90 * 60)).toBe("1 小时 30 分钟");
  });
});
