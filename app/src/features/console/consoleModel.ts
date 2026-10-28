export interface ConsoleError { code: string; message: string; details?: unknown }
export interface ConsoleReply { id: string; ok: boolean; data?: unknown; error?: ConsoleError; format?: "json" }
export interface ConsoleTask {
  operationId: string; requestId: string; libraryRequestId: string; method: string;
  status: "running" | "succeeded" | "failed" | "cancelled";
  startedAt: number; finishedAt: number | null; cancellable: boolean; cancelRequested: boolean;
  progress?: unknown; data?: unknown; error?: ConsoleError;
}
export interface CommandSpec { name: string; method: string; description: string; usage: string; group: string; positionals: string[]; options: string[]; async: boolean }
export interface ConsoleApi { enabled: boolean; address: string | null; configPath: string | null; error: string | null }
export interface ConsoleCatalog { commands: CommandSpec[]; settings: { path: string }[]; events: string[]; api: ConsoleApi; version: string; apiVersion: number }
export interface ConsoleEntry { id: string; line: string; time: number; status: "running" | "succeeded" | "failed" | "event" | "system"; data?: unknown; error?: ConsoleError; task?: ConsoleTask; json?: boolean }
export function startupText(catalog: ConsoleCatalog): string {
  return `DiscoAS ${catalog.version}\nAPI v${catalog.apiVersion} · ${catalog.api.error ?? catalog.api.address ?? "off"}`;
}

export function record(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : undefined;
}
export function outputText(value: unknown, json = false): string {
  if (value === undefined || value === null) return json ? "null" : "完成。";
  if (json) return JSON.stringify(value, null, 2);
  if (typeof value === "string") return value;
  if (typeof value !== "object") return String(value);
  const object = record(value);
  if (object && Array.isArray(object.commands)) {
    return (object.commands as CommandSpec[]).map(row => `${row.usage}\n  ${row.description}`).join("\n\n");
  }
  if (Array.isArray(value) && value.length === 0) return "（空）";
  return JSON.stringify(value, null, 2);
}
export function complete(input: string, catalog: ConsoleCatalog | null): string[] {
  if (!catalog) return [];
  const normalized = input.trimStart();
  const settings = normalized.match(/^(settings\s+(?:get|set|schema)\s+)(\S*)$/);
  if (settings) return catalog.settings.filter(item => item.path.startsWith(settings[2])).slice(0, 10).map(item => settings[1] + item.path);
  const help = normalized.match(/^(help\s+)(.*)$/);
  const prefix = help ? help[2] : normalized;
  const commands = catalog.commands.filter(item => item.name.startsWith(prefix) || item.method.startsWith(prefix)).slice(0, 10);
  return commands.map(item => (help ? help[1] : "") + item.name);
}
export function appendEntry(entries: ConsoleEntry[], next: ConsoleEntry): ConsoleEntry[] { return [...entries, next].slice(-200); }
export function applyTask(entries: ConsoleEntry[], task: ConsoleTask): ConsoleEntry[] {
  return entries.map(entry => entry.id !== task.requestId ? entry : {
    ...entry, task,
    status: task.finishedAt === null ? "running" : task.status === "succeeded" ? "succeeded" : "failed",
    ...(task.finishedAt !== null ? { data: task.data, error: task.error } : {}),
  });
}
