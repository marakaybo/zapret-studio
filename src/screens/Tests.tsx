import { AnimatePresence, motion } from "motion/react";
import { useMemo, useState } from "react";
import { Alert, Check, Cog, Cross, Crown, Gauge, Plus, Trash } from "../icons";
import { Counter, Spinner, Switch } from "../components/ui";
import { isCore } from "../engines";
import type { PresetEngine, Snapshot, StrategyResult, TestStage } from "../types";

/** Свои сайты: список того, что проверять вдобавок к встроенным целям. */
function OwnTargets({
  snap,
  busy,
  onSave,
}: {
  snap: Snapshot;
  busy: boolean;
  onSave: (items: string[]) => void;
}) {
  const [draft, setDraft] = useState("");
  // В снимке лежит итоговый список: свои из настроек плюс utils/targets.txt
  const items = snap.checkTargets;
  // Строки из targets.txt редактировать здесь нельзя — они живут в файле
  const own = snap.config.customTargets;

  const add = () => {
    const value = draft.trim();
    if (!value || items.includes(value)) {
      setDraft("");
      return;
    }
    onSave([...own, value]);
    setDraft("");
  };

  return (
    <div>
      <div className="row" style={{ marginBottom: 6 }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontWeight: 600 }}>Свои сайты</div>
          <div className="sub" style={{ fontSize: 12, marginTop: 2 }}>
            К встроенным целям добавятся твои. Пиши{" "}
            <span className="mono">example.com</span>, ссылку целиком или{" "}
            <span className="mono">host:443</span>. Что ответит чужой сайт, приложение знать не
            может, поэтому засчитывает сам факт ответа — и отдельно узнаёт страницу блокировки
          </div>
        </div>
      </div>

      <div className="server-row">
        <input
          className="input mono"
          spellCheck={false}
          placeholder="example.com"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
        />
        <button className="btn sm" onClick={add} disabled={busy || !draft.trim()}>
          {busy ? <Spinner /> : <Plus />} Добавить
        </button>
      </div>

      {items.length > 0 && (
        <div className="row" style={{ gap: 6, flexWrap: "wrap", marginTop: 10 }}>
          {items.map((t) => {
            const fromFile = !own.includes(t);
            return (
              <span
                key={t}
                className="pill"
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 6,
                  border: "1px solid var(--line-strong)",
                  fontFamily: "inherit",
                }}
                title={fromFile ? "из utils/targets.txt — правится в файле" : undefined}
              >
                <span className="mono" style={{ fontSize: 11 }}>
                  {t}
                </span>
                {fromFile ? (
                  <span className="sub" style={{ fontSize: 11 }}>
                    из файла
                  </span>
                ) : (
                  <button
                    className="btn sm ghost"
                    style={{ padding: 2, minWidth: 0 }}
                    title="Убрать"
                    disabled={busy}
                    onClick={() => onSave(own.filter((x) => x !== t))}
                  >
                    <Trash />
                  </button>
                )}
              </span>
            );
          })}
        </div>
      )}
    </div>
  );
}

const scoreColor = (v: number) => (v >= 90 ? "var(--ok)" : v >= 55 ? "var(--warn)" : "var(--bad)");

function GroupBadges({ r }: { r: StrategyResult }) {
  return (
    <div className="groups">
      {r.groups.map((g) => (
        <span
          key={g.group}
          className={`gbadge ${g.ok === g.total ? "full" : g.ok === 0 ? "none" : "part"}`}
        >
          {g.group} {g.ok}/{g.total}
        </span>
      ))}
    </div>
  );
}

function ResultCard({
  r,
  rank,
  isBest,
  title,
  onApply,
}: {
  r: StrategyResult;
  rank: number;
  isBest: boolean;
  /** У zapret это имя батника, у ByeDPI — название пресета */
  title: string;
  onApply: (name: string) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <motion.div layout className="result" transition={{ type: "spring", stiffness: 340, damping: 32 }}>
      <div className="result-head" onClick={() => setOpen((v) => !v)}>
        <span className={`rank ${isBest ? "top" : ""}`}>{isBest ? <Crown /> : rank}</span>

        <div style={{ minWidth: 0 }}>
          <div className="row" style={{ gap: 8 }}>
            <span style={{ fontWeight: 550, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {title}
            </span>
            {!r.started && <span className="pill" style={{ color: "var(--bad)" }}>не запустилась</span>}
          </div>
          <div className="result-meta">
            <div className="score-bar">
              <motion.div
                className="score-fill"
                initial={{ width: 0 }}
                animate={{ width: `${r.score}%` }}
                transition={{ duration: 0.7, ease: [0.22, 1, 0.36, 1] }}
                style={{ background: scoreColor(r.score) }}
              />
            </div>
            <span className="num score-num" style={{ color: scoreColor(r.score) }}>
              <Counter value={r.score} decimals={0} suffix="%" />
            </span>
            <span className="result-nums">
              {r.avgMs != null ? `${r.avgMs} мс` : "—"}
              {r.speedKbs != null ? ` · ${(r.speedKbs / 1024).toFixed(1)} МБ/с` : ""}
            </span>
          </div>
        </div>

        <GroupBadges r={r} />

        <button
          className="btn sm"
          onClick={(e) => {
            e.stopPropagation();
            onApply(r.strategy);
          }}
          disabled={!r.started}
        >
          Включить
        </button>
      </div>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.25 }}
            style={{ overflow: "hidden" }}
          >
            <div className="targets">
              {r.error && (
                <div style={{ gridColumn: "1 / -1", color: "var(--bad)", fontSize: 12 }}>{r.error}</div>
              )}
              {r.targets.map((t) => (
                <div className="target" key={t.id}>
                  <span style={{ color: t.ok ? "var(--ok)" : "var(--bad)", display: "flex" }}>
                    {t.ok ? <Check /> : <Cross />}
                  </span>
                  <span className="tl">{t.label}</span>
                  <span className="tr" style={{ color: t.ok ? "var(--muted)" : "var(--bad)" }}>
                    {t.ok ? `${t.ms} мс` : t.error}
                  </span>
                </div>
              ))}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
}

export default function Tests({
  snap,
  results,
  stage,
  running,
  busy,
  onRun,
  onCancel,
  onApply,
  onSaveTargets,
}: {
  snap: Snapshot;
  results: StrategyResult[];
  stage: TestStage | null;
  running: boolean;
  busy: boolean;
  onRun: (names: string[], baseline: boolean) => void;
  onCancel: () => void;
  onApply: (name: string) => void;
  onSaveTargets: (items: string[]) => void;
}) {
  // Все движки проверяются одинаково, различаются только источник списка
  // и то, что уходит на бэкенд: имя батника или id пресета. Ядра сюда
  // забыли добавить, когда они появились, — и экран показывал стратегии
  // zapret, хотя выбран был Xray: проверка уходила в пустоту.
  const presets: PresetEngine | null = isCore(snap.engine)
    ? snap[snap.engine]
    : snap.engine === "byedpi"
      ? snap.byedpi
      : snap.engine === "goodbyedpi"
        ? snap.goodbye
        : null;
  const items = presets
    ? presets.presets.map((p) => ({ key: p.id, id: p.id, label: p.name }))
    : snap.strategies.map((s) => ({ key: s.file, id: s.name, label: s.label }));
  const all = items.map((i) => i.id);
  const currentId = presets ? presets.selected : snap.config.selectedStrategy;
  const titleOf = (r: StrategyResult) => (presets ? r.label : r.strategy);
  const [picked, setPicked] = useState<string[]>([]);
  const [baseline, setBaseline] = useState(true);
  // Настройка проверки свёрнута: человек приходит сюда нажать «запустить»,
  // а не выбирать, что проверять. Раньше выбор занимал весь первый экран,
  // и результаты — то, ради чего всё затевалось, — начинались за прокруткой
  const [tuning, setTuning] = useState(false);

  const selection = picked.length ? picked : all;
  const estimate = Math.round(((selection.length + (baseline ? 1 : 0)) * 9.5) / 60);

  const baselineResult = results.find((r) => r.baseline);
  const ranked = useMemo(
    () => results.filter((r) => !r.baseline).sort((a, b) => b.score - a.score || (a.avgMs ?? 9999) - (b.avgMs ?? 9999)),
    [results]
  );
  const bestName = ranked.find((r) => r.started)?.strategy;

  const toggle = (name: string) =>
    setPicked((prev) => (prev.includes(name) ? prev.filter((n) => n !== name) : [...prev, name]));

  return (
    <div className="screen">
      <div className="head">
        <h1>{presets ? "Проверка пресетов" : "Проверка стратегий"}</h1>
        <p className="sub">
          {presets && snap.engine !== "goodbyedpi"
            ? "Поднимаю по очереди и хожу к Discord, YouTube и Google через сам прокси — меряю ровно то, что получит браузер."
            : "Включаю по очереди и стучусь к Discord, YouTube и Google по-настоящему. Прошло рукопожатие TLS — значит, DPI обойдён."}{" "}
          Результат честен на момент проверки: провайдер меняет фильтрацию, так что при проблемах
          прогоняй заново.
        </p>
      </div>

      {snap.warp.connected && (
        <motion.div
          className="notice warn"
          initial={{ opacity: 0, y: -8 }}
          animate={{ opacity: 1, y: 0 }}
          style={{ marginBottom: 11 }}
        >
          <span className="n-icon">
            <Alert />
          </span>
          <div className="n-body">
            <div className="n-title">Подключён Cloudflare WARP</div>
            <div className="n-text">
              Проверки тоже идут через туннель — результат покажет качество WARP, а не обхода
            </div>
          </div>
        </motion.div>
      )}

      <div className="card" style={{ marginBottom: 11, padding: 12 }}>
        <div className="row">
          <span className="rank" style={{ flex: "none" }}>
            <Gauge />
          </span>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontWeight: 600 }}>
              {picked.length
                ? `Выбрано ${picked.length} из ${all.length}`
                : `${presets ? "Все пресеты" : "Все стратегии"} — ${all.length}`}
            </div>
            <div className="sub">
              Примерно {estimate < 1 ? "меньше минуты" : `${estimate} мин`}
              {baseline ? " · со сравнением «без обхода»" : ""} · интернет будет подмигивать, это
              нормально
            </div>
          </div>
          <button
            className={`btn sm ${tuning ? "" : "ghost"}`}
            onClick={() => setTuning((v) => !v)}
            disabled={running}
          >
            <Cog /> Что проверять
          </button>
          {running ? (
            <button className="btn danger" onClick={onCancel}>
              Остановить
            </button>
          ) : (
            <button
              className="btn primary"
              onClick={() => onRun(selection, baseline)}
              disabled={!all.length}
            >
              <Gauge /> Запустить
            </button>
          )}
        </div>
      </div>

      <AnimatePresence>
        {tuning && (
          <motion.div
            className="card"
            initial={{ opacity: 0, height: 0, marginBottom: 0 }}
            animate={{ opacity: 1, height: "auto", marginBottom: 11 }}
            exit={{ opacity: 0, height: 0, marginBottom: 0 }}
            style={{ overflow: "hidden" }}
          >
            <div className="row" style={{ gap: 10, flexWrap: "wrap", marginBottom: 10 }}>
              <button className="btn sm ghost" onClick={() => setPicked([])} disabled={running}>
                Все
              </button>
              <button
                className="btn sm ghost"
                onClick={() => setPicked(currentId ? [currentId] : [])}
                disabled={running}
              >
                Только текущая
              </button>
              <div style={{ flex: 1 }} />
              <div className="row" style={{ gap: 9 }}>
                <span className="sub">Сравнить с «без обхода»</span>
                <Switch on={baseline} onChange={setBaseline} disabled={running} />
              </div>
            </div>

            <div className="tags" style={{ marginBottom: 14 }}>
              {items.map((s) => {
                const on = picked.includes(s.id);
                return (
                  <motion.span
                    key={s.key}
                    className={`tag ${on ? "on" : ""}`}
                    whileTap={{ scale: 0.94 }}
                    onClick={() => !running && toggle(s.id)}
                    style={{ cursor: running ? "default" : "pointer" }}
                  >
                    {s.label}
                  </motion.span>
                );
              })}
            </div>

            <OwnTargets snap={snap} busy={busy} onSave={onSaveTargets} />
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {stage && running && (
          <motion.div
            className="card"
            initial={{ opacity: 0, y: -8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -8 }}
            style={{ marginBottom: 16 }}
          >
            <div className="row" style={{ marginBottom: 10 }}>
              <Spinner />
              <span style={{ fontWeight: 550 }}>
                {stage.phase === "start" ? "Поднимаю" : "Проверяю"} «{stage.strategy}»
              </span>
              <span className="sub" style={{ marginLeft: "auto", fontSize: 12 }}>
                {stage.index + 1} из {stage.total}
              </span>
            </div>
            <div className="progress">
              <motion.div
                className="fill"
                animate={{ width: `${((stage.index + (stage.phase === "probe" ? 0.6 : 0.1)) / stage.total) * 100}%` }}
                transition={{ duration: 0.5 }}
              />
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {baselineResult && (
        <motion.div
          className="card"
          layout
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          style={{ marginBottom: 14, padding: 14 }}
        >
          <div className="row">
            <span className="rank">0</span>
            <div style={{ flex: 1 }}>
              <div style={{ fontWeight: 550 }}>Без обхода</div>
              <div className="sub" style={{ fontSize: 12 }}>
                Точка отсчёта: столько работает, если обход выключен
              </div>
            </div>
            <span style={{ color: scoreColor(baselineResult.score), fontWeight: 650 }}>
              <Counter value={baselineResult.score} suffix="%" />
            </span>
            <GroupBadges r={baselineResult} />
          </div>
        </motion.div>
      )}

      <div className="list">
        <AnimatePresence>
          {ranked.map((r, i) => (
            <ResultCard
              key={r.strategy}
              r={r}
              rank={i + 1}
              isBest={r.strategy === bestName}
              title={titleOf(r)}
              onApply={onApply}
            />
          ))}
        </AnimatePresence>
      </div>

      {!results.length && !running && (
        <div className="empty">
          Результатов пока нет. Запусти проверку — и увидишь, что реально работает.
        </div>
      )}
    </div>
  );
}
