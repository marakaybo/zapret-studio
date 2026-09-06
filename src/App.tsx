import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { AnimatePresence, motion } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, errText } from "./api";
import { Spinner, ToastHost, useToast } from "./components/ui";
import { Bolt, Bulb, Cog, Cross, Gauge, Home as HomeIcon, Layers, Minus, Square, Terminal } from "./icons";
import { engineInfo, engineVersion, isCore } from "./engines";
import { isTauri, mockResults } from "./mock";
import HomeScreen from "./screens/Home";
import Logs from "./screens/Logs";
import Onboarding from "./screens/Onboarding";
import Presets from "./screens/Presets";
import Settings from "./screens/Settings";
import Strategies from "./screens/Strategies";
import Tests from "./screens/Tests";
import Tips from "./screens/Tips";
import type {
  ActionResult,
  Core,
  Engine,
  Health,
  LogLine,
  Progress,
  Snapshot,
  StrategyResult,
  TestStage,
  UpdateCheck,
} from "./types";

type Tab = "home" | "strategies" | "tests" | "tips" | "logs" | "settings";

const TABS: { id: Tab; label: string; icon: React.ReactNode }[] = [
  { id: "home", label: "Главная", icon: <HomeIcon /> },
  // У ByeDPI это не стратегии из батников, а пресеты — подпись меняется по движку
  { id: "strategies", label: "Стратегии", icon: <Layers /> },
  { id: "tests", label: "Проверка", icon: <Gauge /> },
  { id: "tips", label: "Советы", icon: <Bulb /> },
  { id: "logs", label: "Журнал", icon: <Terminal /> },
  { id: "settings", label: "Настройки", icon: <Cog /> },
];

const CORES: Core[] = ["xray", "singbox"];

function TitleBar() {
  const win = isTauri
    ? getCurrentWindow()
    : ({ minimize: () => {}, toggleMaximize: () => {}, close: () => {} } as unknown as ReturnType<typeof getCurrentWindow>);
  return (
    <div className="titlebar" data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region>
        <span className="brand-mark">
          <Bolt />
        </span>
        <span data-tauri-drag-region>Zapret Studio</span>
      </div>
      <div className="win-controls">
        <button onClick={() => win.minimize()} title="Свернуть">
          <Minus />
        </button>
        <button onClick={() => win.toggleMaximize()} title="Развернуть">
          <Square />
        </button>
        <button className="close" onClick={() => win.close()} title="Свернуть в трей">
          <Cross />
        </button>
      </div>
    </div>
  );
}

function InstallOverlay({
  progress,
  cancelling,
  onCancel,
}: {
  progress: Progress;
  cancelling: boolean;
  onCancel: () => void;
}) {
  const phase =
    progress.phase === "download"
      ? "Скачиваю сборку"
      : progress.phase === "extract"
        ? "Распаковываю"
        : progress.phase === "apply"
          ? "Применяю"
          : "Готово";
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(6, 7, 11, 0.82)",
        backdropFilter: "blur(14px)",
        display: "grid",
        placeItems: "center",
        zIndex: 80,
      }}
    >
      <motion.div
        className="card"
        initial={{ scale: 0.94, y: 16 }}
        animate={{ scale: 1, y: 0 }}
        transition={{ type: "spring", stiffness: 260, damping: 24 }}
        style={{ width: 400, textAlign: "center", padding: 26 }}
      >
        <motion.div
          className="hero-mark"
          style={{ width: 52, height: 52, borderRadius: 17, marginBottom: 16 }}
          animate={{ scale: [1, 1.08, 1] }}
          transition={{ duration: 1.6, repeat: Infinity, ease: "easeInOut" }}
        >
          <Bolt />
        </motion.div>
        <div style={{ fontWeight: 600, marginBottom: 4 }}>{phase}</div>
        <div className="sub" style={{ fontSize: 12, marginBottom: 16 }}>
          {progress.detail}
        </div>
        <div className="progress">
          <motion.div
            className="fill"
            animate={{ width: `${Math.max(3, progress.percent)}%` }}
            transition={{ duration: 0.35 }}
          />
        </div>
        {progress.phase !== "done" && (
          <button
            className="btn sm ghost"
            onClick={onCancel}
            disabled={cancelling}
            style={{ marginTop: 16 }}
          >
            {cancelling ? <Spinner /> : <Cross />} {cancelling ? "Отменяю…" : "Отмена"}
          </button>
        )}
      </motion.div>
    </motion.div>
  );
}

function Shell() {
  const toast = useToast();
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [tab, setTab] = useState<Tab>(() => {
    const fromHash = window.location.hash.slice(1) as Tab;
    return TABS.some((t) => t.id === fromHash) ? fromHash : "home";
  });
  const [busy, setBusy] = useState(false);
  const [checking, setChecking] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [update, setUpdate] = useState<UpdateCheck | null>(null);
  const [byeUpdate, setByeUpdate] = useState<UpdateCheck | null>(null);
  const [byeChecking, setByeChecking] = useState(false);
  const [gdpiUpdate, setGdpiUpdate] = useState<UpdateCheck | null>(null);
  const [gdpiChecking, setGdpiChecking] = useState(false);
  const [coreUpdate, setCoreUpdate] = useState<Record<Core, UpdateCheck | null>>({
    xray: null,
    singbox: null,
  });
  const [coreChecking, setCoreChecking] = useState<Record<Core, boolean>>({
    xray: false,
    singbox: false,
  });
  const [appUpdate, setAppUpdate] = useState<UpdateCheck | null>(null);
  const [appChecking, setAppChecking] = useState(false);
  /// Последняя тревога сторожа. Живёт до тех пор, пока обход не починится
  const [alarm, setAlarm] = useState<Health | null>(null);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [results, setResults] = useState<StrategyResult[]>([]);
  const [stage, setStage] = useState<TestStage | null>(null);
  const [testing, setTesting] = useState(false);
  const started = useRef(false);
  const cancelledRef = useRef(false);

  const refresh = useCallback(async () => {
    try {
      setSnap(await api.snapshot());
    } catch (e) {
      toast("err", errText(e));
    }
  }, [toast]);

  useEffect(() => {
    if (started.current) return;
    started.current = true;

    (async () => {
      setSnap(await api.snapshot());
      setLogs(await api.logs());
      // Про обновления не спрашиваем: этим занят фоновый сторож в бэкенде.
      // Он ходит на GitHub не чаще, чем сказано в настройках, и присылает
      // результат событиями ниже. Раньше окно спрашивало про все шесть
      // программ на каждый запуск — и упиралось в лимит GitHub
    })();

    if (!isTauri) {
      setResults(mockResults);
      return;
    }

    const un = [
      listen<LogLine>("log", (e) => setLogs((prev) => [...prev.slice(-599), e.payload])),
      listen<Progress>("update-progress", (e) => {
        setProgress(e.payload);
        if (e.payload.phase === "done") setTimeout(() => setProgress(null), 1100);
      }),
      listen<TestStage>("test-stage", (e) => setStage(e.payload)),
      listen<StrategyResult>("test-result", (e) =>
        setResults((prev) => [...prev.filter((r) => r.strategy !== e.payload.strategy), e.payload])
      ),
      listen("test-done", () => {
        setStage(null);
        setTesting(false);
        refresh();
      }),
      listen("refresh", () => refresh()),
      // Фоновая проверка обновлений присылает результат сама
      listen<UpdateCheck>("update-check", (e) => {
        setUpdate(e.payload);
        if (e.payload.hasUpdate) toast("info", `Вышла новая версия zapret ${e.payload.latest}`);
      }),
      listen<UpdateCheck>("byedpi-update-check", (e) => {
        setByeUpdate(e.payload);
        if (e.payload.hasUpdate) toast("info", `Вышла новая версия ByeDPI ${e.payload.latest}`);
      }),
      listen<UpdateCheck>("goodbye-update-check", (e) => {
        setGdpiUpdate(e.payload);
        if (e.payload.hasUpdate) toast("info", `Вышла новая версия GoodbyeDPI ${e.payload.latest}`);
      }),
      listen<UpdateCheck>("app-update-check", (e) => {
        setAppUpdate(e.payload);
        if (e.payload.hasUpdate)
          toast("info", `Вышло обновление Zapret Studio ${e.payload.latest}`);
      }),
      // Сторож присылает осмотр сам: тревога держится до починки
      listen<Health>("watchdog", (e) => {
        const bad = e.payload.controlOk && e.payload.ok === 0;
        setAlarm(bad ? e.payload : null);
        if (bad) toast("err", `Обход не пробивает: ${e.payload.failed.join(", ")}`);
      }),
      ...CORES.map((id) =>
        listen<UpdateCheck>(`${id}-update-check`, (e) => {
          setCoreUpdate((prev) => ({ ...prev, [id]: e.payload }));
          if (e.payload.hasUpdate)
            toast("info", `Вышла новая версия ${engineInfo(id).label} ${e.payload.latest}`);
        })
      ),
    ];
    return () => {
      un.forEach((p) => p.then((f) => f()));
    };
  }, [refresh]);

  const guard = async (fn: () => Promise<Snapshot | void>, okMsg?: string) => {
    setBusy(true);
    try {
      const s = await fn();
      if (s) setSnap(s as Snapshot);
      else await refresh();
      if (okMsg) toast("ok", okMsg);
    } catch (e) {
      toast("err", errText(e));
    } finally {
      setBusy(false);
    }
  };

  /** Действия, которые возвращают ещё и список того, что сделали. */
  const act = async (fn: () => Promise<ActionResult>) => {
    setBusy(true);
    try {
      const r = await fn();
      setSnap(r.snapshot);
      r.messages.forEach((m) => toast("ok", m));
    } catch (e) {
      toast("err", errText(e));
    } finally {
      setBusy(false);
    }
  };

  const pickFolder = async () => {
    const picked = await open({ directory: true, title: "Папка с распакованным zapret" });
    if (typeof picked !== "string") return;
    await guard(() => api.setZapretDir(picked), "Папка подключена");
  };

  /** Установка рисует оверлей поверх всего окна, поэтому его надо снять в
   *  любом исходе: иначе после ошибки приложением нельзя пользоваться. */
  const runInstall = async (fn: () => Promise<Snapshot>, okMsg: string) => {
    setBusy(true);
    const wasCancelled = () => cancelledRef.current;
    cancelledRef.current = false;
    setCancelling(false);
    try {
      setSnap(await fn());
      toast("ok", okMsg);
    } catch (e) {
      // Отмена — это не сбой, о ней и говорим спокойнее
      toast(wasCancelled() ? "info" : "err", errText(e));
    } finally {
      setBusy(false);
      setCancelling(false);
      setProgress(null);
      cancelledRef.current = false;
    }
  };

  const cancelInstall = async () => {
    cancelledRef.current = true;
    setCancelling(true);
    try {
      await api.cancelInstall();
    } catch (e) {
      toast("err", errText(e));
    }
  };

  const installFresh = () => runInstall(() => api.installZapret(true), "Свежая сборка установлена");
  const applyUpdate = () => runInstall(() => api.installZapret(false), "Обновление установлено");

  /** Результаты прошлой проверки относятся к другому движку — рядом с новым
   *  списком они бы просто врали, поэтому чистим их вместе с переключением. */
  const switchEngine = async (engine: Engine) => {
    setResults([]);
    setStage(null);
    await act(() => api.setEngine(engine));
  };

  const installCore = (engine: Core, fresh: boolean) =>
    runInstall(
      () => api.installCore(engine, fresh),
      `${engineInfo(engine).label} ${fresh ? "установлен" : "обновлён"}`
    );

  const pickCoreFolder = async (engine: Core) => {
    const title = `Папка с ${engineInfo(engine).label}`;
    const picked = await open({ directory: true, title });
    if (typeof picked !== "string") return;
    await guard(() => api.setCoreDir(engine, picked), "Папка подключена");
  };

  const checkCoreUpdate = async (engine: Core) => {
    setCoreChecking((prev) => ({ ...prev, [engine]: true }));
    try {
      const u = await api.checkCoreUpdate(engine);
      const name = engineInfo(engine).label;
      setCoreUpdate((prev) => ({ ...prev, [engine]: u }));
      if (u.error) toast("err", u.error);
      else if (u.hasUpdate) toast("info", `Доступна версия ${name} ${u.latest}`);
      else toast("ok", `${name} последней версии`);
    } catch (e) {
      toast("err", errText(e));
    } finally {
      setCoreChecking((prev) => ({ ...prev, [engine]: false }));
    }
  };

  const applyProfile = (text: string) =>
    act(() => api.importProfile(text));

  const openUrl = (url: string) =>
    api.openUrl(url).catch((e) => toast("err", errText(e)));

  const checkAppUpdate = async () => {
    setAppChecking(true);
    try {
      const u = await api.checkAppUpdate();
      setAppUpdate(u);
      if (u.error) toast("err", u.error);
      else if (u.hasUpdate) toast("info", `Доступна версия ${u.latest}`);
      else toast("ok", "У тебя последняя версия приложения");
    } catch (e) {
      toast("err", errText(e));
    } finally {
      setAppChecking(false);
    }
  };

  /// Установщик заменит файлы приложения, поэтому оно закроется само —
  /// оверлей загрузки в этот раз снимать не нужно
  const installAppUpdate = async () => {
    setBusy(true);
    try {
      toast("info", await api.installAppUpdate());
    } catch (e) {
      toast("err", errText(e));
      setBusy(false);
      setProgress(null);
    }
  };

  /// Осмотр по нажатию — тот же, что делает сторож
  const runHealthCheck = async () => {
    try {
      const h = await api.healthCheck();
      setAlarm(h.controlOk && h.ok === 0 ? h : null);
      if (!h.controlOk) toast("err", "Интернет не отвечает — обход тут ни при чём");
      else if (h.ok === 0) toast("err", `Обход не пробивает: ${h.failed.join(", ")}`);
      else if (h.ok < h.total) toast("info", `Пробивает частично: не отвечает ${h.failed.join(", ")}`);
      else toast("ok", "Обход работает");
    } catch (e) {
      toast("err", errText(e));
    }
  };

  const installByedpi = () => runInstall(() => api.installByedpi(true), "ByeDPI установлен");
  const updateByedpi = () => runInstall(() => api.installByedpi(false), "ByeDPI обновлён");

  const installGoodbye = () => runInstall(() => api.installGoodbye(true), "GoodbyeDPI установлен");
  const updateGoodbye = () => runInstall(() => api.installGoodbye(false), "GoodbyeDPI обновлён");

  const pickGoodbyeFolder = async () => {
    const picked = await open({ directory: true, title: "Папка с распакованным GoodbyeDPI" });
    if (typeof picked !== "string") return;
    await guard(() => api.setGoodbyeDir(picked), "Папка GoodbyeDPI подключена");
  };

  const checkGoodbyeUpdate = async () => {
    setGdpiChecking(true);
    try {
      const u = await api.checkGoodbyeUpdate();
      setGdpiUpdate(u);
      if (u.error) toast("err", u.error);
      else if (u.hasUpdate) toast("info", `Доступна версия GoodbyeDPI ${u.latest}`);
      else toast("ok", "GoodbyeDPI последней версии");
    } catch (e) {
      toast("err", errText(e));
    } finally {
      setGdpiChecking(false);
    }
  };

  const pickByedpiFolder = async () => {
    const picked = await open({ directory: true, title: "Папка с ciadpi.exe" });
    if (typeof picked !== "string") return;
    await guard(() => api.setByedpiDir(picked), "Папка ByeDPI подключена");
  };

  const checkByedpiUpdate = async () => {
    setByeChecking(true);
    try {
      const u = await api.checkByedpiUpdate();
      setByeUpdate(u);
      if (u.error) toast("err", u.error);
      else if (u.hasUpdate) toast("info", `Доступна версия ByeDPI ${u.latest}`);
      else toast("ok", "ByeDPI последней версии");
    } catch (e) {
      toast("err", errText(e));
    } finally {
      setByeChecking(false);
    }
  };

  /// Онбординг для тех, кому zapret не нужен вовсе: ставим ByeDPI и сразу
  /// переключаем движок, иначе приложение снова попросит папку zapret.
  const startWithByedpi = () =>
    runInstall(async () => {
      await api.installByedpi(true);
      await api.setEngine("byedpi");
      return api.setOption("onboarded", true);
    }, "ByeDPI установлен и включён как движок обхода");

  const checkUpdate = async () => {
    setChecking(true);
    try {
      const u = await api.checkUpdate();
      setUpdate(u);
      if (u.error) toast("err", u.error);
      else if (u.hasUpdate) toast("info", `Доступна версия ${u.latest}`);
      else toast("ok", "У тебя последняя версия");
    } catch (e) {
      toast("err", errText(e));
    } finally {
      setChecking(false);
    }
  };

  const toggle = () =>
    guard(
      () => (snap?.running ? api.stop() : api.start()),
      snap?.running ? "Обход выключен" : "Обход включён"
    );

  const runTests = async (names: string[], baseline: boolean) => {
    setResults([]);
    setTesting(true);
    try {
      if (snap?.engine === "zapret") await api.runTests(names, baseline);
      else await api.runPresetTests(names, baseline);
      toast("ok", "Проверка завершена");
    } catch (e) {
      toast("err", errText(e));
      setTesting(false);
      setStage(null);
    }
  };

  if (!snap) {
    return (
      <div className="app">
        <TitleBar />
        <div style={{ flex: 1, display: "grid", placeItems: "center" }}>
          <Spinner />
        </div>
      </div>
    );
  }

  // Знакомство показываем только по-настоящему в первый раз. Пропавшая папка
  // zapret — повод для плашки на главной, а не для того, чтобы отобрать у
  // человека всё окно: во время установки она пропадает на несколько секунд
  // сама по себе, и раньше поверх обновления вылезал экран первого запуска.
  const onboarding = !snap.config.onboarded && !progress;
  /// Движок выбран zapret, а папки нет — про это надо сказать, но мягко
  const zapretMissing = snap.config.onboarded && snap.engine === "zapret" && !snap.dirOk;
  const presetEngine = snap.engine !== "zapret";
  const core = isCore(snap.engine) ? snap.engine : null;

  return (
    <div className="app">
      <TitleBar />

      <AnimatePresence>
        {progress && (
          <InstallOverlay progress={progress} cancelling={cancelling} onCancel={cancelInstall} />
        )}
      </AnimatePresence>

      {onboarding ? (
        <Onboarding onInstall={installFresh} onPick={pickFolder} onByedpi={startWithByedpi} busy={busy} />
      ) : (
        <div className="body">
          <nav className="sidebar">
            {TABS.map((t) => (
              <button key={t.id} className={`nav-item ${tab === t.id ? "active" : ""}`} onClick={() => setTab(t.id)}>
                {tab === t.id && (
                  <motion.span
                    layoutId="nav-glow"
                    className="glow"
                    transition={{ type: "spring", stiffness: 420, damping: 36 }}
                  />
                )}
                {t.icon}
                <span>{t.id === "strategies" && presetEngine ? "Пресеты" : t.label}</span>
                {t.id === "settings" &&
                  (update?.hasUpdate ||
                    byeUpdate?.hasUpdate ||
                    gdpiUpdate?.hasUpdate ||
                    coreUpdate.xray?.hasUpdate ||
                    coreUpdate.singbox?.hasUpdate ||
                    appUpdate?.hasUpdate) && <span className="nav-badge">1</span>}
              </button>
            ))}
            <div className="sidebar-foot">
              <span>{engineVersion(snap)}</span>
              <span>{snap.running ? "обход работает" : "обход выключен"}</span>
            </div>
          </nav>

          <main className="content">
            <AnimatePresence mode="wait">
              <motion.div
                key={tab}
                initial={{ opacity: 0, y: 12 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -8 }}
                transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
              >
                {tab === "home" && (
                  <HomeScreen
                    snap={snap}
                    update={update}
                    busy={busy}
                    results={results}
                    onToggle={toggle}
                    onUpdate={applyUpdate}
                    onOpenFolder={() => api.openFolder().catch((e) => toast("err", errText(e)))}
                    onGoTests={() => setTab("tests")}
                    onStopConflicts={() => act(() => api.stopConflicts())}
                    onStopStray={() => act(() => api.stopStray())}
                    onWarp={(on) => act(() => (on ? api.warpConnect() : api.warpDisconnect()))}
                    onWarpCheck={() => api.warpCheck()}
                    onInstallWarp={() => openUrl(snap.warp.installUrl)}
                    alarm={alarm}
                    onHealthCheck={runHealthCheck}
                    zapretMissing={zapretMissing}
                    onPickFolder={pickFolder}
                    onInstallFresh={installFresh}
                    onGameFilter={(mode) => act(() => api.setGameFilter(mode))}
                    onIpsetMode={(mode) => act(() => api.setIpsetMode(mode))}
                    onEngine={switchEngine}
                    onSystemProxy={(v) => act(() => api.setSystemProxy(snap.engine, v))}
                  />
                )}
                {tab === "strategies" &&
                  (presetEngine ? (
                    <Presets
                      snap={snap}
                      busy={busy}
                      onSelect={(id) => guard(() => api.selectPreset(id))}
                      onStart={(id) => guard(() => api.start(id), "Обход перезапущен")}
                      onSave={(id, name, args) => guard(() => api.savePreset(id, name, args), "Пресет сохранён")}
                      onDelete={(id) => guard(() => api.deletePreset(id), "Пресет удалён")}
                      onInstall={
                        core
                          ? () => installCore(core, true)
                          : snap.engine === "byedpi"
                            ? installByedpi
                            : installGoodbye
                      }
                      onPickFolder={
                        core
                          ? () => pickCoreFolder(core)
                          : snap.engine === "byedpi"
                            ? pickByedpiFolder
                            : pickGoodbyeFolder
                      }
                      onUpdateBlacklist={() => act(() => api.updateGoodbyeBlacklist())}
                      onServer={(url) => core && act(() => api.setCoreServer(core, url))}
                      onSubscription={(url) => core && act(() => api.loadSubscription(core, url))}
                      onSelectServer={(i) => core && act(() => api.selectServer(core, i))}
                      onPingServers={() => (core ? api.pingServers(core) : Promise.resolve([]))}
                    />
                  ) : (
                    <Strategies
                      snap={snap}
                      busy={busy}
                      onSelect={(name) => guard(() => api.selectStrategy(name))}
                      onStart={(name) => guard(() => api.start(name), `Запущена «${name}»`)}
                    />
                  ))}
                {tab === "tests" && (
                  <Tests
                    snap={snap}
                    results={results}
                    stage={stage}
                    running={testing}
                    busy={busy}
                    onRun={runTests}
                    onCancel={() => api.cancelTests()}
                    onApply={(name) => guard(() => api.start(name), "Включено")}
                    onSaveTargets={(items) =>
                      guard(() => api.setCustomTargets(items), "Список сайтов сохранён")
                    }
                  />
                )}
                {tab === "tips" && <Tips snap={snap} onEngine={switchEngine} />}
                {tab === "logs" && (
                  <Logs
                    logs={logs}
                    onClear={() => {
                      api.clearLogs();
                      setLogs([]);
                    }}
                  />
                )}
                {tab === "settings" && (
                  <Settings
                    snap={snap}
                    update={update}
                    busy={busy}
                    checking={checking}
                    onPickFolder={pickFolder}
                    onInstallFresh={installFresh}
                    onCheckUpdate={checkUpdate}
                    onUpdate={applyUpdate}
                    onOpenFolder={() => api.openFolder().catch((e) => toast("err", errText(e)))}
                    onService={(enable) =>
                      guard(
                        () => (enable ? api.installService() : api.removeService()),
                        enable ? "Служба установлена — обход будет включаться сам" : "Служба удалена"
                      )
                    }
                    onAppAutostart={(enable) => guard(() => api.setAppAutostart(enable))}
                    onOption={(key, value) => guard(() => api.setOption(key, value))}
                    onDiagnostics={() => api.diagnostics()}
                    onStopConflicts={() => act(() => api.stopConflicts())}
                    onWarp={(on) => act(() => (on ? api.warpConnect() : api.warpDisconnect()))}
                    onWarpCheck={() => api.warpCheck()}
                    onInstallWarp={() => openUrl(snap.warp.installUrl)}
                    coreUpdate={coreUpdate}
                    coreChecking={coreChecking}
                    onCoreInstall={(e) => installCore(e, true)}
                    onCoreUpdate={(e) => installCore(e, false)}
                    onCorePick={pickCoreFolder}
                    onCoreCheck={checkCoreUpdate}
                    onCorePort={(e, port) => act(() => api.setCorePort(e, port))}
                    onCoreServer={(e, url) => act(() => api.setCoreServer(e, url))}
                    onUpdateIpset={() => act(() => api.updateIpset())}
                    onSetFake={(kind, source) => act(() => api.setActiveFake(kind, source))}
                    onClearDiscord={() => act(() => api.clearDiscordCache())}
                    byeUpdate={byeUpdate}
                    byeChecking={byeChecking}
                    gdpiUpdate={gdpiUpdate}
                    gdpiChecking={gdpiChecking}
                    onInstallGoodbye={installGoodbye}
                    onUpdateGoodbye={updateGoodbye}
                    onPickGoodbyeFolder={pickGoodbyeFolder}
                    onCheckGoodbyeUpdate={checkGoodbyeUpdate}
                    onUpdateBlacklist={() => act(() => api.updateGoodbyeBlacklist())}
                    onInstallByedpi={installByedpi}
                    onUpdateByedpi={updateByedpi}
                    onPickByedpiFolder={pickByedpiFolder}
                    onCheckByedpiUpdate={checkByedpiUpdate}
                    onEngine={switchEngine}
                    onByedpiPort={(port) => act(() => api.setByedpiPort(port))}
                    onSystemProxy={(v) => act(() => api.setSystemProxy("byedpi", v))}
                    onCoreSystemProxy={(e, v) => act(() => api.setSystemProxy(e, v))}
                    appUpdate={appUpdate}
                    appChecking={appChecking}
                    onCheckAppUpdate={checkAppUpdate}
                    onInstallAppUpdate={installAppUpdate}
                    onExportProfile={() => api.exportProfile()}
                    onImportProfile={applyProfile}
                    onHostsStatus={() => api.hostsStatus()}
                    onApplyHosts={async () => {
                      setBusy(true);
                      try {
                        toast("ok", await api.applyHosts());
                      } catch (e) {
                        toast("err", errText(e));
                      } finally {
                        setBusy(false);
                      }
                    }}
                  />
                )}
              </motion.div>
            </AnimatePresence>
          </main>
        </div>
      )}
    </div>
  );
}

export default function App() {
  return (
    <ToastHost>
      <Shell />
    </ToastHost>
  );
}
