import { AnimatePresence, motion } from "motion/react";
import { useMemo, useState } from "react";
import { Crown, Power, Search } from "../icons";
import type { Snapshot } from "../types";

export default function Strategies({
  snap,
  busy,
  onSelect,
  onStart,
}: {
  snap: Snapshot;
  busy: boolean;
  onSelect: (name: string) => void;
  onStart: (name: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<string | null>(null);

  const items = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return snap.strategies;
    return snap.strategies.filter(
      (s) => s.name.toLowerCase().includes(q) || s.tags.some((t) => t.includes(q))
    );
  }, [snap.strategies, query]);

  const best = snap.config.bestStrategy;
  const selected = snap.config.selectedStrategy;

  return (
    <div className="screen">
      <div className="head">
        <h1>Стратегии</h1>
        <p className="sub">
          Каждая стратегия — свой способ обмануть DPI. Выбери одну и включи; какая именно
          работает у твоего провайдера, покажет вкладка «Проверка».
        </p>
      </div>

      <div className="row" style={{ marginBottom: 14, position: "relative" }}>
        <span style={{ position: "absolute", left: 13, color: "var(--dim)", display: "flex" }}>
          <Search />
        </span>
        <input
          className="input"
          style={{ paddingLeft: 38 }}
          placeholder={`Поиск среди ${snap.strategies.length} стратегий…`}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>

      {items.length === 0 && <div className="empty">Ничего не нашлось</div>}

      <div className="list">
        {items.map((s, i) => {
          const isSel = s.name === selected;
          const isRunning = snap.running && snap.current === s.name;
          const expanded = open === s.name;
          return (
            <motion.div
              key={s.file}
              layout
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: Math.min(i * 0.018, 0.3), layout: { type: "spring", stiffness: 380, damping: 34 } }}
              className={`item ${isSel ? "selected" : ""}`}
              style={{ flexDirection: "column", alignItems: "stretch", gap: 0 }}
              onClick={() => onSelect(s.name)}
            >
              <div className="row">
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="row" style={{ gap: 8 }}>
                    <span className="name">{s.name}</span>
                    {best === s.name && (
                      <span className="pill best" style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
                        <Crown /> лучшая
                      </span>
                    )}
                    {isRunning && <span className="pill live">работает</span>}
                  </div>
                  <div className="tags" style={{ marginTop: 6 }}>
                    {s.tags.map((t) => (
                      <span key={t} className="tag">
                        {t}
                      </span>
                    ))}
                    <span
                      className="tag"
                      style={{ cursor: "pointer", borderStyle: "dashed" }}
                      onClick={(e) => {
                        e.stopPropagation();
                        setOpen(expanded ? null : s.name);
                      }}
                    >
                      {expanded ? "скрыть параметры" : "параметры"}
                    </span>
                  </div>
                </div>

                <button
                  className={`btn sm ${isSel ? "primary" : ""}`}
                  disabled={busy}
                  onClick={(e) => {
                    e.stopPropagation();
                    onStart(s.name);
                  }}
                >
                  <Power /> {isRunning ? "Перезапустить" : "Включить"}
                </button>
              </div>

              <AnimatePresence>
                {expanded && (
                  <motion.pre
                    initial={{ opacity: 0, height: 0, marginTop: 0 }}
                    animate={{ opacity: 1, height: "auto", marginTop: 12 }}
                    exit={{ opacity: 0, height: 0, marginTop: 0 }}
                    transition={{ duration: 0.24 }}
                    style={{
                      overflow: "hidden",
                      fontFamily: "Cascadia Mono, Consolas, monospace",
                      fontSize: 11.5,
                      color: "var(--muted)",
                      whiteSpace: "pre-wrap",
                      wordBreak: "break-all",
                      userSelect: "text",
                      lineHeight: 1.6,
                    }}
                  >
                    {s.args.join(" ")}
                  </motion.pre>
                )}
              </AnimatePresence>
            </motion.div>
          );
        })}
      </div>
    </div>
  );
}
