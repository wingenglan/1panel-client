import { create } from "zustand";

export type NoticeKind = "success" | "error" | "info";

export interface Notice {
  id: string;
  kind: NoticeKind;
  message: string;
}

interface NoticeState {
  notices: Notice[];
  push: (kind: NoticeKind, message: string) => void;
  dismiss: (id: string) => void;
}

/** 维护跨页面的本地提示消息；提示只保存短文本，不保存远端输出或凭据。 */
export const useNoticeStore = create<NoticeState>((set, get) => ({
  notices: [],
  /** 添加一个会自动消失的本地提示。 */
  push: (kind, message) => {
    const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
    set((state) => ({ notices: [...state.notices, { id, kind, message }].slice(-4) }));
    window.setTimeout(() => get().dismiss(id), kind === "error" ? 7000 : 4500);
  },
  /** 从提示列表中移除指定消息。 */
  dismiss: (id) => set((state) => ({ notices: state.notices.filter((notice) => notice.id !== id) })),
}));

/** 从非 React 代码推送本地提示，供 mutation 或 IPC 回调复用。 */
export function pushNotice(kind: NoticeKind, message: string) {
  useNoticeStore.getState().push(kind, message);
}
