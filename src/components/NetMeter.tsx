import { motion } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, errText } from "../api";
import { Gauge, Refresh } from "../icons";
import { Counter, Spinner } from "./ui";

/** Сколько замеров держим на графике — примерно полторы минуты истории */
const HISTORY = 45;
const PERIOD_MS = 2000;

const color = (ms: number | null) =>
  ms == null ? "var(--bad)" : ms < 80 ? "var(--ok)" : ms < 200 ? "var(--warn)" : "var(--bad)";

/** Ломаная по истории замеров: пропуски рисуем как провал в самый низ */
function Spark({ points }: { points: (number | null)[] }) {
  const w = 100;
  const h = 28;
  if (points.length < 2) return <svg viewBox={`0 0 ${w} ${h}`} className="spark" />;

  // Верх шкалы — худший замер, но не меньше 120 мс, иначе тихая сеть
  // рисуется драматичной пилой на ровном месте
  const worst = Math.max(120, ...points.filter((p): p is number => p != null));
  const step = w / (points.length - 1);
  const y = (p: number | null) => (p == null ? h : h - (Math.min(p, worst) / worst) * (h - 2) - 1);
  const line = points.map((p, i) => `${(i * step).toFixed(1)},${y(p).toFixed(1)}`).join(" ");

  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="spark" preserveAspectRatio="none">
      <polyline points={`0,${h} ${line} ${w},${h}`} className="spark-fill" />
      <polyline points={line} className="spark-line" />
    </svg>
  );
}

export default function NetMeter({ running }: { running: boolean }) {
  const [history, setHistory] = useState<(number | null)[]>([]);
  const [via, setVia] = useState("напрямую");
  const [speed, setSpeed] = useState<number | null>(null);
  const [measuring, setMeasuring] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);

  const tick = useCallback(async () => {
    try {
      const s = await api.ping();
      if (!alive.current) return;
      setVia(s.via);
      setError(null);
      setHistory((prev) => [...prev, s.ms].slice(-HISTORY));
    } catch (e) {
      if (!alive.current) return;
      setError(errText(e));
      setHistory((prev) => [...prev, null].slice(-HISTORY));
    }
  }, []);

  // Меряем, только пока экран открыт: фоновый пинг никому не нужен
  useEffect(() => {
    alive.current = true;
    tick();
    const id = setInterval(tick, PERIOD_MS);
    return () => {
      alive.current = false;
      clearInterval(id);
    };
  }, [tick]);

  // Смена движка меняет путь трафика — старые точки к нему уже не относятся
  useEffect(() => {
    setHistory([]);
    setSpeed(null);
  }, [running]);

  const last = history.length ? history[history.length - 1] : null;
  const seen = history.filter((p): p is number => p != null);
  const avg = seen.length ? Math.round(seen.reduce((a, b) => a + b, 0) / seen.length) : null;
  const lost = history.length ? history.filter((p) => p == null).length : 0;

  const runSpeed = async () => {
    setMeasuring(true);
    try {
      setSpeed(await api.measureSpeed());
      setError(null);
    } catch (e) {
      setSpeed(null);
      setError(errText(e));
    } finally {
      setMeasuring(false);
    }
  };

  return (
    <motion.div
      className="card"
      initial={{ opacity: 0, y: 14 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: 0.08, duration: 0.35 }}
      style={{ marginTop: 14, padding: 15 }}
    >
      <div className="row" style={{ alignItems: "center", gap: 14 }}>
        <div style={{ minWidth: 96 }}>
          <div className="sub" style={{ fontSize: 11.5 }}>Отклик</div>
          <div style={{ display: "flex", alignItems: "baseline", gap: 5 }}>
            <span className="num" style={{ fontSize: 26, fontWeight: 650, color: color(last) }}>
              {last == null ? "—" : <Counter value={last} />}
            </span>
            <span className="sub" style={{ fontSize: 12 }}>мс</span>
          </div>
        </div>

        <div style={{ flex: 1, minWidth: 0 }}>
          <Spark points={history} />
          <div className="sub" style={{ fontSize: 11.5, marginTop: 4 }}>
            {error
              ? error
              : `${via} · среднее ${avg == null ? "—" : `${avg} мс`}${
                  lost ? ` · потеряно ${lost} из ${history.length}` : ""
                }`}
          </div>
        </div>

        <div style={{ textAlign: "right" }}>
          <div className="sub" style={{ fontSize: 11.5 }}>Скорость</div>
          <div style={{ display: "flex", alignItems: "baseline", gap: 5, justifyContent: "flex-end" }}>
            <span className="num" style={{ fontSize: 19, fontWeight: 600 }}>
              {speed == null ? "—" : (speed / 1024).toFixed(1)}
            </span>
            <span className="sub" style={{ fontSize: 12 }}>МБ/с</span>
          </div>
        </div>

        <button className="btn sm ghost" onClick={runSpeed} disabled={measuring} title="Замерить скорость">
          {measuring ? <Spinner /> : speed == null ? <Gauge /> : <Refresh />}
          {measuring ? "Меряю…" : "Замерить"}
        </button>
      </div>
    </motion.div>
  );
}
