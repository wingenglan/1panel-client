import { describe, expect, it } from "vitest";
import { errorMessage, isAppError } from "./errors";

describe("structured application errors", () => {
  it("preserves the actionable code", () => {
    const error = { code: "SSH_AUTH_FAILED", category: "authentication", message: "SSH 认证失败", recoverable: true };
    expect(isAppError(error)).toBe(true);
    expect(errorMessage(error)).toBe("SSH 认证失败");
  });

  it("does not render object coercion noise", () => {
    expect(errorMessage({ unexpected: true })).toBe("发生未知错误");
  });
});
