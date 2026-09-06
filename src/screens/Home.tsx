import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { Alert, Crown, Download, Folder, Gauge, Globe, Power, Refresh } from "../icons";
import { EnginePicker, Segmented, Spinner, Switch } from "../components/ui";
import NetMeter from "../components/NetMeter";
import { ENGINES, engineInfo, isCore } from "../engines";
import type {
  Engine,
  Health,
  IpsetMode,
  PresetEngine,
  Snapshot,
  StrategyResult,
  UpdateCheck,
  WarpProbe,
} from "../types";

export default function Home({
  snap,
  update,
  busy,
  results,
  onToggle,
  onUpdate,
  onOpenFolder,
  onGoTests,
  onStopConflicts,
  onGameFilter,
  onIpsetMode,
  onEngine,
  onSystemProxy,
  onStopStray,
  onWarp,
  onWarpCheck,
  onInstallWarp,
  alarm,
  onHealthCheck,
  zapretMissing,
  onPickFolder,
  onInstallFresh,
}: {
  snap: Snapshot;
  update: UpdateCheck | null;
  busy: boolean;
  results: StrategyResult[];
  onToggle: () => void;
  onUpdate: () => void;
  onOpenFolder: () => void;
  onGoTests: () => void;
  onStopConflicts: () => void;
  onGameFilter: (mode: string) => void;
  onIpsetMode: (mode: IpsetMode) => void;
  onEngine: (engine: Engine) => void;
  onSystemProxy: (enable: boolean) => void;
  onStopStray: () => void;
  onWarp: (connect: boolean) => void;
  onWarpCheck: () => Promise<WarpProbe>;
  onInstallWarp: () => void;
  /// Последняя тревога сторожа — null, пока всё хорошо
  alarm: Health | null;
  onHealthCheck: () => void;
  /// Выбран zapret, но его папки нет — обход не запустится
  zapretMissing: boolean;
  onPickFolder: () => void;
  onInstallFresh: () => void;
}) {
  const running = snap.running;
  const bye = snap.byedpi;
  const byedpi = snap.engine === "byedpi";
  const goodbye = snap.engine === "goodbyedpi";
  // Xray и sing-box настраиваются одинаково, поэтому дальше они идут одним «core»
  const core = isCore(snap.engine) ? snap[snap.engine] : null;
  const view: PresetEngine | null = byedpi ? snap.byedpi : goodbye ? snap.goodbye : core;
  // Локальный прокси — ByeDPI и оба ядра: у них есть порт и системный прокси
  const proxy = core ?? (byedpi ? bye : null);
  const info = engineInfo(snap.engine);
  const [probe, setProbe] = useState<WarpProbe | null>(null);
  const [probing, setProbing] = useState(false);

  const checkWarp = async () => {
    setProbing(true);
    try {
      setProbe(await onWarpCheck());
    } finally {
      setProbing(false);
    }
  };
  const current = view
    ? (snap.current ?? view.presets.find((p) => p.id === view.selected)?.name ?? "не выбран")
    : (snap.current ?? snap.config.selectedStrategy ?? "не выбрана");
  const svc = snap.service;
  const best = results.filter((r) => !r.baseline && r.started).sort((a, b) => b.score - a.score)[0];
  // У пресетов в результате лежит id, а показывать надо название
  const bestTitle = best ? (view ? best.label : best.strategy) : "";

  const stats = core
    ? [
        { k: "Пресет", v: current },
        { k: `Версия ${info.label}`, v: core.version ?? "неизвестна" },
        {
          k: "Свой сервер",
          v: core.serverError ? `ссылка с ошибкой: ${core.serverError}` : (core.server ?? "не задан — работает только фрагментация"),
        },
        { k: "Последняя проверка", v: core.lastTestAt ?? "ещё не было" },
      ]
    : byedpi
    ? [
        { k: "Пресет", v: current },
        { k: "Версия ByeDPI", v: bye.version ?? "неизвестна" },
        {
          k: "Куда идёт трафик",
          v: bye.systemProxyActive ? `системный прокси → 127.0.0.1:${bye.port}` : "вручную, через SOCKS5",
        },
        { k: "Последняя проверка", v: bye.lastTestAt ?? "ещё не было" },
      ]
    : goodbye
    ? [
        { k: "Пресет", v: current },
        { k: "Версия GoodbyeDPI", v: snap.goodbye.version ?? "неизвестна" },
        {
          k: "Список заблокированного",
          v: snap.goodbye.blacklist != null ? `${snap.goodbye.blacklist} доменов` : "не найден",
        },
        { k: "Последняя проверка", v: snap.goodbye.lastTestAt ?? "ещё не было" },
      ]
    : [
        { k: "Стратегия", v: current },
        { k: "Версия zapret", v: snap.config.installedVersion ?? "неизвестна" },
        {
          k: "Автозапуск обхода",
          v: svc.installed ? (svc.running ? "служба работает" : "служба установлена") : "выключен",
        },
        { k: "Последняя проверка", v: snap.config.lastTestAt ?? "ещё не было" },
      ];

  return (
    <div className="screen">
      <AnimatePresence>
        {zapretMissing && (
          <motion.div key="no-zapret" className="notice warn"
            initial={{ opacity: 0, y: -10, height: 0, marginBottom: 0 }}
            animate={{ opacity: 1, y: 0, height: "auto", marginBottom: 11 }}
            exit={{ opacity: 0, y: -10, height: 0, marginBottom: 0 }}
          >
            <span className="n-icon">
              <Alert />
            </span>
            <div className="n-body">
              <div className="n-title">Папки zapret нет на месте</div>
              <div className="n-text">
                Файлов не найти — папку удалили или перенесли. Остальные движки работают
              </div>
            </div>
            <button className="btn sm ghost" onClick={onPickFolder} disabled={busy}>
              <Folder /> Указать
            </button>
            <button className="btn sm" onClick={onInstallFresh} disabled={busy}>
              {busy ? <Spinner /> : <Download />} Скачать
            </button>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {alarm && (
          <motion.div key="alarm" className="notice bad"
            initial={{ opacity: 0, y: -10, height: 0, marginBottom: 0 }}
            animate={{ opacity: 1, y: 0, height: "auto", marginBottom: 11 }}
            exit={{ opacity: 0, y: -10, height: 0, marginBottom: 0 }}
          >
            <span className="n-icon">
              <Alert />
            </span>
            <div className="n-body">
              <div className="n-title">Обход перестал пробивать</div>
              <div className="n-text">
                В {alarm.checkedAt} не достучался: {alarm.failed.join(", ")}. Интернет на месте —
                похоже, провайдер поменял фильтрацию
              </div>
            </div>
            <button className="btn sm ghost" onClick={onHealthCheck} disabled={busy}>
              <Refresh /> Ещё раз
            </button>
            <button className="btn sm danger" onClick={onGoTests} disabled={busy}>
              <Gauge /> Проверить
            </button>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {snap.conflicts.length > 0 && (
          <motion.div key="conflicts" className="notice bad"
            initial={{ opacity: 0, y: -10, height: 0, marginBottom: 0 }}
            animate={{ opacity: 1, y: 0, height: "auto", marginBottom: 11 }}
            exit={{ opacity: 0, y: -10, height: 0, marginBottom: 0 }}
          >
            <span className="n-icon">
              <Alert />
            </span>
            <div className="n-body">
              <div className="n-title">Работает другой обход блокировок</div>
              <div className="n-text">
                {snap.conflicts
                  .map((c) => (c.kind === "service" ? `служба ${c.name}` : c.name))
                  .join(", ")}{" "}
                — делит драйвер с zapret, и проверка стратегий врёт
              </div>
            </div>
            <button className="btn sm danger" onClick={onStopConflicts} disabled={busy}>
              {busy ? <Spinner /> : null} Остановить
            </button>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {snap.stray && (
          <motion.div key="stray" className="notice warn"
            initial={{ opacity: 0, y: -10, height: 0, marginBottom: 0 }}
            animate={{ opacity: 1, y: 0, height: "auto", marginBottom: 11 }}
            exit={{ opacity: 0, y: -10, height: 0, marginBottom: 0 }}
          >
            <span className="n-icon">
              <Alert />
            </span>
            <div className="n-body">
              <div className="n-title">Работает не тот движок: {snap.stray}</div>
              <div className="n-text">
                Остался от прошлого запуска и правит те же пакеты — проверка меряет его
              </div>
            </div>
            <button className="btn sm" onClick={onStopStray} disabled={busy}>
              {busy ? <Spinner /> : null} Остановить
            </button>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {update?.hasUpdate && (
          <motion.div key="update" className="notice info"
            initial={{ opacity: 0, y: -10, height: 0, marginBottom: 0 }}
            animate={{ opacity: 1, y: 0, height: "auto", marginBottom: 11 }}
            exit={{ opacity: 0, y: -10, height: 0, marginBottom: 0 }}
          >
            <span className="n-icon">
              <Download />
            </span>
            <div className="n-body">
              <div className="n-title">Вышла новая версия zapret {update.latest}</div>
              <div className="n-text">
                Установлена {update.current ?? "—"}. Обновлю за пару секунд и сохраню твои списки
              </div>
            </div>
            <button className="btn sm primary" onClick={onUpdate} disabled={busy}>
              {busy ? <Spinner /> : <Download />} Обновить
            </button>
          </motion.div>
        )}
      </AnimatePresence>

      <motion.div
        className="card"
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        style={{ marginBottom: 18, padding: 14 }}
      >
        <div className="row">
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontWeight: 600 }}>Движок обхода</div>
            <div className="sub" style={{ fontSize: 12, marginTop: 2 }}>
              {info.short}
            </div>
          </div>
          <EnginePicker
            id="engine"
            value={snap.engine}
            onChange={onEngine}
            disabled={busy}
            options={ENGINES.map((e) => ({ value: e.id as Engine, label: e.label, title: e.when }))}
          />
        </div>
      </motion.div>

      <div className="power-wrap">
        <div className="power">
          <AnimatePresence>
            {running && (
              <motion.div
                className="power-halo"
                initial={{ opacity: 0, scale: 0.8 }}
                animate={{ opacity: [0.55, 0.95, 0.55], scale: [1, 1.06, 1] }}
                exit={{ opacity: 0, scale: 0.8 }}
                transition={{ duration: 3.2, repeat: Infinity, ease: "easeInOut" }}
              />
            )}
          </AnimatePresence>

          <motion.div
            className="power-ring"
            animate={running ? { rotate: 360 } : { rotate: 0 }}
            transition={running ? { duration: 9, repeat: Infinity, ease: "linear" } : { duration: 0.4 }}
            style={{ opacity: running ? 1 : 0.35 }}
          />

          <motion.div
            className="power-core"
            onClick={busy ? undefined : onToggle}
            whileHover={busy ? {} : { scale: 1.03 }}
            whileTap={busy ? {} : { scale: 0.96 }}
            transition={{ type: "spring", stiffness: 420, damping: 26 }}
          >
            <motion.div
              animate={{ color: running ? "var(--ok)" : "var(--dim)" }}
              transition={{ duration: 0.35 }}
              style={{ display: "grid", placeItems: "center", gap: 8 }}
            >
              {busy ? <Spinner /> : <Power />}
              <span className="power-label" style={{ color: running ? "var(--ok)" : "var(--muted)" }}>
                {busy ? "минуту…" : running ? "ВКЛЮЧЕНО" : "ВЫКЛЮЧЕНО"}
              </span>
            </motion.div>
          </motion.div>
        </div>

        <div className="status-line">
          <motion.span
            className={`dot ${running ? "on" : "off"}`}
            animate={running ? { scale: [1, 1.25, 1] } : { scale: 1 }}
            transition={{ duration: 2, repeat: running ? Infinity : 0 }}
          />
          <span style={{ color: running ? "var(--text)" : "var(--muted)" }}>
            {!running
              ? "Обход выключен — Discord и YouTube идут напрямую"
              : proxy
                ? proxy.systemProxyActive
                  ? `${info.label} «${current}» · весь трафик через 127.0.0.1:${proxy.port}`
                  : `${info.label} «${current}» · прокси на 127.0.0.1:${proxy.port}, трафик надо направить самому`
                : goodbye
                  ? `Работает GoodbyeDPI «${current}»`
                  : svc.running
                    ? `Работает служба · ${svc.strategy ?? current}`
                    : `Работает стратегия «${current}»`}
          </span>
        </div>

        <div className="row" style={{ marginTop: 18, gap: 10 }}>
          <button className="btn sm" onClick={onGoTests}>
            <Gauge /> {view ? "Проверить пресеты" : "Проверить стратегии"}
          </button>
          {running && (
            <button
              className="btn sm ghost"
              onClick={onHealthCheck}
              disabled={busy}
              title="Быстрый осмотр: пробивает ли обход прямо сейчас"
            >
              <Refresh /> Пробивает?
            </button>
          )}
          {!view && (
            <button className="btn sm ghost" onClick={onOpenFolder}>
              <Folder /> Папка zapret
            </button>
          )}
        </div>
      </div>

      <NetMeter running={running} />

      <motion.div
        className="card"
        initial={{ opacity: 0, y: 14 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1, duration: 0.35 }}
        style={{ marginTop: 14, padding: 15 }}
      >
        <div className="row">
          <span
            className={`dot ${
              !snap.warp.installed ? "off" : (probe?.active ?? snap.warp.connected) ? "on" : "off"
            }`}
          />
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontWeight: 550 }}>
              Cloudflare WARP —{" "}
              {!snap.warp.installed
                ? "не установлен"
                : snap.warp.connected
                  ? "подключён"
                  : "отключён"}
            </div>
            <div className="sub" style={{ fontSize: 12, marginTop: 2 }}>
              {!snap.warp.installed
                ? "Это отдельная программа Cloudflare, приложение её не ставит. Пока её нет, переключателя и не будет — скачай, и он появится сам"
                : snap.warp.connected
                  ? "Весь трафик идёт через туннель, включая проверку стратегий — она покажет качество WARP, а не обхода"
                  : "В России его серверы часто режут: если не подключается, включи обход — он чинит Cloudflare"}
            </div>
            {probe && (
              <div
                style={{
                  fontSize: 12,
                  marginTop: 4,
                  color: probe.error ? "var(--warn)" : probe.active ? "var(--ok)" : "var(--muted)",
                }}
              >
                {probe.error
                  ? probe.error
                  : probe.active
                    ? `Туннель работает: Cloudflare видит нас из ${probe.loc || "?"} (${probe.ip}), узел ${probe.colo}`
                    : `Туннеля нет: Cloudflare видит обычный адрес ${probe.ip} из ${probe.loc || "?"}`}
                <span className="sub"> · {probe.checkedAt}</span>
              </div>
            )}
          </div>
          {snap.warp.installed ? (
            <>
              <button className="btn sm ghost" onClick={checkWarp} disabled={probing}>
                {probing ? <Spinner /> : <Refresh />} Проверить
              </button>
              <Switch on={snap.warp.connected} onChange={onWarp} disabled={busy} />
            </>
          ) : (
            <button className="btn sm" onClick={onInstallWarp}>
              <Globe /> Скачать
            </button>
          )}
        </div>
      </motion.div>

      <motion.div
        className="card"
        initial={{ opacity: 0, y: 14 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.05, duration: 0.35 }}
        style={{ marginTop: 14 }}
      >
        <div className="row" style={{ marginBottom: 12 }}>
          <h2 style={{ flex: 1 }}>Быстрые настройки</h2>
          <span className="sub" style={{ fontSize: 12 }}>
            применяются сразу, с перезапуском обхода
          </span>
        </div>

        {core ? (
          <>
            <div className="setting" style={{ paddingTop: 0 }}>
              <div className="txt">
                <div className="t">Заворачивать трафик автоматически</div>
                <div className="d">
                  Пропишу {info.label} системным прокси Windows — тогда через него пойдут браузеры и
                  Discord. Выключи, если хочешь указать 127.0.0.1:{core.port} вручную
                </div>
              </div>
              <Switch on={core.systemProxy} onChange={onSystemProxy} disabled={busy} />
            </div>
            <div className="setting">
              <div className="txt">
                <div className="t">Свой сервер</div>
                <div className="d">
                  {core.serverError
                    ? `Ссылка сохранена, но не разбирается: ${core.serverError}`
                    : core.server
                      ? `${core.server} — пресеты «через свой сервер» готовы к работе`
                      : "Не задан. Пресеты с фрагментацией работают и без него; ссылка vless:// нужна только для туннеля — вставь её в настройках"}
                </div>
              </div>
            </div>
            <div className="setting">
              <div className="txt">
                <div className="t">Чего ядро не закрывает</div>
                <div className="d">
                  Через прокси идёт только TCP тех приложений, что читают системные настройки.
                  Голос Discord (UDP) и игры так не обойти — для них остаётся zapret
                </div>
              </div>
            </div>
          </>
        ) : goodbye ? (
          <div className="setting" style={{ paddingTop: 0 }}>
            <div className="txt">
              <div className="t">Настройка — в пресетах</div>
              <div className="d">
                У GoodbyeDPI нет отдельных переключателей: режим обхода, списки доменов и подмена
                DNS задаются ключами прямо в пресете. Списки обновляются на вкладке «Пресеты»
              </div>
            </div>
          </div>
        ) : byedpi ? (
          <>
            <div className="setting" style={{ paddingTop: 0 }}>
              <div className="txt">
                <div className="t">Заворачивать трафик автоматически</div>
                <div className="d">
                  Пропишу ByeDPI системным прокси Windows — тогда через него пойдут браузеры и
                  Discord. Выключи, если хочешь указать SOCKS5 127.0.0.1:{bye.port} вручную
                </div>
              </div>
              <Switch on={bye.systemProxy} onChange={onSystemProxy} disabled={busy} />
            </div>
            <div className="setting">
              <div className="txt">
                <div className="t">Чего ByeDPI не закрывает</div>
                <div className="d">
                  Через прокси идёт только TCP тех приложений, что читают системные настройки.
                  Голос Discord (UDP) и игры так не обойти — для них остаётся zapret
                </div>
              </div>
            </div>
          </>
        ) : (
        <>
        <div className="setting" style={{ paddingTop: 0 }}>
          <div className="txt">
            <div className="t">Игровой фильтр</div>
            <div className="d">Обход на портах 1024–65535: помогает играм, нагружает сеть сильнее</div>
          </div>
          <Segmented
            id="game"
            value={snap.config.gameFilter}
            onChange={onGameFilter}
            disabled={busy}
            options={[
              { value: "off", label: "Выкл" },
              { value: "all", label: "TCP+UDP" },
              { value: "tcp", label: "TCP" },
              { value: "udp", label: "UDP" },
            ]}
          />
        </div>

        <div className="setting">
          <div className="txt">
            <div className="t">Список адресов</div>
            <div className="d">
              {snap.ipsetMode === "loaded"
                ? "Правила применяются только к адресам из списка"
                : snap.ipsetMode === "none"
                  ? "Правила по списку отключены"
                  : snap.ipsetMode === "any"
                    ? "Правила применяются к любым адресам — шире, но грубее"
                    : "Файл списка не найден"}
            </div>
          </div>
          <Segmented
            id="ipset"
            value={snap.ipsetMode === "missing" ? "none" : snap.ipsetMode}
            onChange={onIpsetMode}
            disabled={busy || snap.ipsetMode === "missing"}
            options={[
              { value: "loaded" as IpsetMode, label: "Список" },
              { value: "none" as IpsetMode, label: "Выкл" },
              { value: "any" as IpsetMode, label: "Любые" },
            ]}
          />
        </div>
        </>
        )}
      </motion.div>

      <motion.div
        className="card"
        initial={{ opacity: 0, y: 14 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1, duration: 0.35 }}
        style={{ marginTop: 14, padding: 15 }}
      >
        {best ? (
          <div className="row">
            <span className="rank top" style={{ flex: "none" }}>
              <Crown />
            </span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontWeight: 550 }}>Лучшая по последней проверке — «{bestTitle}»</div>
              <div className="sub" style={{ fontSize: 12 }}>
                {best.ok} из {best.total} проверок пройдено
                {best.avgMs != null ? ` · отклик ${best.avgMs} мс` : ""}
              </div>
            </div>
            <div className="groups">
              {best.groups.map((g) => (
                <span
                  key={g.group}
                  className={`gbadge ${g.ok === g.total ? "full" : g.ok === 0 ? "none" : "part"}`}
                >
                  {g.group} {g.ok}/{g.total}
                </span>
              ))}
            </div>
          </div>
        ) : (
          <div className="row">
            <span className="rank" style={{ flex: "none" }}>
              <Gauge />
            </span>
            <div style={{ flex: 1 }}>
              <div style={{ fontWeight: 550 }}>
                {view ? "Пресеты ещё не проверялись" : "Стратегии ещё не проверялись"}
              </div>
              <div className="sub" style={{ fontSize: 12 }}>
                Прогоню все по очереди и покажу, что реально пробивает Discord и YouTube
              </div>
            </div>
            <button className="btn sm" onClick={onGoTests}>
              Проверить
            </button>
          </div>
        )}
      </motion.div>

      <div className="grid-2" style={{ marginTop: 14 }}>
        {stats.map((s, i) => (
          <motion.div
            key={s.k}
            className="card stat"
            initial={{ opacity: 0, y: 14 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.12 + i * 0.05, duration: 0.35 }}
            style={{ padding: 15 }}
          >
            <span className="k">{s.k}</span>
            <span className="v" title={s.v}>
              {s.v}
            </span>
          </motion.div>
        ))}
      </div>
    </div>
  );
}
