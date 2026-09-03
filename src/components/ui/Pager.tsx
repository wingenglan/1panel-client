import { ChevronLeft, ChevronRight } from "lucide-react";
import { useState } from "react";

const PAGE_SIZES = [30, 60, 90];

/** 生成当前页附近的页码窗口，较长的间隔以省略号表示。 */
function pageWindow(page: number, pageCount: number): (number | "…")[] {
  if (pageCount <= 7) return Array.from({ length: pageCount }, (_, i) => i + 1);
  const pages: (number | "…")[] = [1];
  const start = Math.max(2, page - 1);
  const end = Math.min(pageCount - 1, page + 1);
  if (start > 2) pages.push("…");
  for (let p = start; p <= end; p++) pages.push(p);
  if (end < pageCount - 1) pages.push("…");
  pages.push(pageCount);
  return pages;
}

/** 与 Web 端 el-pagination 对齐的分页器：总数/每页条数/页码/前往。
 *  Web 面板的列表页空数据时仍渲染「共 0 条」分页条（如网站-运行环境），
 *  传入 showEmpty 后 total=0 也渲染；外部页码变化时同步跳转草稿。 */
export function Pager({ total, page, pageSize, pageSizes = PAGE_SIZES, showEmpty = false, onPageChange, onPageSizeChange }: {
  total: number;
  page: number;
  pageSize: number;
  pageSizes?: number[];
  showEmpty?: boolean;
  onPageChange: (page: number) => void;
  onPageSizeChange: (size: number) => void;
}) {
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  const [jump, setJump] = useState(String(page));
  const [lastPage, setLastPage] = useState(page);
  if (lastPage !== page) {
    setLastPage(page);
    setJump(String(page));
  }
  /** 将跳转输入限制为有效整数页码，再通知父组件。 */
  const go = () => {
    const value = Number(jump);
    if (Number.isFinite(value)) onPageChange(Math.min(pageCount, Math.max(1, Math.trunc(value))));
  };
  if (!total && !showEmpty) return null;
  return <div className="pager">
    <span className="pager__total">共 {total} 条</span>
    <select className="pager__size" value={pageSize} onChange={(event) => onPageSizeChange(Number(event.target.value))}>
      {pageSizes.map((size) => <option key={size} value={size}>{size}条/页</option>)}
    </select>
    <button className="pager__nav" disabled={page <= 1} onClick={() => onPageChange(page - 1)} aria-label="上一页"><ChevronLeft size={14} /></button>
    <div className="pager__pages">
      {pageWindow(page, pageCount).map((item, index) => item === "…"
        ? <span className="pager__ellipsis" key={`ellipsis-${index}`}>…</span>
        : <button className={item === page ? "is-active" : ""} key={item} onClick={() => onPageChange(item)}>{item}</button>)}
    </div>
    <button className="pager__nav" disabled={page >= pageCount} onClick={() => onPageChange(page + 1)} aria-label="下一页"><ChevronRight size={14} /></button>
    <span className="pager__jump">前往 <input className="pager__jump-input" type="number" min={1} max={pageCount} value={jump} onChange={(event) => setJump(event.target.value)} onKeyDown={(event) => event.key === "Enter" && go()} onBlur={go} /> 页</span>
  </div>;
}
