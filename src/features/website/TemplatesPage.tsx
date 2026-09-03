import * as Dialog from "@radix-ui/react-dialog";
import { ArrowLeft, ChevronDown, Plus, RefreshCw, Search, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "../../components/ui/Button";
import { Pager } from "../../components/ui/Pager";
import { useNoticeStore } from "../../lib/noticeStore";

type VariableType = "text" | "textarea" | "number" | "select" | "color";

type TemplateVariable = { key: string; label: string; type: VariableType; default: string; options: string; required: boolean };

type SiteTemplate = { id: string; name: string; type: "single" | "multi"; content: string; filePath: string; variables: TemplateVariable[]; remark: string; createdAt: string };

type TemplateProduct = { id: string; name: string; templateName: string; templateType: "single" | "multi"; outputPath: string; content: string; createdAt: string };

const TEMPLATES_KEY = "1panel-client.website-templates";
const PRODUCTS_KEY = "1panel-client.website-template-products";

const CONTENT_PLACEHOLDER = "<html><body><h1>{{title}}</h1></body></html>";

const VARIABLE_TYPES: { value: VariableType; label: string }[] = [
  { value: "text", label: "单行文本" },
  { value: "textarea", label: "多行文本" },
  { value: "number", label: "数字" },
  { value: "select", label: "下拉选择" },
  { value: "color", label: "颜色" },
];

function readList<T>(key: string): T[] {
  try { const parsed = JSON.parse(localStorage.getItem(key) ?? "[]") as unknown; return Array.isArray(parsed) ? parsed as T[] : []; } catch { return []; }
}

/** 兼容旧版本地数据：变量旧格式 { label, defaultValue } -> { key, label, type, default, options, required }。 */
function migrateTemplate(item: Record<string, unknown>): SiteTemplate {
  const variables = Array.isArray(item.variables)
    ? item.variables.map((value: unknown): TemplateVariable => {
      const v = value && typeof value === "object" ? value as Record<string, unknown> : {};
      return typeof v.key === "string"
        ? { key: v.key, label: String(v.label ?? ""), type: VARIABLE_TYPES.find((type) => type.value === v.type)?.value ?? "text", default: String(v.default ?? ""), options: String(v.options ?? ""), required: !!v.required }
        : { key: String(v.label ?? ""), label: "", type: "text", default: String(v.defaultValue ?? ""), options: "", required: false };
    })
    : [];
  return { id: String(item.id ?? ""), name: String(item.name ?? ""), type: item.type === "multi" ? "multi" : "single", content: String(item.content ?? ""), filePath: String(item.filePath ?? ""), variables, remark: String(item.remark ?? ""), createdAt: String(item.createdAt ?? "") };
}

/** 将旧版本地产物字段转换为当前类型，并补齐缺失字段。 */
function migrateProduct(item: Record<string, unknown>): TemplateProduct {
  return { id: String(item.id ?? ""), name: String(item.name ?? item.templateName ?? ""), templateName: String(item.templateName ?? ""), templateType: item.templateType === "multi" ? "multi" : "single", outputPath: String(item.outputPath ?? ""), content: String(item.content ?? ""), createdAt: String(item.createdAt ?? "") };
}

function pad(value: number): string { return value < 10 ? `0${value}` : String(value); }

/** 与 Web 端 dateFormat（YYYY-MM-DD HH:mm:ss）一致。 */
function formatTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

function detectKeys(content: string): string[] {
  const keys: string[] = [];
  for (const match of content.matchAll(/\{\{(\w+)\}\}/g)) {
    if (!keys.includes(match[1])) keys.push(match[1]);
  }
  return keys;
}

const newVariable = (key = ""): TemplateVariable => ({ key, label: "", type: "text", default: "", options: "", required: false });

/** web 版 网站-模板 页：模板 CRUD（右滑抽屉）+ 变量自动识别 + 生成产物（预览）+ 产物列表；
 *  客户端模板保存在本地，行为对齐 Web 面板（Drawer/el-table/el-pagination 等效布局）。 */
export function TemplatesPage() {
  const [templates, setTemplates] = useState<SiteTemplate[]>(() => readList<Record<string, unknown>>(TEMPLATES_KEY).map(migrateTemplate));
  const [products, setProducts] = useState<TemplateProduct[]>(() => readList<Record<string, unknown>>(PRODUCTS_KEY).map(migrateProduct));
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [editOpen, setEditOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<SiteTemplate | null>(null);
  const [generateOpen, setGenerateOpen] = useState(false);
  const [genTemplateId, setGenTemplateId] = useState<string>("");
  const [genName, setGenName] = useState("");
  const [genValues, setGenValues] = useState<Record<string, string>>({});
  const [outputsOpen, setOutputsOpen] = useState(false);
  const [outputPage, setOutputPage] = useState(1);
  const [outputPageSize, setOutputPageSize] = useState(20);
  const [confirm, setConfirm] = useState<{ title: string; message: string; run: () => void } | null>(null);
  const pushNotice = useNoticeStore((state) => state.push);

  useEffect(() => localStorage.setItem(TEMPLATES_KEY, JSON.stringify(templates)), [templates]);
  useEffect(() => localStorage.setItem(PRODUCTS_KEY, JSON.stringify(products)), [products]);

  const rows = useMemo(() => {
    const keyword = query.trim().toLocaleLowerCase();
    const filtered = templates.filter((template) => !keyword || template.name.toLocaleLowerCase().includes(keyword) || template.remark.toLocaleLowerCase().includes(keyword));
    return filtered.slice((page - 1) * pageSize, page * pageSize);
  }, [templates, query, page, pageSize]);

  const filteredCount = useMemo(() => {
    const keyword = query.trim().toLocaleLowerCase();
    return keyword ? templates.filter((template) => template.name.toLocaleLowerCase().includes(keyword) || template.remark.toLocaleLowerCase().includes(keyword)).length : templates.length;
  }, [templates, query]);

  const genTemplate = useMemo(() => templates.find((template) => template.id === genTemplateId) ?? null, [templates, genTemplateId]);

  const genVariables = useMemo(() => {
    if (!genTemplate) return [];
    const { content } = genTemplate;
    const vars = [...genTemplate.variables];
    detectKeys(content).forEach((key) => { if (!vars.some((variable) => variable.key === key)) vars.push(newVariable(key)); });
    return vars;
  }, [genTemplate]);

  const previewHTML = useMemo(() => {
    if (!genTemplate || genTemplate.type !== "single") return "";
    let html = genTemplate.content;
    for (const variable of genVariables) {
      html = html.replace(new RegExp(`\\{\\{\\s*${variable.key}\\s*\\}\\}`, "g"), genValues[variable.key] || "");
    }
    return html.replace(/\{\{\w+\}\}/g, "");
  }, [genTemplate, genVariables, genValues]);

  const openCreate = () => {
    setEditing(false);
    setDraft({ id: `t-${Date.now()}`, name: "", type: "single", content: "", filePath: "", variables: [], remark: "", createdAt: new Date().toISOString() });
    setEditOpen(true);
  };
  const openEdit = (template: SiteTemplate) => {
    setEditing(true);
    setDraft({ ...template, variables: template.variables.map((variable) => ({ ...variable })) });
    setEditOpen(true);
  };
  const submitDraft = () => {
    if (!draft || !draft.name.trim()) return;
    const variables = draft.variables.filter((variable) => variable.key.trim());
    setTemplates((current) => editing ? current.map((item) => item.id === draft.id ? { ...draft, variables } : item) : [...current, { ...draft, variables }]);
    setEditOpen(false);
    setDraft(null);
    pushNotice("success", editing ? "编辑成功" : "创建成功");
  };
  const updateDraftVariable = (index: number, patch: Partial<TemplateVariable>) => {
    if (!draft) return;
    setDraft({ ...draft, variables: draft.variables.map((variable, i) => i === index ? { ...variable, ...patch } : variable) });
  };
  const detectDraftVariables = () => {
    if (!draft) return;
    const keys = detectKeys(draft.content);
    setDraft((current) => !current ? current : { ...current, variables: [...current.variables, ...keys.filter((key) => !current.variables.some((variable) => variable.key === key)).map((key) => newVariable(key))] });
  };

  const openGenerate = (templateId?: string) => {
    setGenTemplateId(templateId ?? "");
    setGenName("");
    setGenValues({});
    setGenerateOpen(true);
  };
  const applyGenTemplate = (templateId: string) => {
    setGenTemplateId(templateId);
    const template = templates.find((item) => item.id === templateId);
    if (!template) return;
    const values: Record<string, string> = {};
    detectKeys(template.content).forEach((key) => { values[key] = ""; });
    template.variables.forEach((variable) => { values[variable.key] = variable.default; });
    setGenValues(values);
  };
  const submitGenerate = () => {
    if (!genTemplate || !genName.trim()) return;
    const outputPath = `${genTemplate.name}/${genName.trim()}.html`;
    setProducts((current) => [{ id: `p-${Date.now()}`, name: genName.trim(), templateName: genTemplate.name, templateType: genTemplate.type, outputPath, content: previewHTML, createdAt: new Date().toISOString() }, ...current]);
    setGenerateOpen(false);
    setGenTemplateId("");
    setGenName("");
    setGenValues({});
    setOutputPage(1);
    pushNotice("success", "产物生成成功");
  };
  const deleteTemplateRow = (template: SiteTemplate) => setConfirm({ title: "删除", message: `确定删除此模板吗？关联的产物也将被删除 - ${template.name}`, run: () => { setTemplates((current) => current.filter((item) => item.id !== template.id)); pushNotice("success", "删除成功"); } });
  const deleteProductRow = (product: TemplateProduct) => setConfirm({ title: "删除", message: `确定删除此产物吗？ - ${product.name}`, run: () => { setProducts((current) => current.filter((item) => item.id !== product.id)); pushNotice("success", "删除成功"); } });

  const pagedProducts = products.slice((outputPage - 1) * outputPageSize, outputPage * outputPageSize);

  return <section className="website-page">
    <div className="web-toolbar">
      <div className="web-toolbar__left">
        <Button variant="primary" onClick={openCreate}><Plus size={14} /> 创建模板</Button>
        <Button variant="secondary" onClick={() => { setOutputsOpen(true); setOutputPage(1); }}>产物列表</Button>
      </div>
      <div className="web-toolbar__right">
        <label className="web-search"><input value={query} onChange={(event) => { setQuery(event.target.value); setPage(1); }} placeholder="搜索" /><Search size={14} /></label>
        <button className="icon-control" onClick={() => setTemplates((current) => [...current])} title="刷新"><RefreshCw size={15} /></button>
      </div>
    </div>

    <div className="web-table-wrap">
      <div className="web-table">
        <div className="ops-head tmpl-grid"><span>模板名称</span><span>模板类型</span><span>变量定义</span><span>备注</span><span>时间</span><span className="web-ops-cell">操作</span></div>
        {rows.length > 0 ? rows.map((template) => <div className="ops-row tmpl-grid" key={template.id}>
          <span className="web-kind" title={template.name}>{template.name}</span>
          <span><span className="web-ssl-tag is-ok">{template.type === "single" ? "单文件" : "多文件(zip)"}</span></span>
          <span className="web-muted">{template.variables.length}</span>
          <span className="web-muted" title={template.remark}>{template.remark}</span>
          <span className="web-muted">{formatTime(template.createdAt)}</span>
          <div className="web-ops-cell">
            <button className="web-text-btn" onClick={() => openEdit(template)}>编辑</button>
            <button className="web-text-btn" onClick={() => openGenerate(template.id)}>生成产物</button>
            <button className="web-text-btn" onClick={() => deleteTemplateRow(template)}>删除</button>
          </div>
        </div>) : <div className="web-table-empty">暂无数据</div>}
      </div>
      <div className="web-table-pager"><Pager total={filteredCount} page={page} pageSize={pageSize} pageSizes={[20, 50, 100]} showEmpty onPageChange={setPage} onPageSizeChange={(size) => { setPageSize(size); setPage(1); }} /></div>
    </div>

    {/* 创建 / 编辑模板：右侧抽屉 */}
    {editOpen && draft && <div className="web-drawer-backdrop" onMouseDown={() => setEditOpen(false)}>
      <section className="web-drawer" onMouseDown={(event) => event.stopPropagation()}>
        <header className="web-drawer__header">
          <button type="button" className="web-drawer__back" onClick={() => setEditOpen(false)} title="返回"><ArrowLeft size={17} /></button>
          <h2>{editing ? "编辑模板" : "创建模板"}</h2>
          <button type="button" className="web-drawer__close" onClick={() => setEditOpen(false)} title="关闭此对话框"><X size={17} /></button>
        </header>
        <div className="web-drawer__body">
          <div className="tmpl-field"><label>模板名称 <span className="req">*</span></label><input className="web-input" value={draft?.name ?? ""} onChange={(event) => setDraft((current) => current ? { ...current, name: event.target.value } : current)} /></div>
          <div className="tmpl-field"><label>模板类型 <span className="req">*</span></label>
            <div className="web-radio-group">
              <label className="web-radio"><input type="radio" name="tmpl-type" value="single" checked={draft?.type === "single"} disabled={editing} onChange={() => setDraft((current) => current ? { ...current, type: "single" } : current)} /><span>单文件</span></label>
              <label className="web-radio"><input type="radio" name="tmpl-type" value="multi" checked={draft?.type === "multi"} disabled={editing} onChange={() => setDraft((current) => current ? { ...current, type: "multi" } : current)} /><span>多文件(zip)</span></label>
            </div>
          </div>
          {draft?.type === "single"
            ? <div className="tmpl-field"><label>模板内容 <span className="req">*</span></label>
              <textarea className="web-input tmpl-content" rows={12} placeholder={CONTENT_PLACEHOLDER} value={draft.content} onChange={(event) => setDraft({ ...draft, content: event.target.value })} onBlur={detectDraftVariables} />
              <span className="input-help">自动识别内容中双花括号包裹的变量并加入变量定义表格</span>
            </div>
            : <div className="tmpl-field"><label>上传 zip <span className="req">*</span></label>
              <label className="web-upload-btn"><input type="file" accept=".zip" onChange={(event) => { const file = event.target.files?.[0]; if (file) setDraft((current) => current ? { ...current, filePath: file.name } : current); }} /><Plus size={13} /> 上传 zip</label>
              {draft?.filePath && <span className="web-upload-info">{draft.filePath}</span>}
              <span className="input-help">自动识别内容中双花括号包裹的变量并加入变量定义表格</span>
            </div>}
          <div className="tmpl-field"><label>备注</label><input className="web-input" value={draft?.remark ?? ""} onChange={(event) => setDraft((current) => current ? { ...current, remark: event.target.value } : current)} /></div>
          <div className="tmpl-field"><label>变量定义</label>
            <Button size="sm" variant="secondary" onClick={() => setDraft((current) => current ? { ...current, variables: [...current.variables, newVariable()] } : current)}><Plus size={12} /> 添加</Button>
            {draft && draft.variables.length > 0 && <table className="web-vars-table">
              <thead><tr><th>变量名</th><th>标签</th><th>类型</th><th>默认值</th><th>选项(逗号分隔)</th><th>必填</th><th className="web-ops-cell">操作</th></tr></thead>
              <tbody>{draft.variables.map((variable, index) => <tr key={index}>
                <td><input className="web-input" value={variable.key} placeholder="" onChange={(event) => updateDraftVariable(index, { key: event.target.value })} /></td>
                <td><input className="web-input" value={variable.label} onChange={(event) => updateDraftVariable(index, { label: event.target.value })} /></td>
                <td><select className="web-input" value={variable.type} onChange={(event) => updateDraftVariable(index, { type: event.target.value as VariableType })}>{VARIABLE_TYPES.map((item) => <option key={item.value} value={item.value}>{item.label}</option>)}</select></td>
                <td><input className="web-input" value={variable.default} onChange={(event) => updateDraftVariable(index, { default: event.target.value })} /></td>
                <td><input className="web-input" value={variable.options} disabled={variable.type !== "select"} onChange={(event) => updateDraftVariable(index, { options: event.target.value })} /></td>
                <td><label className="web-switch"><input type="checkbox" checked={variable.required} onChange={(event) => updateDraftVariable(index, { required: event.target.checked })} /><i /></label></td>
                <td className="web-ops-cell"><button className="web-text-btn is-danger" onClick={() => setDraft((current) => current ? { ...current, variables: current.variables.filter((_, i) => i !== index) } : current)}>删除</button></td>
              </tr>)}</tbody>
            </table>}
          </div>
        </div>
        <footer className="web-drawer__footer">
          <Button variant="ghost" onClick={() => setEditOpen(false)}>取消</Button>
          <Button variant="primary" onClick={submitDraft} disabled={!draft?.name.trim() || (draft?.type === "multi" && !draft.filePath)}>保存</Button>
        </footer>
      </section>
    </div>}

    {/* 生成产物：右侧抽屉 */}
    {generateOpen && <div className="web-drawer-backdrop" onMouseDown={() => setGenerateOpen(false)}>
      <section className="web-drawer" onMouseDown={(event) => event.stopPropagation()}>
        <header className="web-drawer__header">
          <button type="button" className="web-drawer__back" onClick={() => setGenerateOpen(false)} title="返回"><ArrowLeft size={17} /></button>
          <h2>生成产物</h2>
          <button type="button" className="web-drawer__close" onClick={() => setGenerateOpen(false)} title="关闭此对话框"><X size={17} /></button>
        </header>
        <div className="web-drawer__body">
          <div className="tmpl-field"><label>选择模板 <span className="req">*</span></label>
            <div className="web-select-wrap"><select className="web-input" value={genTemplateId} onChange={(event) => applyGenTemplate(event.target.value)}>{templates.map((template) => <option key={template.id} value={template.id}>{template.name}</option>)}</select><ChevronDown size={14} className="web-select-caret" /></div>
          </div>
          <div className="tmpl-field"><label>产物名称 <span className="req">*</span></label><input className="web-input" value={genName} onChange={(event) => setGenName(event.target.value)} /></div>
          {genTemplate && <div className="tmpl-gap">
            <div className="web-divider"><span>填写变量</span></div>
            {genVariables.length === 0
              ? <span className="input-help">此模板没有定义变量</span>
              : genVariables.map((variable) => <div className="tmpl-field" key={variable.key}><label>{variable.label || variable.key}{variable.required && <span className="req"> *</span>}</label>
                {variable.type === "textarea"
                  ? <textarea className="web-input" rows={4} value={genValues[variable.key] ?? ""} onChange={(event) => setGenValues((current) => ({ ...current, [variable.key]: event.target.value }))} />
                  : variable.type === "number"
                    ? <input className="web-input" type="number" value={genValues[variable.key] ?? ""} onChange={(event) => setGenValues((current) => ({ ...current, [variable.key]: event.target.value }))} />
                    : variable.type === "select"
                      ? <select className="web-input" value={genValues[variable.key] ?? ""} onChange={(event) => setGenValues((current) => ({ ...current, [variable.key]: event.target.value }))}>{variable.options.split(",").map((option) => option.trim()).filter(Boolean).map((option) => <option key={option} value={option}>{option}</option>)}</select>
                      : variable.type === "color"
                        ? <div className="web-color-row"><input type="color" value={genValues[variable.key] || "#000000"} onChange={(event) => setGenValues((current) => ({ ...current, [variable.key]: event.target.value }))} /><span className="web-color-hex mono">{genValues[variable.key] || "#000000"}</span></div>
                        : <input className="web-input" value={genValues[variable.key] ?? ""} onChange={(event) => setGenValues((current) => ({ ...current, [variable.key]: event.target.value }))} />}
              </div>)}
            <div className="web-divider"><span>预览</span></div>
            <div className="preview-box">{previewHTML ? <iframe className="preview-frame" sandbox="allow-same-origin" srcDoc={previewHTML} title="模板预览" /> : <div className="preview-empty">选择模板后可实时预览</div>}</div>
          </div>}
        </div>
        <footer className="web-drawer__footer">
          <Button variant="ghost" onClick={() => setGenerateOpen(false)}>取消</Button>
          <Button variant="primary" onClick={submitGenerate} disabled={!genTemplate || !genName.trim()}>生成产物</Button>
        </footer>
      </section>
    </div>}

    {/* 产物列表：右侧抽屉 */}
    {outputsOpen && <div className="web-drawer-backdrop" onMouseDown={() => setOutputsOpen(false)}>
      <section className="web-drawer web-drawer--wide" onMouseDown={(event) => event.stopPropagation()}>
        <header className="web-drawer__header">
          <button type="button" className="web-drawer__back" onClick={() => setOutputsOpen(false)} title="返回"><ArrowLeft size={17} /></button>
          <h2>产物列表</h2>
          <span className="web-drawer__extra"><Button size="sm" variant="secondary" onClick={() => openGenerate()}>生成产物</Button><button type="button" className="web-drawer__close" onClick={() => setOutputsOpen(false)} title="关闭此对话框"><X size={17} /></button></span>
        </header>
        <div className="web-drawer__body web-drawer__body--flush">
          <div className="web-table">
            <div className="ops-head out-grid"><span>产物名称</span><span>模板名称</span><span>模板类型</span><span>文件路径</span><span>时间</span><span className="web-ops-cell">操作</span></div>
            {pagedProducts.length > 0 ? pagedProducts.map((product) => <div className="ops-row out-grid" key={product.id}>
              <span className="web-kind" title={product.name}>{product.name}</span>
              <span title={product.templateName}>{product.templateName}</span>
              <span><span className="web-ssl-tag is-ok">{product.templateType === "single" ? "单文件" : "多文件(zip)"}</span></span>
              <span className="web-muted mono" title={product.outputPath}>{product.outputPath}</span>
              <span className="web-muted">{formatTime(product.createdAt)}</span>
              <div className="web-ops-cell"><button className="web-text-btn" onClick={() => deleteProductRow(product)}>删除</button></div>
            </div>) : <div className="web-table-empty">暂无数据</div>}
          </div>
          <div className="web-table-pager"><Pager total={products.length} page={outputPage} pageSize={outputPageSize} pageSizes={[20, 50, 100]} showEmpty onPageChange={setOutputPage} onPageSizeChange={(size) => { setOutputPageSize(size); setOutputPage(1); }} /></div>
        </div>
      </section>
    </div>}

    <Dialog.Root open={!!confirm} onOpenChange={(open) => !open && setConfirm(null)}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="dialog-content dialog-content--narrow confirm-dialog">
          <Dialog.Title>{confirm?.title}</Dialog.Title>
          <Dialog.Description>{confirm?.message}</Dialog.Description>
          <div className="dialog-actions">
            <Button variant="ghost" onClick={() => setConfirm(null)}>取消</Button>
            <Button variant="danger" onClick={() => { const run = confirm?.run; setConfirm(null); run?.(); }}>确认删除</Button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  </section>;
}
