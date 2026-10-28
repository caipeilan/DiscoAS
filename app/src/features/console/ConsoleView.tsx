import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "../../Icon";
import { Popover } from "../../components/Popover";
import { t } from "../../i18n";
import { complete, outputText, type ConsoleEntry } from "./consoleModel";
import type { ConsoleController } from "./useConsole";
import "./console.css";

function Entry({ entry, json }: { entry: ConsoleEntry; json: boolean }) {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const text = entry.error ? `${entry.error.code}: ${entry.error.message}${entry.error.details ? `\n${outputText(entry.error.details, true)}` : ""}` : outputText(entry.data, entry.json || json);
  if (entry.status === "system") return <pre className="console-banner">{outputText(entry.data)}</pre>;
  const large = text.length > 16000;
  return <article className="console-entry" data-status={entry.status}>
    <div className="console-command"><span className="console-input-prompt" aria-hidden="true">{entry.status === "event" ? <span>↳</span> : <>DiscoAS <span>❯</span></>}</span><code>{entry.line}</code>
      {entry.status === "running" && <span className="console-entry-state">{t("执行中")}</span>}
      {entry.status !== "running" && <button type="button" className="console-copy" title={copied ? t("已复制") : t("复制输出")} aria-label={t("复制输出")} onClick={() => {
        void navigator.clipboard.writeText(text).then(() => { setCopied(true); window.setTimeout(() => setCopied(false), 1400); });
      }}><Icon name={copied ? "check" : "copy"} size={14} /></button>}
    </div>
    {entry.status === "running" ? <pre className="console-result">{entry.task?.progress ? outputText(entry.task.progress, json) : "…"}{entry.task?.cancelRequested ? `\n${t("正在取消…")}` : ""}</pre>
      : <pre className="console-result">{large && !expanded ? `${text.slice(0, 16000)}\n…` : text}</pre>}
    {large && <button type="button" className="text-button console-expand" onClick={() => setExpanded(value => !value)}>{expanded ? t("收起输出") : t("展开完整输出")}</button>}
  </article>;
}
export default function ConsoleView({ controller }: { controller: ConsoleController }) {
  const { entries, history, catalog, format, setFormat, clear, execute, interrupt } = controller;
  const [input, setInput] = useState("");
  const [suggesting, setSuggesting] = useState(false);
  const [choice, setChoice] = useState(0);
  const historyIndex = useRef<number | null>(null);
  const scratch = useRef("");
  const field = useRef<HTMLInputElement>(null);
  const viewport = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const suggestions = useMemo(() => complete(input, catalog), [input, catalog]);
  const closeSuggestions = useCallback(() => setSuggesting(false), []);
  useEffect(() => { field.current?.focus(); }, []);
  useEffect(() => { if (pinned.current && viewport.current) viewport.current.scrollTop = viewport.current.scrollHeight; }, [entries]);
  const run = (line = input) => { if (!line.trim()) return; setInput(""); setSuggesting(false); historyIndex.current = null; pinned.current = true; void execute(line); field.current?.focus(); };
  const accept = (value: string) => { setInput(value + " "); setSuggesting(false); field.current?.focus(); };
  const apiTitle = catalog?.api.error ?? `${catalog?.api.enabled ? t("关闭本机接口") : t("开启本机接口")}${catalog?.api.address ? ` · ${catalog.api.address}` : ""}`;
  return <section className="console-page" aria-label={t("控制台")}>
    <header className="console-header"><h1>{t("控制台")}</h1>
      <div className="console-tools">
        <button type="button" className="console-api" aria-pressed={catalog?.api.enabled ?? false} title={apiTitle} aria-label={apiTitle} onClick={() => run(catalog?.api.enabled ? "api stop" : "api start")}>
          <span className="console-status-dot" data-enabled={catalog?.api.enabled || undefined} />{catalog?.api.enabled ? catalog.api.address?.replace("http://", "") : "API"}
        </button>
        <div className="console-format" role="group" aria-label={t("输出格式")}>{(["text", "json"] as const).map(value => <button type="button" key={value} aria-pressed={format === value} onClick={() => setFormat(value)}>{value === "text" ? t("文本") : "JSON"}</button>)}</div>
        <span className="console-tool-separator" aria-hidden="true" />
        <button type="button" className="console-tool" title={t("命令帮助")} aria-label={t("命令帮助")} onClick={() => run("help")}><span className="console-help" aria-hidden="true">?</span></button>
        <button type="button" className="console-tool" title={t("清屏")} aria-label={t("清屏")} onClick={() => { clear(); field.current?.focus(); }}><Icon name="remove" size={17} /></button>
      </div>
    </header>
    <div className="console-terminal">
      <div ref={viewport} className="console-scroll" onScroll={() => { const node = viewport.current; if (node) pinned.current = node.scrollHeight - node.scrollTop - node.clientHeight < 40; }}
        onClick={event => { if (event.target === event.currentTarget && !window.getSelection()?.toString()) field.current?.focus(); }}>
        <div className="console-transcript" role="log" aria-label={t("命令输出")} aria-live="polite">{entries.map(entry => <Entry key={entry.id} entry={entry} json={format === "json"} />)}</div>
        <form className="console-input-row" onSubmit={event => { event.preventDefault(); if (suggesting && suggestions[choice]) accept(suggestions[choice]); else run(); }}>
          <label htmlFor="console-command-input" className="console-input-prompt">DiscoAS <span aria-hidden="true">❯</span></label>
          <input ref={field} id="console-command-input" value={input} autoComplete="off" spellCheck={false} autoCapitalize="off" aria-label={t("输入命令")}
            role="combobox" aria-autocomplete="list" aria-expanded={suggesting && suggestions.length > 0} aria-controls={suggesting ? "console-completions" : undefined} aria-activedescendant={suggesting && suggestions[choice] ? `console-completion-${choice}` : undefined}
            onChange={event => { setInput(event.target.value); historyIndex.current = null; setSuggesting(false); setChoice(0); }}
            onKeyDown={event => {
              if (event.nativeEvent.isComposing) return;
              if (event.ctrlKey && event.key.toLowerCase() === "c" && !window.getSelection()?.toString() && field.current?.selectionStart === field.current?.selectionEnd) { event.preventDefault(); void interrupt(); return; }
              if (event.key === "Tab" && !event.shiftKey && suggestions.length) { event.preventDefault(); if (suggestions.length === 1) accept(suggestions[0]); else { setSuggesting(true); setChoice(current => suggesting ? (current + 1) % suggestions.length : 0); } return; }
              if (event.key === "Escape") { setSuggesting(false); return; }
              if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
              event.preventDefault();
              if (suggesting) { setChoice(current => (current + (event.key === "ArrowUp" ? -1 : 1) + suggestions.length) % suggestions.length); return; }
              if (historyIndex.current === null) { scratch.current = input; historyIndex.current = history.length; }
              historyIndex.current = Math.max(0, Math.min(history.length, historyIndex.current + (event.key === "ArrowUp" ? -1 : 1)));
              setInput(historyIndex.current === history.length ? scratch.current : history[historyIndex.current] ?? "");
            }} />
        </form>
      </div>
    </div>
    {suggesting && suggestions.length > 0 && field.current && <Popover anchor={field.current} onClose={closeSuggestions} className="console-completions" id="console-completions" role="listbox">{suggestions.map((value, index) => <button type="button" tabIndex={-1} role="option" aria-selected={choice === index} id={`console-completion-${index}`} key={value} onMouseDown={event => event.preventDefault()} onClick={() => accept(value)}><code>{value}</code></button>)}</Popover>}
  </section>;
}
