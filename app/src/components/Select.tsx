import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Icon } from "../Icon";
import { t } from "../i18n";
import { Popover } from "./Popover";
import { usePresence } from "./Motion";
import { filterOptions, moveOption, type SelectOption } from "./selectModel";

export function Select({ value, onChange, options, label, disabled = false, className = "", searchable = false,
  searchLabel, placeholder, emptyMessage }: {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  label: string;
  disabled?: boolean;
  className?: string;
  searchable?: boolean;
  searchLabel?: string;
  placeholder?: string;
  emptyMessage?: string;
}) {
  const trigger = useRef<HTMLButtonElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const id = useId();
  const [open, setOpen] = useState(false);
  const presence = usePresence(open, 140);
  const [query, setQuery] = useState("");
  const [highlighted, setHighlighted] = useState(-1);
  const highlightedNode = useRef<Element | null>(null);
  const change = useRef(onChange); change.current = onChange;
  const close = useCallback(() => setOpen(false), []);
  const filtered = useMemo(() => filterOptions(options, query), [options, query]);
  const selected = useMemo(() => options.find((option) => option.value === value), [options, value]);
  const reveal = () => {
    if (disabled || trigger.current?.matches(":disabled")) return;
    setQuery("");
    const index = options.findIndex((option) => option.value === value && !option.disabled);
    setHighlighted(index >= 0 ? index : moveOption(options, -1, 1));
    setOpen(true);
  };
  const choose = useCallback((index: number) => {
    const option = filtered[index];
    if (!option || option.disabled || disabled || trigger.current?.matches(":disabled")) { close(); return; }
    change.current(option.value); close(); trigger.current?.focus();
  }, [filtered, disabled, close]);
  useEffect(() => { if (disabled) close(); }, [disabled, close]);
  useEffect(() => {
    if (open && searchable) search.current?.focus();
  }, [open, searchable]);
  useEffect(() => {
    if (open && highlighted >= 0) list.current?.children[highlighted]?.scrollIntoView({ block: "nearest" });
  }, [open, highlighted]);
  useLayoutEffect(() => {
    highlightedNode.current?.classList.remove("highlighted");
    highlightedNode.current = open && highlighted >= 0 ? list.current?.children[highlighted] ?? null : null;
    highlightedNode.current?.classList.add("highlighted");
  }, [open, highlighted, filtered]);
  const optionNodes = useMemo(() => presence.mounted ? filtered.map((option, index) => <button key={option.value} id={`${id}-${index}`} type="button" tabIndex={-1}
    role="option" aria-selected={value === option.value} aria-disabled={option.disabled || undefined}
    className="select-option" disabled={option.disabled}
    onPointerMove={() => setHighlighted(index)} onPointerDown={(event) => event.preventDefault()} onClick={() => choose(index)}>
    <span>{option.label}</span>{value === option.value && <Icon name="check" size={14} />}
  </button>) : null, [presence.mounted, filtered, value, id, choose]);
  const keys = (event: React.KeyboardEvent) => {
    if (event.nativeEvent.isComposing) return;
    if (event.key === "Tab") {
      // Search lives in a body portal; continue the tab order from its original control.
      if (event.target === search.current) trigger.current?.focus();
      close(); return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) reveal();
      else setHighlighted((current) => moveOption(filtered, current, event.key === "ArrowDown" ? 1 : -1));
    } else if (open && (event.key === "Enter" || (event.key === " " && event.target === trigger.current))) {
      event.preventDefault(); choose(highlighted);
    } else if (open && !searchable && (event.key === "Home" || event.key === "End")) {
      event.preventDefault(); setHighlighted(moveOption(filtered, event.key === "Home" ? -1 : 0, event.key === "Home" ? 1 : -1));
    }
  };
  return <div className={`custom-select ${className}`}>
    <button ref={trigger} type="button" className="select-trigger" role="combobox" aria-label={label}
      aria-expanded={open} aria-controls={open ? id : undefined} aria-haspopup="listbox"
      aria-activedescendant={open && !searchable && highlighted >= 0 ? `${id}-${highlighted}` : undefined}
      disabled={disabled} onClick={() => open ? close() : reveal()} onKeyDown={keys}>
      <span>{selected?.label || value || placeholder || label}</span><Icon name="chevron" size={14} />
    </button>
    {presence.mounted && trigger.current && <Popover anchor={trigger.current} onClose={close} open={presence.visible} interactive={open} className="select-popover">
      {searchable && <input ref={search} className="select-search" aria-label={searchLabel || t("搜索选项")}
        role="combobox" aria-autocomplete="list" aria-expanded="true" aria-controls={id}
        aria-activedescendant={highlighted >= 0 ? `${id}-${highlighted}` : undefined}
        placeholder={searchLabel || t("搜索选项")} value={query} onKeyDown={keys}
        onChange={(event) => { setQuery(event.target.value); setHighlighted(moveOption(filterOptions(options, event.target.value), -1, 1)); }} />}
      <div ref={list} id={id} role="listbox" aria-label={label} className="select-options">
        {optionNodes}
        {!filtered.length && <p className="select-empty">{emptyMessage || t("没有匹配的选项")}</p>}
      </div>
    </Popover>}
  </div>;
}
