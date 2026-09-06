import { motion } from "motion/react";
import { useEffect, useRef } from "react";
import type { LogLine } from "../types";

export default function Logs({ logs, onClear }: { logs: LogLine[]; onClear: () => void }) {
  const box = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  useEffect(() => {
    const el = box.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [logs]);

  const onScroll = () => {
    const el = box.current;
    if (!el) return;
    stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  };

  return (
    <div className="screen">
      <div className="head row">
        <div style={{ flex: 1 }}>
          <h1>Журнал</h1>
          <p className="sub">Что говорит winws.exe. Сюда же попадают ошибки запуска.</p>
        </div>
        <button className="btn sm ghost" onClick={() => navigator.clipboard.writeText(logs.map((l) => `${l.time} ${l.text}`).join("\n"))}>
          Скопировать
        </button>
        <button className="btn sm ghost" onClick={onClear}>
          Очистить
        </button>
      </div>

      <div className="logbox" ref={box} onScroll={onScroll}>
        {logs.length === 0 && <div style={{ color: "var(--dim)" }}>Пока пусто — включи обход, и здесь появятся строки.</div>}
        {logs.map((l, i) => (
          <motion.div
            key={i}
            className={`logline ${l.kind}`}
            initial={{ opacity: 0, x: -6 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.18 }}
          >
            <span className="t">{l.time}</span>
            <span className="m">{l.text}</span>
          </motion.div>
        ))}
      </div>
    </div>
  );
}
