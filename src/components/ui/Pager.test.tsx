import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Pager } from "./Pager";

afterEach(cleanup);

// 外部翻页必须清除尚未提交的跳转草稿，并在下一次输入时沿用新页码。
it("synchronizes the jump input when the controlled page changes", () => {
  const props = { total: 200, pageSize: 20, onPageChange: vi.fn(), onPageSizeChange: vi.fn() };
  const view = render(<Pager {...props} page={1} />);
  fireEvent.change(screen.getByRole("spinbutton"), { target: { value: "8" } });
  view.rerender(<Pager {...props} page={3} />);
  expect(screen.getByRole("spinbutton")).toHaveValue(3);
  fireEvent.blur(screen.getByRole("spinbutton"));
  expect(props.onPageChange).toHaveBeenLastCalledWith(3);
});

// 跳转和空列表边界应保持受控，不能产生零页或超出末页的回调。
it("bounds jump requests and disables empty-list navigation", () => {
  const props = { total: 95, pageSize: 20, page: 1, onPageChange: vi.fn(), onPageSizeChange: vi.fn() };
  const view = render(<Pager {...props} />);
  fireEvent.change(screen.getByRole("spinbutton"), { target: { value: "99" } });
  fireEvent.keyDown(screen.getByRole("spinbutton"), { key: "Enter" });
  expect(props.onPageChange).toHaveBeenLastCalledWith(5);
  view.rerender(<Pager {...props} total={0} showEmpty />);
  expect(screen.getByRole("button", { name: "上一页" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "下一页" })).toBeDisabled();
});
