import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { PhpInstallDialog, SslCertificateDialog } from "./dialogs";

afterEach(cleanup);
afterEach(() => vi.restoreAllMocks());

/** 为弹窗提供独立查询缓存，避免重试和跨用例缓存影响断言。 */
function queryWrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  /** 向测试中的弹窗注入本用例的查询客户端。 */
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  };
}

// 默认值或后台快照更新不能抹掉用户正在输入的证书申请信息。
it("preserves the certificate draft until the dialog is remounted", () => {
  const wrapper = queryWrapper();
  const props = { serverId: "server-a", open: true, onClose: vi.fn() };
  const view = render(<SslCertificateDialog {...props} defaults={{ domain: "first.example.com" }} />, { wrapper });
  fireEvent.change(screen.getByLabelText("域名"), { target: { value: "edited.example.com" } });
  fireEvent.change(screen.getByLabelText("邮箱"), { target: { value: "ops@example.com" } });
  view.rerender(<SslCertificateDialog {...props} defaults={{ domain: "refreshed.example.com" }} />);
  expect(screen.getByLabelText("域名")).toHaveValue("edited.example.com");
  expect(screen.getByLabelText("邮箱")).toHaveValue("ops@example.com");
  view.unmount();
  render(<SslCertificateDialog {...props} defaults={{ domain: "second.example.com" }} />, { wrapper });
  expect(screen.getByLabelText("域名")).toHaveValue("second.example.com");
  expect(screen.getByLabelText("邮箱")).toHaveValue("");
});

// 切换服务器后必须读取新节点的计划，不能复用上一节点的安装候选。
it("loads the PHP installation plan for the selected server", async () => {
  const load = vi.spyOn(api, "phpInstallPlan").mockImplementation(async (serverId) => ({
    packageManager: serverId === "server-a" ? "apt" : "dnf", packages: ["php-fpm"], services: ["php-fpm"], command: "preview", risk: "test plan",
  }));
  const props = { open: true, onClose: vi.fn() };
  const view = render(<PhpInstallDialog {...props} serverId="server-a" />, { wrapper: queryWrapper() });
  expect(await screen.findByText("apt")).toBeInTheDocument();
  view.rerender(<PhpInstallDialog {...props} serverId="server-b" />);
  expect(await screen.findByText("dnf")).toBeInTheDocument();
  expect(load).toHaveBeenLastCalledWith("server-b");
  expect(screen.queryByText("apt")).not.toBeInTheDocument();
});
