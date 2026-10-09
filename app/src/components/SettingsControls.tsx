import { useCallback, useEffect, useId, useRef, useState, type CSSProperties } from "react";
import { t } from "../i18n";
import { Popover } from "./Popover";
import { commitNumberInput, parseNumberInput, stepNumberInput } from "./numberInput";

export function HelpHint({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  const id = useId();
  const container = useRef<HTMLSpanElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const closing = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const ignoreRestoredFocus = useRef(false);
  const [open, setOpen] = useState(false);
  const [pinned, setPinned] = useState(false);
  const cancelClose = useCallback(() => { clearTimeout(closing.current); }, []);
  const close = useCallback(() => {
    clearTimeout(closing.current);
    setOpen(false);
    setPinned(false);
    // The shared popover restores keyboard focus after Escape.
    ignoreRestoredFocus.current = true;
    queueMicrotask(() => { ignoreRestoredFocus.current = false; });
  }, []);
  useEffect(() => () => clearTimeout(closing.current), []);
  const reveal = () => {
    cancelClose();
    if (!trigger.current?.matches(":disabled")) setOpen(true);
  };
  const leave = () => {
    cancelClose();
    if (!pinned && !container.current?.contains(document.activeElement)) {
      closing.current = setTimeout(() => setOpen(false), 120);
    }
  };
  return (
    <span
      ref={container}
      className="help-hint"
      onPointerEnter={reveal}
      onPointerLeave={leave}
      onFocus={() => { if (!ignoreRestoredFocus.current) reveal(); }}
      onBlur={(event) => {
        if (!pinned && !event.currentTarget.contains(event.relatedTarget)) setOpen(false);
      }}
    >
      <button
        ref={trigger}
        type="button"
        className="help-hint-button"
        aria-label={t("{p0}说明", { p0: label })}
        aria-describedby={open ? id : undefined}
        aria-expanded={open}
        onClick={() => {
          cancelClose();
          setPinned(!pinned);
          setOpen(!pinned);
        }}
      >
        <svg viewBox="0 0 24 24" fill="none" aria-hidden="true" focusable="false">
          <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="1.5" />
          <path d="M9.4 9a2.6 2.6 0 0 1 5.2 0c0 1.8-2.6 2-2.6 3.9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          <circle cx="12" cy="16" r=".9" fill="currentColor" />
        </svg>
      </button>
      {open && trigger.current && <Popover anchor={trigger.current} onClose={close}
        matchWidth={false} className="help-hint-popover" id={id} role="tooltip">
        <span className="help-hint-content" onPointerEnter={cancelClose} onPointerLeave={leave}>{children}</span>
      </Popover>}
    </span>
  );
}

export function ScaleField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
}) {
  return (
    <div className="scale-field">
      <input
        type="range"
        aria-label={t("{p0}滑块", { p0: label })}
        min={50}
        max={300}
        step={1}
        value={Math.round(value * 100)}
        style={{ "--range-progress": `${Math.max(0, Math.min(100, (value * 100 - 50) / 2.5))}%` } as CSSProperties}
        onChange={(e) => onChange(Number(e.target.value) / 100)}
      />
      <NumberField
        label={label}
        value={Math.round(value * 100)}
        min={50}
        max={300}
        onChange={(v) => onChange(v / 100)}
        suffix="%"
      />
    </div>
  );
}
export function SettingRow({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="setting-row">
      <div className="setting-label">
        <strong>{title}</strong>
        {description && <HelpHint label={title}>{description}</HelpHint>}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}
export function NumberField({
  label,
  value,
  min,
  max,
  suffix = t("首"),
  decimal = false,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  suffix?: string;
  decimal?: boolean;
  onChange: (v: number) => void;
}) {
  const [text, setText] = useState(String(value));
  useEffect(() => {
    setText((current) => parseNumberInput(current, decimal) === value ? current : String(value));
  }, [value, decimal]);
  const commit = () => {
    const accepted = commitNumberInput(text, value, min, max, decimal);
    setText(String(accepted));
    onChange(accepted);
  };
  const step = (direction: 1 | -1) => {
    const accepted = stepNumberInput(commitNumberInput(text, value, min, max, decimal), direction, min, max);
    setText(String(accepted));
    onChange(accepted);
  };
  return (
    <div className="number-field">
      <input
        type={decimal ? "text" : "number"}
        inputMode={decimal ? "decimal" : "numeric"}
        role="spinbutton"
        aria-label={label}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={value}
        min={min}
        max={max}
        step={decimal ? "any" : 1}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          const candidate = parseNumberInput(e.target.value, decimal);
          if (candidate !== null && candidate >= min && candidate <= max)
            onChange(candidate);
        }}
        onKeyDown={(event) => {
          if (!event.nativeEvent.isComposing && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
            event.preventDefault();
            step(event.key === "ArrowUp" ? 1 : -1);
          }
        }}
        onBlur={commit}
      />
      {suffix && <span>{suffix}</span>}
      <div className="number-steppers">
        <button type="button" tabIndex={-1} aria-label={t("增加 {p0}", { p0: label })}
          disabled={value >= max} onMouseDown={(event) => event.preventDefault()} onClick={() => step(1)}>
          <svg viewBox="0 0 12 12" fill="none" aria-hidden="true"><path d="m3 7 3-3 3 3" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" /></svg>
        </button>
        <button type="button" tabIndex={-1} aria-label={t("减少 {p0}", { p0: label })}
          disabled={value <= min} onMouseDown={(event) => event.preventDefault()} onClick={() => step(-1)}>
          <svg viewBox="0 0 12 12" fill="none" aria-hidden="true"><path d="m3 5 3 3 3-3" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" /></svg>
        </button>
      </div>
    </div>
  );
}
export function Toggle({
  label,
  value,
  onChange,
}: {
  label: string;
  value: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={value}
      aria-label={label}
      className={`toggle ${value ? "on" : ""}`}
      onClick={() => onChange(!value)}
    >
      <span />
    </button>
  );
}
