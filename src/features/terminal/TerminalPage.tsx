import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Terminal } from "@xterm/xterm";
import { useQuery } from "@tanstack/react-query";
import { ArrowDown, Eraser, EyeOff, History as HistoryIcon, Minus, Pencil, Plus, Power, RotateCw, Search, Sparkles, TerminalSquare, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { NavLink, useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { api, type TerminalEvent } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { materializeShortcut, matchShortcuts, shortcutVariables } from "./shortcutMatcher";
import { ShortcutManager } from "./ShortcutManager";
import type { ShortcutRecord } from "../../types/server";

interface TerminalTab {
  key: string;
  title: string;
  revision: number;
  shortcutsEnabled: boolean;
  shortcutToggleVersion: number;
}

/** Creates a fresh terminal tab with shortcut completion enabled by default. */
function newTab(index: number): TerminalTab {
  return { key: crypto.randomUUID(), title: `Shell ${index}`, revision: 0, shortcutsEnabled: true, shortcutToggleVersion: 0 };
}

interface TerminalHistoryItem {
  id: string;
  command: string;
  line: number;
  at: number;
}

/** Formats a shell history timestamp for the compact history panel. */
function formatHistoryTime(timestamp: number): string {
  return new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" }).format(timestamp);
}

function decodeBase64(value: string): Uint8Array {
  const binary = atob(value);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

/** 管理基于 SSH 的多标签交互终端，并同步远端终端事件。 */
export function TerminalPage() {
  const { serverId = "" } = useParams();
  const profile = useQuery({ queryKey: ["server", serverId], queryFn: () => api.getServer(serverId) });
  const connection = useQuery({ queryKey: ["connection", serverId], queryFn: () => api.connectionState(serverId), refetchInterval: 3000 });
  const shortcuts = useQuery({ queryKey: ["shortcuts", serverId], queryFn: () => api.listShortcuts(serverId), enabled: !!serverId });
  const [tabs, setTabs] = useState<TerminalTab[]>(() => [newTab(1)]);
  const [activeKey, setActiveKey] = useState(() => tabs[0].key);
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [fontSize, setFontSize] = useState(13);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchRequest, setSearchRequest] = useState(0);
  const [clearRequest, setClearRequest] = useState(0);
  const [historyOpenKey, setHistoryOpenKey] = useState<string | null>(null);
  const [shortcutManagerOpen, setShortcutManagerOpen] = useState(false);

  const addTab = () => {
    const tab = newTab(tabs.length + 1);
    setTabs((current) => [...current, tab]);
    setActiveKey(tab.key);
  };
  const closeTab = (key: string) => {
    setTabs((current) => {
      const index = current.findIndex((tab) => tab.key === key);
      const next = current.filter((tab) => tab.key !== key);
      if (!next.length) {
        const replacement = newTab(1);
        setActiveKey(replacement.key);
        return [replacement];
      }
      if (activeKey === key) setActiveKey(next[Math.min(index, next.length - 1)].key);
      return next;
    });
  };
  const renameTab = (key: string, title: string) => {
    const value = title.trim();
    if (value) setTabs((current) => current.map((tab) => tab.key === key ? { ...tab, title: value } : tab));
    setEditingKey(null);
  };
  const reopenTab = (key: string) => {
    setTabs((current) => current.map((tab) => tab.key === key ? { ...tab, revision: tab.revision + 1 } : tab));
  };
  /** Toggles completion suggestions for one shell without affecting its sibling shells. */
  const toggleShortcuts = (key: string) => {
    setTabs((current) => current.map((tab) => tab.key === key ? { ...tab, shortcutsEnabled: !tab.shortcutsEnabled, shortcutToggleVersion: tab.shortcutToggleVersion + 1 } : tab));
  };

  return <section className="terminal-page">
    <div className="workspace-header terminal-header">
      <div><div className="breadcrumb">服务器 / {profile.data?.name ?? "…"} / <span>终端</span></div><h1>SSH 终端</h1><p>{profile.data ? `${profile.data.username}@${profile.data.host}` : "正在载入"}</p></div>
      <div className="workspace-header__actions"><Button size="sm" onClick={() => setShortcutManagerOpen(true)}><Sparkles size={14} /> 快捷指令</Button><span className={`connection-pill ${connection.data?.status === "online" ? "is-online" : ""}`}><i /> {connection.data?.status === "online" ? `${tabs.length} 个交互会话` : "SSH 已断开"}</span></div>
    </div>
    <nav className="workspace-tabs"><NavLink end to={`/servers/${serverId}`}>概览</NavLink><NavLink to={`/servers/${serverId}/files`}>文件</NavLink><NavLink className="active" to={`/servers/${serverId}/terminal`}>终端</NavLink><NavLink to={`/servers/${serverId}/operations`}>端口与进程</NavLink><NavLink to={`/servers/${serverId}/services`}>服务</NavLink><NavLink to={`/servers/${serverId}/tools`}>工具</NavLink><NavLink to={`/servers/${serverId}/logs`}>日志</NavLink><NavLink to={`/servers/${serverId}/nginx`}>Nginx</NavLink><NavLink to={`/servers/${serverId}/docker`}>Docker</NavLink></nav>
    <div className="terminal-workspace">
      <div className="terminal-tabbar">
        <div className="terminal-tabs">
          {tabs.map((tab) => <button key={tab.key} className={`terminal-tab ${activeKey === tab.key ? "active" : ""}`} onClick={() => setActiveKey(tab.key)} onDoubleClick={() => setEditingKey(tab.key)}>
            <TerminalSquare size={13} />
            {editingKey === tab.key ? <input autoFocus defaultValue={tab.title} onClick={(event) => event.stopPropagation()} onBlur={(event) => renameTab(tab.key, event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") renameTab(tab.key, event.currentTarget.value); if (event.key === "Escape") setEditingKey(null); }} /> : <span>{tab.title}</span>}
            <Pencil className="terminal-rename" size={10} />
            <span className="terminal-tab__history" role="button" tabIndex={0} title="查看本 shell 命令历史" aria-label="查看本 shell 命令历史" onClick={(event) => { event.stopPropagation(); setActiveKey(tab.key); setHistoryOpenKey(tab.key); }} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); event.stopPropagation(); setActiveKey(tab.key); setHistoryOpenKey(tab.key); } }}><HistoryIcon size={10} /></span>
            <span className={`terminal-tab__shortcut ${tab.shortcutsEnabled ? "is-on" : "is-off"}`} role="button" tabIndex={0} title={tab.shortcutsEnabled ? "关闭本 shell 快捷指令" : "开启本 shell 快捷指令"} aria-label={tab.shortcutsEnabled ? "关闭本 shell 快捷指令" : "开启本 shell 快捷指令"} onClick={(event) => { event.stopPropagation(); toggleShortcuts(tab.key); }} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); event.stopPropagation(); toggleShortcuts(tab.key); } }}><Power size={10} /></span>
            <X size={12} onClick={(event) => { event.stopPropagation(); closeTab(tab.key); }} />
          </button>)}
        </div>
        <button className="terminal-add" title="新建终端" onClick={addTab}><Plus size={14} /></button>
        {searchOpen && <form className="terminal-search" onSubmit={(event) => { event.preventDefault(); setSearchRequest((value) => value + 1); }}><Search size={12} /><input autoFocus value={searchQuery} onChange={(event) => setSearchQuery(event.target.value)} placeholder="搜索输出" /><button type="button" onClick={() => setSearchOpen(false)}><X size={12} /></button></form>}
        <div className="terminal-tools"><button title="搜索终端输出" onClick={() => setSearchOpen((value) => !value)}><Search size={14} /></button><button title="查看当前 shell 命令历史" onClick={() => setHistoryOpenKey((value) => value === activeKey ? null : activeKey)}><HistoryIcon size={14} /></button><button title="清屏" onClick={() => setClearRequest((value) => value + 1)}><Eraser size={14} /></button><button onClick={() => setFontSize((value) => Math.max(9, value - 1))} title="缩小字体"><Minus size={14} /></button><span>{fontSize}px</span><button onClick={() => setFontSize((value) => Math.min(24, value + 1))} title="放大字体"><Plus size={14} /></button></div>
      </div>
      {connection.data?.status !== "online" && <div className="terminal-blocked">服务器连接已断开。<NavLink to={`/servers/${serverId}`}>返回概览重新连接</NavLink></div>}
      {tabs.map((tab) => <SessionTerminal key={`${tab.key}:${tab.revision}`} serverId={serverId} active={activeKey === tab.key} fontSize={fontSize} shortcutsEnabled={tab.shortcutsEnabled} shortcutToggleVersion={tab.shortcutToggleVersion} shortcuts={shortcuts.data ?? []} historyOpen={historyOpenKey === tab.key} onHistoryClose={() => setHistoryOpenKey(null)} searchQuery={searchQuery} searchRequest={activeKey === tab.key ? searchRequest : 0} clearRequest={activeKey === tab.key ? clearRequest : 0} onReconnect={() => reopenTab(tab.key)} />)}
    </div>
    <ShortcutManager serverId={serverId} open={shortcutManagerOpen} onClose={() => { setShortcutManagerOpen(false); void shortcuts.refetch(); }} />
  </section>;
}

interface SessionProps {
  serverId: string;
  active: boolean;
  fontSize: number;
  searchQuery: string;
  searchRequest: number;
  clearRequest: number;
  shortcutsEnabled: boolean;
  shortcutToggleVersion: number;
  shortcuts: ShortcutRecord[];
  historyOpen: boolean;
  onHistoryClose: () => void;
  onReconnect: () => void;
}

function SessionTerminal({ serverId, active, fontSize, shortcutsEnabled, shortcutToggleVersion, shortcuts, historyOpen, onHistoryClose, searchQuery, searchRequest, clearRequest, onReconnect }: SessionProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  const terminalIdRef = useRef<string | null>(null);
  const activeRef = useRef(active);
  const [status, setStatus] = useState<"opening" | "online" | "closed" | "error">("opening");
  const [error, setError] = useState<string | null>(null);
  const [inputBuffer, setInputBuffer] = useState("");
  const [selectedSuggestion, setSelectedSuggestion] = useState(0);
  const [pendingShortcut, setPendingShortcut] = useState<ShortcutRecord | null>(null);
  const [variableValues, setVariableValues] = useState<Record<string, string>>({});
  const [shortcutSessionHiddenVersion, setShortcutSessionHiddenVersion] = useState<number | null>(null);
  const [suggestionsHidden, setSuggestionsHidden] = useState(false);
  const [history, setHistory] = useState<TerminalHistoryItem[]>([]);
  const [historyNotice, setHistoryNotice] = useState<string | null>(null);
  const sessionShortcutsHidden = shortcutSessionHiddenVersion === shortcutToggleVersion;
  const suggestions = useMemo(() => shortcutsEnabled && !sessionShortcutsHidden && !suggestionsHidden && inputBuffer.trim() ? matchShortcuts(shortcuts, inputBuffer) : [], [inputBuffer, shortcuts, shortcutsEnabled, sessionShortcutsHidden, suggestionsHidden]);
  const suggestionsRef = useRef<ShortcutRecord[]>([]);
  const selectedSuggestionRef = useRef(0);
  const inputBufferRef = useRef("");
  const pendingShortcutRef = useRef<ShortcutRecord | null>(null);
  useEffect(() => {
    suggestionsRef.current = suggestions;
    selectedSuggestionRef.current = selectedSuggestion;
    inputBufferRef.current = inputBuffer;
    pendingShortcutRef.current = pendingShortcut;
  }, [inputBuffer, pendingShortcut, selectedSuggestion, suggestions]);

  useEffect(() => {
    activeRef.current = active;
  }, [active]);

  useEffect(() => {
    if (!hostRef.current) return;
    let disposed = false;
    const terminal = new Terminal({
      cursorBlink: true, cursorStyle: "bar", fontFamily: '"Cascadia Mono", "JetBrains Mono", monospace',
      fontSize: 13, lineHeight: 1.25, scrollback: 10_000, allowProposedApi: false,
      theme: { background: "#090d0e", foreground: "#d9e2df", cursor: "#c7f36b", selectionBackground: "#40502b", black: "#111718", brightBlack: "#65716e", green: "#b7dc62", brightGreen: "#c7f36b", red: "#ff735c", brightRed: "#ff9180", yellow: "#e8b85e", brightYellow: "#f3ca7c", blue: "#6fa8dc", brightBlue: "#8ebfe9", magenta: "#b28ad5", brightMagenta: "#c8a4e6", cyan: "#66b8ad", brightCyan: "#82d0c5", white: "#c8d0ce", brightWhite: "#f2f6f4" },
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    terminal.loadAddon(fit);
    terminal.loadAddon(search);
    terminal.open(hostRef.current);
    terminalRef.current = terminal;
    fitRef.current = fit;
    searchRef.current = search;
    fit.fit();

    const handleEvent = (event: TerminalEvent) => {
      if (disposed) return;
      if (event.event === "data") terminal.write(decodeBase64(event.data.data));
      if (event.event === "exit") terminal.writeln(`\r\n\x1b[33m[进程退出: ${event.data.exitStatus}]\x1b[0m`);
      if (event.event === "closed") { setStatus("closed"); terminal.writeln("\r\n\x1b[31m[SSH 终端已断开]\x1b[0m"); }
    };
    api.openTerminal(serverId, terminal.cols, terminal.rows, handleEvent).then((terminalId) => {
      if (disposed) { void api.closeTerminal(terminalId); return; }
      terminalIdRef.current = terminalId;
      setStatus("online");
      if (activeRef.current) terminal.focus();
    }).catch((reason) => { if (!disposed) { setStatus("error"); setError(errorMessage(reason)); } });

    /** Records a submitted command with the terminal line where its output begins. */
    const recordCommand = (command: string) => {
      const normalized = command.replace(/\s+/g, " ").trim();
      if (!normalized) return;
      const line = terminal.buffer.active.baseY + terminal.buffer.active.cursorY;
      setHistory((current) => [...current, { id: crypto.randomUUID(), command: normalized, line, at: Date.now() }].slice(-200));
    };

    /** Inserts a shortcut into the remote shell without executing it automatically. */
    const insertShortcut = (shortcut: ShortcutRecord, values: Record<string, string> = {}) => {
      const command = materializeShortcut(shortcut.commandTemplate, values);
      if (!command || !terminalIdRef.current) return;
      void api.writeTerminal(terminalIdRef.current, new TextEncoder().encode("\u0015" + command)).catch((reason) => setError(errorMessage(reason)));
      inputBufferRef.current = command;
      setInputBuffer(command);
      setSelectedSuggestion(0);
      setPendingShortcut(null);
      setVariableValues({});
      setSuggestionsHidden(true);
      void api.useShortcut(shortcut.id);
    };
    const chooseShortcut = (shortcut: ShortcutRecord) => {
      const variables = shortcutVariables(shortcut.commandTemplate);
      if (variables.length) {
        setVariableValues(Object.fromEntries(variables.map((name) => [name, ""])));
        setPendingShortcut(shortcut);
      } else {
        insertShortcut(shortcut);
      }
    };
    const dataDisposable = terminal.onData((data) => {
      if ((data.includes("\n") || data.includes("\r")) && data.length > 2 && !window.confirm("即将粘贴多行内容到远程终端，确定继续吗？")) return;
      if (data === "\r" || data === "\n") {
        recordCommand(inputBufferRef.current);
        inputBufferRef.current = "";
        setInputBuffer("");
        setSuggestionsHidden(false);
      } else if (data.includes("\n") || data.includes("\r")) {
        inputBufferRef.current = "";
        setInputBuffer("");
        setSuggestionsHidden(false);
      } else if (data === "\u0003" || data === "\u0015" || data.includes("\u001b")) {
        inputBufferRef.current = "";
        setInputBuffer("");
        setSuggestionsHidden(false);
      } else if (data === "\u007f") {
        inputBufferRef.current = inputBufferRef.current.slice(0, -1);
        setInputBuffer(inputBufferRef.current);
        setSuggestionsHidden(false);
      } else if (data.length && [...data].every((character) => character >= " " && character !== "\u007f")) {
        inputBufferRef.current += data;
        setInputBuffer(inputBufferRef.current);
        setSelectedSuggestion(0);
        setSuggestionsHidden(false);
      }
      if (terminalIdRef.current) void api.writeTerminal(terminalIdRef.current, new TextEncoder().encode(data)).catch((reason) => setError(errorMessage(reason)));
    });
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") return true;
      if (event.key === "Tab" && suggestionsRef.current[selectedSuggestionRef.current]) {
        chooseShortcut(suggestionsRef.current[selectedSuggestionRef.current]);
        return false;
      }
      if (event.key === "ArrowDown" && suggestionsRef.current.length) {
        setSelectedSuggestion((value) => Math.min(value + 1, suggestionsRef.current.length - 1));
        return false;
      }
      if (event.key === "ArrowUp" && suggestionsRef.current.length) {
        setSelectedSuggestion((value) => Math.max(value - 1, 0));
        return false;
      }
      if (event.key === "Escape" && (suggestionsRef.current.length || pendingShortcutRef.current)) {
        setSelectedSuggestion(0);
        setPendingShortcut(null);
        setSuggestionsHidden(true);
        return false;
      }
      if (!event.ctrlKey || !event.shiftKey) return true;
      if (event.key.toLowerCase() === "c" && terminal.hasSelection()) { void navigator.clipboard.writeText(terminal.getSelection()); return false; }
      if (event.key.toLowerCase() === "v") { void navigator.clipboard.readText().then((value) => terminal.paste(value)); return false; }
      return true;
    });
    const resizeObserver = new ResizeObserver(() => {
      if (!activeRef.current) return;
      fit.fit();
      if (terminalIdRef.current) void api.resizeTerminal(terminalIdRef.current, terminal.cols, terminal.rows);
    });
    resizeObserver.observe(hostRef.current);
    return () => {
      disposed = true;
      resizeObserver.disconnect();
      dataDisposable.dispose();
      terminal.dispose();
      terminalRef.current = null;
      fitRef.current = null;
      searchRef.current = null;
      if (terminalIdRef.current) void api.closeTerminal(terminalIdRef.current);
      terminalIdRef.current = null;
    };
  }, [serverId]);

  useEffect(() => { if (terminalRef.current) terminalRef.current.options.fontSize = fontSize; fitRef.current?.fit(); }, [fontSize]);
  useEffect(() => { if (active) { requestAnimationFrame(() => { fitRef.current?.fit(); terminalRef.current?.focus(); }); } }, [active]);
  useEffect(() => { if (searchRequest && searchQuery) searchRef.current?.findNext(searchQuery, { caseSensitive: false, incremental: false }); }, [searchQuery, searchRequest]);
  useEffect(() => {
    if (!clearRequest) return;
    terminalRef.current?.clear();
  }, [clearRequest]);

  const variables = pendingShortcut ? shortcutVariables(pendingShortcut.commandTemplate) : [];
  const sessionClass = active ? "terminal-session is-active" : "terminal-session";
  /** Hides suggestions until the current command line changes. */
  const hideSuggestions = () => {
    setSuggestionsHidden(true);
    setPendingShortcut(null);
  };
  /** Hides shortcut completion for this shell until its per-tab switch is toggled again. */
  const hideSessionShortcuts = () => {
    setShortcutSessionHiddenVersion(shortcutToggleVersion);
    setSuggestionsHidden(true);
    setPendingShortcut(null);
  };
  /** Handles a clicked suggestion, opening its variable form or inserting a static command. */
  const selectShortcut = (shortcut: ShortcutRecord) => {
    const names = shortcutVariables(shortcut.commandTemplate);
    if (names.length) {
      setVariableValues(Object.fromEntries(names.map((name) => [name, ""])));
      setPendingShortcut(shortcut);
      return;
    }
    const command = materializeShortcut(shortcut.commandTemplate, {});
    if (!command || !terminalIdRef.current) return;
    void api.writeTerminal(terminalIdRef.current, new TextEncoder().encode("\u0015" + command)).catch((reason) => setError(errorMessage(reason)));
    inputBufferRef.current = command;
    setInputBuffer(command);
    setSelectedSuggestion(0);
    setSuggestionsHidden(true);
    void api.useShortcut(shortcut.id);
  };
  /** Moves the xterm viewport to the output line associated with a stored command. */
  const scrollToHistory = (item: TerminalHistoryItem) => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    const maxLine = terminal.buffer.active.baseY;
    terminal.scrollToLine(Math.max(0, Math.min(item.line, maxLine)));
    setHistoryNotice(`已定位到 ${formatHistoryTime(item.at)} 执行的命令`);
  };
  /** Returns the shell viewport to the newest output after browsing command history. */
  const scrollHistoryToBottom = () => {
    terminalRef.current?.scrollToBottom();
    setHistoryNotice("已回到最新输出");
  };
  const commitVariableShortcut = (event: FormEvent) => {
    event.preventDefault();
    if (!pendingShortcut || !terminalIdRef.current) return;
    const command = materializeShortcut(pendingShortcut.commandTemplate, variableValues);
    if (!command) return;
    void api.writeTerminal(terminalIdRef.current, new TextEncoder().encode("\u0015" + command));
    void api.useShortcut(pendingShortcut.id);
    inputBufferRef.current = command;
    setPendingShortcut(null);
    setInputBuffer(command);
    setSuggestionsHidden(true);
  };
  return <div className={sessionClass}>
    <div ref={hostRef} className="terminal-host" aria-label="SSH 交互终端" />
    {suggestions.length > 0 && <div className="terminal-shortcut-popover" role="listbox" aria-label="快捷指令建议">
      <header className="terminal-shortcut-popover__header">
        <div><strong>快捷建议</strong><small>Tab 插入 · Enter 执行 · ↑↓ 选择</small></div>
        <div className="terminal-shortcut-popover__actions">
          <button type="button" title="仅暂时隐藏建议" onMouseDown={(event) => event.preventDefault()} onClick={hideSuggestions}><EyeOff size={12} />本次隐藏</button>
          <button type="button" title="本 shell 会话内不再显示建议" onMouseDown={(event) => event.preventDefault()} onClick={hideSessionShortcuts}><Power size={12} />本 shell 隐藏</button>
        </div>
      </header>
      <div className="terminal-shortcut-popover__list">{suggestions.map((shortcut, index) => <button key={shortcut.id} className={index === selectedSuggestion ? "is-selected" : ""} onMouseDown={(event) => { event.preventDefault(); selectShortcut(shortcut); }}><span><strong>{shortcut.name}</strong><code>{shortcut.commandTemplate}</code></span><small><b>{shortcut.groupName || "未分组"}</b> · {shortcut.description}</small></button>)}</div>
    </div>}
    {pendingShortcut && <form className="terminal-variable-popover" onSubmit={commitVariableShortcut}><div><strong>填写快捷指令参数</strong><button type="button" onClick={() => setPendingShortcut(null)} aria-label="取消"><X size={13} /></button></div>{variables.map((name) => <label key={name}><span>{name}</span><input autoFocus={name === variables[0]} value={variableValues[name] ?? ""} onChange={(event) => setVariableValues((current) => ({ ...current, [name]: event.target.value }))} required /></label>)}<div className="dialog-actions"><Button type="button" onClick={() => setPendingShortcut(null)}>取消</Button><Button type="submit" variant="primary">插入命令</Button></div></form>}
    {historyOpen && <aside className="terminal-history-panel" aria-label="当前 shell 命令历史">
      <header><div><strong>命令历史</strong><small>{history.length} 条 · 仅当前 shell</small></div><button type="button" title="关闭历史" onClick={onHistoryClose}><X size={14} /></button></header>
      <div className="terminal-history-panel__actions"><button type="button" onClick={scrollHistoryToBottom}><ArrowDown size={13} />滚动到底部</button>{historyNotice && <span>{historyNotice}</span>}</div>
      <div className="terminal-history-list">
        {!history.length && <div className="terminal-history-empty">执行命令后会显示在这里，可点击命令定位到对应输出。</div>}
        {history.slice().reverse().map((item) => <button type="button" key={item.id} onClick={() => scrollToHistory(item)}><code>{item.command}</code><small>{formatHistoryTime(item.at)}</small><span>定位</span></button>)}
      </div>
    </aside>}
    {status === "opening" && <div className="terminal-session-state">正在创建 PTY…</div>}{error && <div className="terminal-error">{error}<Button size="sm" onClick={onReconnect}><RotateCw size={13} /> 重开会话</Button></div>}{status === "closed" && !error && <div className="terminal-error">会话已关闭<Button size="sm" onClick={onReconnect}><RotateCw size={13} /> 重开会话</Button></div>}
  </div>;
}
