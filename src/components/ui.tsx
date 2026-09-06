import { AnimatePresence, motion } from "motion/react";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Alert, Check, Cross } from "../icons";

/* ------------------------------------------------------------------ тосты */

export type ToastKind = "ok" | "err" | "info";
type Toast = { id: number; kind: ToastKind; text: string };

const ToastCtx = createContext<(kind: ToastKind, text: string) => void>(() => {});
export const useToast = () => useContext(ToastCtx);

export function ToastHost({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<Toast[]>([]);

  const push = useCallback((kind: ToastKind, text: string) => {
    const id = Date.now() + Math.random();
    setItems((prev) => [...prev, { id, kind, text }]);
    setTimeout(() => setItems((prev) => prev.filter((t) => t.id !== id)), kind === "err" ? 7000 : 4000);
  }, []);

  const value = useMemo(() => push, [push]);

  return (
    <ToastCtx.Provider value={value}>
      {children}
      <div className="toasts">
        <AnimatePresence>
          {items.map((t) => (
            <motion.div
              key={t.id}
              className={`toast ${t.kind}`}
              initial={{ opacity: 0, x: 40, scale: 0.94 }}
              animate={{ opacity: 1, x: 0, scale: 1 }}
              exit={{ opacity: 0, x: 40, scale: 0.94 }}
              transition={{ type: "spring", stiffness: 420, damping: 32 }}
            >
              <div className="bar" />
              <div style={{ display: "flex", gap: 9, alignItems: "flex-start" }}>
                <span style={{ marginTop: 1, color: t.kind === "err" ? "var(--bad)" : t.kind === "ok" ? "var(--ok)" : "var(--accent-2)" }}>
                  {t.kind === "err" ? <Alert /> : <Check />}
                </span>
                <span style={{ fontSize: 13, lineHeight: 1.45 }}>{t.text}</span>
              </div>
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </ToastCtx.Provider>
  );
}

/* --------------------------------------------------------------- элементы */

export function Switch({ on, onChange, disabled }: { on: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <div
      className={`switch ${on ? "on" : ""}`}
      style={{ opacity: disabled ? 0.4 : 1, pointerEvents: disabled ? "none" : "auto", justifyContent: on ? "flex-end" : "flex-start" }}
      onClick={() => onChange(!on)}
    >
      <motion.div layout className="knob" transition={{ type: "spring", stiffness: 620, damping: 34 }} />
    </div>
  );
}

/** id обязателен: без него подсветка «перелетает» между разными переключателями. */
export function Segmented<T extends string>({
  id,
  value,
  options,
  onChange,
  disabled,
}: {
  id: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  disabled?: boolean;
}) {
  return (
    <div className="seg" style={disabled ? { opacity: 0.45, pointerEvents: "none" } : undefined}>
      {options.map((o) => (
        <button key={o.value} className={o.value === value ? "on" : ""} onClick={() => onChange(o.value)}>
          {o.value === value && (
            <motion.span
              layoutId={`seg-bg-${id}`}
              className="seg-bg"
              transition={{ type: "spring", stiffness: 480, damping: 38 }}
            />
          )}
          <span style={{ position: "relative" }}>{o.label}</span>
        </button>
      ))}
    </div>
  );
}

/** Переключатель движков: тот же вид, что у Segmented, но кнопки переносятся
 *  на вторую строку — пять названий в один ряд уже не влезают. */
export function EnginePicker<T extends string>({
  id,
  value,
  options,
  onChange,
  disabled,
}: {
  id: string;
  value: T;
  options: { value: T; label: string; title?: string }[];
  onChange: (v: T) => void;
  disabled?: boolean;
}) {
  return (
    <div className="engine-pick" style={disabled ? { opacity: 0.45, pointerEvents: "none" } : undefined}>
      {options.map((o) => (
        <button
          key={o.value}
          className={o.value === value ? "on" : ""}
          title={o.title}
          onClick={() => onChange(o.value)}
        >
          {o.value === value && (
            <motion.span
              layoutId={`engine-bg-${id}`}
              className="seg-bg"
              transition={{ type: "spring", stiffness: 480, damping: 38 }}
            />
          )}
          <span style={{ position: "relative" }}>{o.label}</span>
        </button>
      ))}
    </div>
  );
}

export function Spinner() {
  return (
    <motion.span
      className="spinner"
      animate={{ rotate: 360 }}
      transition={{ repeat: Infinity, duration: 0.8, ease: "linear" }}
      style={{ display: "inline-block" }}
    />
  );
}

export function CloseButton({ onClick }: { onClick: () => void }) {
  return (
    <button className="btn ghost sm" onClick={onClick}>
      <Cross />
    </button>
  );
}

/* Плавно «докручивает» число до нового значения — мелочь, а глаз радует. */
export function Counter({ value, decimals = 0, suffix = "" }: { value: number; decimals?: number; suffix?: string }) {
  const [shown, setShown] = useState(value);
  const from = useRef(value);
  useEffect(() => {
    const start = performance.now();
    const begin = from.current;
    let raf = 0;
    const tick = (now: number) => {
      const p = Math.min(1, (now - start) / 550);
      const eased = 1 - Math.pow(1 - p, 3);
      const next = begin + (value - begin) * eased;
      setShown(next);
      from.current = next;
      if (p < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [value]);
  return (
    <span className="num">
      {shown.toFixed(decimals)}
      {suffix}
    </span>
  );
}
