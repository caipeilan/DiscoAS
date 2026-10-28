import { useCallback, useEffect, useRef, useState } from "react";
import { call, onDesktopEvent } from "../../services/desktop";
import { errorText } from "../../i18n";
import { appendEntry, applyTask, record, startupText, type ConsoleCatalog, type ConsoleEntry, type ConsoleReply } from "./consoleModel";

export function useConsole(visible: boolean) {
  const [entries, setEntries] = useState<ConsoleEntry[]>([]);
  const [history, setHistory] = useState<string[]>([]);
  const [catalog, setCatalog] = useState<ConsoleCatalog | null>(null);
  const [format, setFormat] = useState<"text" | "json">("text");
  const [watching, setWatching] = useState(false);
  const sequence = useRef(0);
  const initialized = useRef(false);
  const watches = useRef(new Set<string>());
  const active = useRef(visible); active.current = visible;
  const running = useRef(entries); running.current = entries;
  const clear = useCallback(() => { initialized.current = true; setEntries([]); }, []);
  const updateApi = useCallback(async () => {
    try {
      const next = await call<ConsoleCatalog>("console_catalog"); setCatalog(next);
      if (!initialized.current) {
        initialized.current = true;
        setEntries(current => [{ id: "console-start", line: "", time: Date.now(), status: "system", data: startupText(next) }, ...current]);
      }
    } catch { /* The next command reports transport errors inline. */ }
  }, []);
  useEffect(() => { if (visible) void updateApi(); }, [visible, updateApi]);
  useEffect(() => {
    let alive = true, cleanup: (() => void) | undefined;
    void onDesktopEvent("console-api-changed", api => setCatalog(current => current ? { ...current, api } : current))
      .then(stop => { if (alive) cleanup = stop; else stop(); });
    return () => { alive = false; cleanup?.(); };
  }, []);
  useEffect(() => {
    let alive = true, cleanup: (() => void) | undefined;
    void onDesktopEvent("console-task-changed", task => setEntries(current => applyTask(current, task)))
      .then(stop => { if (alive) cleanup = stop; else stop(); });
    return () => { alive = false; cleanup?.(); };
  }, []);
  const stopWatches = useCallback(async () => {
    const ids = [...watches.current]; watches.current.clear();
    setWatching(false);
    await Promise.allSettled(ids.map(subscriptionId => call("console_execute", { request: { method: "event.unwatch", params: { subscriptionId } } })));
  }, []);
  useEffect(() => {
    if (!visible) { void stopWatches(); return; }
    if (!watching) return;
    let stopped = false, timer = 0;
    const poll = async () => {
      for (const subscriptionId of [...watches.current]) {
        try {
          const reply = await call<ConsoleReply>("console_execute", { request: { method: "event.poll", params: { subscriptionId } } });
          if (stopped || !watches.current.has(subscriptionId)) continue;
          if (!reply.ok) {
            watches.current.delete(subscriptionId);
            setEntries(current => appendEntry(current, { id: `event-error-${++sequence.current}`, line: `event poll ${subscriptionId}`, time: Date.now(), status: "failed", error: reply.error }));
            continue;
          }
          const result = record(reply.data);
          const events = Array.isArray(result?.events) ? result.events : [];
          for (const event of events) {
            const value = record(event);
            setEntries(current => appendEntry(current, { id: `event-${++sequence.current}`, line: String(value?.event ?? "event"), time: Number(value?.time ?? Date.now()), status: "event", data: value?.payload }));
          }
          if (result?.closed) watches.current.delete(subscriptionId);
        } catch (error) {
          watches.current.delete(subscriptionId);
          setEntries(current => appendEntry(current, { id: `event-error-${++sequence.current}`, line: `event poll ${subscriptionId}`, time: Date.now(), status: "failed", error: { code: "CONNECTION_FAILED", message: errorText(error) } }));
        }
      }
      if (!stopped && watches.current.size) timer = window.setTimeout(poll, 350);
      else if (!stopped) setWatching(false);
    };
    timer = window.setTimeout(poll, 350);
    return () => { stopped = true; clearTimeout(timer); void stopWatches(); };
  }, [visible, watching, stopWatches]);
  const execute = useCallback(async (line: string) => {
    line = line.trim(); if (!line) return;
    const id = `ui-${Date.now()}-${++sequence.current}`;
    setHistory(current => current[current.length - 1] === line ? current : [...current, line].slice(-200));
    setEntries(current => appendEntry(current, { id, line, time: Date.now(), status: "running" }));
    try {
      const reply = await call<ConsoleReply>("console_execute", { request: { id, command: line } });
      const value = record(reply.data);
      if (reply.ok && typeof value?.subscriptionId === "string" && !value.stopped) {
        if (active.current) { watches.current.add(value.subscriptionId); setWatching(true); }
        else void call("console_execute", { request: { method: "event.unwatch", params: { subscriptionId: value.subscriptionId } } });
      }
      if (reply.ok && /^(?:console[ .]output)\b/.test(line) && (value?.format === "text" || value?.format === "json")) setFormat(value.format);
      if (/^api[ .]/.test(line)) void updateApi();
      setEntries(current => current.map(entry => entry.id !== id ? entry : {
        ...entry, status: entry.task?.finishedAt === null ? "running" : reply.ok ? "succeeded" : "failed",
        data: entry.task?.finishedAt ? entry.task.data : reply.data, error: entry.task?.finishedAt ? entry.task.error : reply.error, json: reply.format === "json",
      }));
    } catch (error) {
      setEntries(current => current.map(entry => entry.id !== id ? entry : { ...entry, status: "failed", error: { code: "CONNECTION_FAILED", message: errorText(error) } }));
    }
  }, [updateApi]);
  const interrupt = useCallback(async () => {
    const task = [...running.current].reverse().find(entry => entry.status === "running" && entry.task)?.task;
    if (task) await execute(`task cancel ${task.operationId}`);
    else if (watches.current.size) { await stopWatches(); setEntries(current => appendEntry(current, { id: `stop-${++sequence.current}`, line: "Ctrl+C", time: Date.now(), status: "succeeded", data: "已停止当前控制台的事件订阅。" })); }
  }, [execute, stopWatches]);
  return { entries, history, catalog, format, setFormat, clear, execute, interrupt };
}
export type ConsoleController = ReturnType<typeof useConsole>;
