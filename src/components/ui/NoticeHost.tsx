import { CheckCircle2, CircleAlert, Info, X } from "lucide-react";
import { useNoticeStore } from "../../lib/noticeStore";

/** 渲染跨页面 toast，并提供键盘和鼠标关闭入口。 */
export function NoticeHost() {
  const notices = useNoticeStore((state) => state.notices);
  const dismiss = useNoticeStore((state) => state.dismiss);
  return <aside className="notice-host" aria-label="通知" aria-live="polite">{notices.map((notice) => <div className={`notice notice--${notice.kind}`} key={notice.id}><span>{notice.kind === "success" ? <CheckCircle2 size={15} /> : notice.kind === "error" ? <CircleAlert size={15} /> : <Info size={15} />}</span><p>{notice.message}</p><button aria-label="关闭通知" onClick={() => dismiss(notice.id)}><X size={14} /></button></div>)}</aside>;
}
