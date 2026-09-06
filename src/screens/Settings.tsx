import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { Bolt, Download, Folder, Globe, Link, Refresh, Shield } from "../icons";
import { EnginePicker, Segmented, Spinner, Switch } from "../components/ui";
import { ENGINES, engineInfo } from "../engines";
import type {
  Check as DiagCheck,
  Core,
  CoreState,
  Engine,
  HostsStatus,
  Snapshot,
  UpdateCheck,
  WarpProbe,
} from "../types";

function Row({ title, desc, children }: { title: string; desc: string; children: React.ReactNode }) {
  return (
    <div className="setting">
      <div className="txt">
        <div className="t">{title}</div>
        <div className="d">{desc}</div>
      </div>
      {children}
    </div>
  );
}

/** Xray и sing-box настраиваются одинаково, поэтому карточка у них общая. */
function CoreCard({
  engine,
  state,
  update,
  checking,
  busy,
  active,
  onInstall,
  onUpdate,
  onPick,
  onCheck,
  onPort,
  onServer,
  onSystemProxy,
  onAutostart,
}: {
  engine: Core;
  state: CoreState;
  update: UpdateCheck | null;
  checking: boolean;
  busy: boolean;
  active: boolean;
  onInstall: () => void;
  onUpdate: () => void;
  onPick: () => void;
  onCheck: () => void;
  onPort: (port: number) => void;
  onServer: (url: string) => void;
  onSystemProxy: (on: boolean) => void;
  onAutostart: (on: boolean) => void;
}) {
  const info = engineInfo(engine);
  const [port, setPort] = useState(String(state.port));
  const [server, setServer] = useState("");

  return (
    <div className="card">
      <div className="row" style={{ marginBottom: 4 }}>
        <h2 style={{ flex: 1 }}>{info.label}</h2>
        <span className="sub" style={{ fontSize: 12 }}>
          {active ? "сейчас работает этот движок" : "запасной движок обхода"}
        </span>
      </div>
      <p className="sub" style={{ fontSize: 12, marginBottom: 4 }}>
        {info.short}. Пресеты с фрагментацией работают сами по себе, сервер им не нужен. Ссылка на
        свой сервер нужна только пресетам «через свой сервер» — с ней это уже туннель, а не обход.
      </p>

      {state.installed ? (
        <>
          <Row
            title={state.managed ? "Папкой управляет приложение" : "Подключена твоя папка"}
            desc={state.dir ?? "не выбрана"}
          >
            <button className="btn sm ghost" onClick={onPick} disabled={busy}>
              <Folder /> Выбрать другую
            </button>
          </Row>
          <Row
            title={`Установлена версия ${state.version ?? "неизвестна"}`}
            desc={
              update?.error
                ? `Не удалось проверить: ${update.error}`
                : update?.latest
                  ? update.hasUpdate
                    ? `На GitHub уже ${update.latest} от ${update.release?.publishedAt ?? ""}`
                    : "Это последняя версия"
                  : `Беру релизы из ${info.repo}`
            }
          >
            <button className="btn sm ghost" onClick={onCheck} disabled={checking}>
              {checking ? <Spinner /> : <Refresh />} Проверить
            </button>
            {update?.hasUpdate && (
              <button className="btn sm primary" onClick={onUpdate} disabled={busy}>
                {busy ? <Spinner /> : <Download />} Обновить
              </button>
            )}
          </Row>
          <div className="setting" style={{ flexDirection: "column", alignItems: "stretch" }}>
            <div className="txt">
              <div className="t">Свой сервер</div>
              <div className="d">
                {state.serverError
                  ? `Ссылка сохранена, но не разбирается: ${state.serverError}`
                  : state.server
                    ? `Сейчас: ${state.server}. Вставь другую ссылку, чтобы заменить, или очисти поле и сохрани, чтобы забыть`
                    : "Вставь ссылку из панели или бота: vless://, vmess://, trojan:// или ss://. Приложение серверов не выдаёт — сервер твой"}
              </div>
            </div>
            <div className="server-row">
              <input
                className="input mono"
                spellCheck={false}
                placeholder={state.server ? "новая ссылка или пусто, чтобы забыть" : "vless://…"}
                value={server}
                onChange={(e) => setServer(e.target.value)}
              />
              <button
                className="btn sm primary"
                disabled={busy || (!server.trim() && !state.server)}
                onClick={() => {
                  onServer(server.trim());
                  setServer("");
                }}
              >
                {busy ? <Spinner /> : <Link />} Сохранить
              </button>
            </div>
          </div>
          <Row
            title="Заворачивать трафик автоматически"
            desc={
              state.systemProxyActive
                ? `Системный прокси Windows сейчас направлен в ${info.label}`
                : `Пропишу ${info.label} в системные настройки прокси при запуске обхода и уберу оттуда при остановке`
            }
          >
            <Switch on={state.systemProxy} onChange={onSystemProxy} disabled={busy} />
          </Row>
          <Row
            title="Порт прокси"
            desc={`${info.label} слушает 127.0.0.1:${state.port}. Меняй, если порт уже кем-то занят`}
          >
            <input
              className="input"
              style={{ width: 92 }}
              inputMode="numeric"
              value={port}
              onChange={(e) => setPort(e.target.value.replace(/\D/g, "").slice(0, 5))}
            />
            <button
              className="btn sm"
              disabled={busy || Number(port) === state.port || !Number(port)}
              onClick={() => onPort(Number(port))}
            >
              Применить
            </button>
          </Row>
          <Row
            title={`Включать ${info.label} при запуске приложения`}
            desc="Своей службы у ядра нет: прокси поднимается вместе с окном. Вместе с автозапуском приложения это даёт обход сразу после входа в Windows"
          >
            <Switch on={state.autostart} onChange={onAutostart} disabled={busy} />
          </Row>
        </>
      ) : (
        <Row
          title={`${info.label} не установлен`}
          desc={`Скачаю последний релиз с GitHub (${info.repo}). Или укажи папку, если он у тебя уже есть`}
        >
          <button className="btn sm ghost" onClick={onPick} disabled={busy}>
            <Folder /> Указать папку
          </button>
          <button className="btn sm primary" onClick={onInstall} disabled={busy}>
            {busy ? <Spinner /> : <Globe />} Скачать
          </button>
        </Row>
      )}
    </div>
  );
}

export default function Settings({
  snap,
  update,
  busy,
  checking,
  onPickFolder,
  onInstallFresh,
  onCheckUpdate,
  onUpdate,
  onOpenFolder,
  onService,
  onAppAutostart,
  onOption,
  onDiagnostics,
  onStopConflicts,
  onWarp,
  onUpdateIpset,
  onSetFake,
  onClearDiscord,
  onHostsStatus,
  onApplyHosts,
  byeUpdate,
  byeChecking,
  gdpiUpdate,
  gdpiChecking,
  onInstallGoodbye,
  onUpdateGoodbye,
  onPickGoodbyeFolder,
  onCheckGoodbyeUpdate,
  onUpdateBlacklist,
  onInstallByedpi,
  onUpdateByedpi,
  onPickByedpiFolder,
  onCheckByedpiUpdate,
  onEngine,
  onByedpiPort,
  onSystemProxy,
  onWarpCheck,
  onInstallWarp,
  coreUpdate,
  coreChecking,
  onCoreInstall,
  onCoreUpdate,
  onCorePick,
  onCoreCheck,
  onCorePort,
  onCoreServer,
  onCoreSystemProxy,
  appUpdate,
  appChecking,
  onCheckAppUpdate,
  onInstallAppUpdate,
  onExportProfile,
  onImportProfile,
}: {
  snap: Snapshot;
  update: UpdateCheck | null;
  busy: boolean;
  checking: boolean;
  onPickFolder: () => void;
  onInstallFresh: () => void;
  onCheckUpdate: () => void;
  onUpdate: () => void;
  onOpenFolder: () => void;
  onService: (enable: boolean) => void;
  onAppAutostart: (enable: boolean) => void;
  onOption: (key: string, value: unknown) => void;
  onDiagnostics: () => Promise<DiagCheck[]>;
  onStopConflicts: () => void;
  onWarp: (connect: boolean) => void;
  onUpdateIpset: () => void;
  onSetFake: (kind: "discord" | "game", source: string) => void;
  onClearDiscord: () => void;
  onHostsStatus: () => Promise<HostsStatus>;
  onApplyHosts: () => void;
  byeUpdate: UpdateCheck | null;
  byeChecking: boolean;
  gdpiUpdate: UpdateCheck | null;
  gdpiChecking: boolean;
  onInstallGoodbye: () => void;
  onUpdateGoodbye: () => void;
  onPickGoodbyeFolder: () => void;
  onCheckGoodbyeUpdate: () => void;
  onUpdateBlacklist: () => void;
  onInstallByedpi: () => void;
  onUpdateByedpi: () => void;
  onPickByedpiFolder: () => void;
  onCheckByedpiUpdate: () => void;
  onEngine: (engine: Engine) => void;
  onByedpiPort: (port: number) => void;
  onSystemProxy: (enable: boolean) => void;
  onWarpCheck: () => Promise<WarpProbe>;
  onInstallWarp: () => void;
  coreUpdate: Record<Core, UpdateCheck | null>;
  coreChecking: Record<Core, boolean>;
  onCoreInstall: (engine: Core) => void;
  onCoreUpdate: (engine: Core) => void;
  onCorePick: (engine: Core) => void;
  onCoreCheck: (engine: Core) => void;
  onCorePort: (engine: Core, port: number) => void;
  onCoreServer: (engine: Core, url: string) => void;
  onCoreSystemProxy: (engine: Core, enable: boolean) => void;
  appUpdate: UpdateCheck | null;
  appChecking: boolean;
  onCheckAppUpdate: () => void;
  onInstallAppUpdate: () => void;
  onExportProfile: () => Promise<string>;
  onImportProfile: (text: string) => void;
}) {
  const cfg = snap.config;
  const bye = snap.byedpi;
  const gdpi = snap.goodbye;
  const [checks, setChecks] = useState<DiagCheck[] | null>(null);
  const [diagBusy, setDiagBusy] = useState(false);
  const [hosts, setHosts] = useState<HostsStatus | null>(null);
  const [hostsBusy, setHostsBusy] = useState(false);
  const [port, setPort] = useState(String(bye.port));
  const [repo, setRepo] = useState(snap.appRepo);
  const [mine, setMine] = useState<string | null>(null);
  const [theirs, setTheirs] = useState("");
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

  const runDiag = async () => {
    setDiagBusy(true);
    try {
      setChecks(await onDiagnostics());
    } finally {
      setDiagBusy(false);
    }
  };

  const checkHosts = async () => {
    setHostsBusy(true);
    try {
      setHosts(await onHostsStatus());
    } catch {
      setHosts(null);
    } finally {
      setHostsBusy(false);
    }
  };

  return (
    <div className="screen">
      <div className="head">
        <h1>Настройки</h1>
        <p className="sub">Источник сборки, обновления, автозапуск, инструменты zapret и диагностика.</p>
      </div>

      <div className="stack">
        <div className="card">
          <div className="row" style={{ marginBottom: 4 }}>
            <span style={{ color: "var(--accent-2)", display: "flex", fontSize: 17 }}>
              <Bolt />
            </span>
            <h2 style={{ flex: 1 }}>Само приложение</h2>
          </div>
          <p className="sub" style={{ fontSize: 12, marginBottom: 4 }}>
            Zapret Studio следит за обновлениями пяти чужих программ — и за своими тоже.
            Установщик приложение скачает и запустит, но только по нажатию: ставить себя само
            по таймеру оно не будет.
          </p>
          <Row
            title={`Установлена версия ${snap.appVersion}`}
            desc={
              appUpdate?.error
                ? `Не удалось проверить: ${appUpdate.error}`
                : appUpdate?.latest
                  ? appUpdate.hasUpdate
                    ? `Вышла ${appUpdate.latest} от ${appUpdate.release?.publishedAt ?? ""}`
                    : "Это последняя версия"
                  : `Беру релизы из ${snap.appRepo}`
            }
          >
            <button className="btn sm ghost" onClick={onCheckAppUpdate} disabled={appChecking}>
              {appChecking ? <Spinner /> : <Refresh />} Проверить
            </button>
            {appUpdate?.hasUpdate && (
              <button className="btn sm primary" onClick={onInstallAppUpdate} disabled={busy}>
                {busy ? <Spinner /> : <Download />} Обновить
              </button>
            )}
          </Row>
          {appUpdate?.hasUpdate && appUpdate.release?.notes && (
            <div
              className="sub"
              style={{ fontSize: 12, whiteSpace: "pre-wrap", maxHeight: 160, overflow: "auto", paddingBottom: 8 }}
            >
              {appUpdate.release.notes.slice(0, 800)}
            </div>
          )}
          <Row
            title="Откуда брать обновления"
            desc="Репозиторий с релизами приложения в виде «владелец/репозиторий». Меняй, если раздаёшь свою сборку из своего репозитория"
          >
            <input
              className="input mono"
              style={{ width: 240 }}
              spellCheck={false}
              value={repo}
              onChange={(e) => setRepo(e.target.value)}
            />
            <button
              className="btn sm"
              disabled={busy || !repo.trim() || repo.trim() === snap.appRepo}
              onClick={() => onOption("appRepo", repo.trim())}
            >
              Применить
            </button>
          </Row>
        </div>

        <div className="card">
          <h2 style={{ marginBottom: 4 }}>Профиль настроек</h2>
          <p className="sub" style={{ fontSize: 12, marginBottom: 4 }}>
            Подобранную стратегию можно передать другому человеку: у одного провайдера обычно
            работает одно и то же, а подбирать заново — полчаса. Внутри только выбор — движок,
            пресеты, порты, свои сайты для проверки. Ссылок на серверы там нет: в них твои uuid
            и пароли, а профиль улетает в переписку навсегда.
          </p>

          <Row
            title="Мой профиль"
            desc={mine ? "Скопируй и отправь — вставляется в поле ниже" : "Соберу текст, который можно переслать"}
          >
            <button
              className="btn sm"
              onClick={async () => setMine(await onExportProfile())}
              disabled={busy}
            >
              <Link /> {mine ? "Обновить" : "Показать"}
            </button>
            {mine && (
              <button className="btn sm ghost" onClick={() => setMine(null)}>
                Скрыть
              </button>
            )}
          </Row>
          {mine && (
            <textarea
              className="input mono"
              readOnly
              rows={7}
              spellCheck={false}
              value={mine}
              onFocus={(e) => e.currentTarget.select()}
              style={{ marginBottom: 10, fontSize: 11, userSelect: "text" }}
            />
          )}

          <div className="setting" style={{ flexDirection: "column", alignItems: "stretch" }}>
            <div className="txt">
              <div className="t">Применить чужой</div>
              <div className="d">
                Вставь профиль, который прислали. Твои папки, версии и свои серверы останутся
                на месте — заменится только выбор
              </div>
            </div>
            <div className="server-row">
              <textarea
                className="input mono"
                rows={3}
                spellCheck={false}
                placeholder='{"version":1,"engine":"zapret", …}'
                value={theirs}
                onChange={(e) => setTheirs(e.target.value)}
                style={{ flex: 1, fontSize: 11 }}
              />
              <button
                className="btn sm primary"
                disabled={busy || !theirs.trim()}
                onClick={() => {
                  onImportProfile(theirs.trim());
                  setTheirs("");
                }}
              >
                {busy ? <Spinner /> : null} Применить
              </button>
            </div>
          </div>
        </div>

        <div className="card">
          <h2 style={{ marginBottom: 4 }}>Сборка zapret</h2>
          <Row
            title={cfg.managed ? "Папкой управляет приложение" : "Подключена твоя папка"}
            desc={cfg.zapretDir ?? "не выбрана"}
          >
            <button className="btn sm ghost" onClick={onOpenFolder}>
              <Folder /> Открыть
            </button>
            <button className="btn sm" onClick={onPickFolder} disabled={busy}>
              Выбрать другую
            </button>
          </Row>
          {!cfg.managed && (
            <Row
              title="Перейти на свою папку приложения"
              desc="Скачаю свежий релиз в служебную папку и буду обновлять его сам"
            >
              <button className="btn sm" onClick={onInstallFresh} disabled={busy}>
                {busy ? <Spinner /> : <Download />} Скачать
              </button>
            </Row>
          )}
        </div>

        <div className="card">
          <h2 style={{ marginBottom: 4 }}>Обновления</h2>
          <Row
            title={`Установлена версия ${cfg.installedVersion ?? "неизвестна"}`}
            desc={
              update?.error
                ? `Не удалось проверить: ${update.error}`
                : update?.latest
                  ? update.hasUpdate
                    ? `На GitHub уже ${update.latest} от ${update.release?.publishedAt ?? ""}`
                    : "Это последняя версия"
                  : cfg.lastUpdateCheck
                    ? `Последняя проверка: ${cfg.lastUpdateCheck}`
                    : "Нажми «Проверить», чтобы спросить GitHub"
            }
          >
            <button className="btn sm ghost" onClick={onCheckUpdate} disabled={checking}>
              {checking ? <Spinner /> : <Refresh />} Проверить
            </button>
            {update?.hasUpdate && (
              <button className="btn sm primary" onClick={onUpdate} disabled={busy}>
                {busy ? <Spinner /> : <Download />} Обновить
              </button>
            )}
          </Row>
          <Row title="Проверять в фоне" desc="Сам хожу на GitHub и присылаю уведомление, когда выходит новая версия">
            <Switch on={cfg.autoCheckUpdates} onChange={(v) => onOption("autoCheckUpdates", v)} />
          </Row>
          {cfg.autoCheckUpdates && (
            <>
              <Row title="Как часто проверять" desc="Реже — меньше запросов к GitHub, позже узнаешь о новой версии">
                <Segmented
                  id="interval"
                  value={String(cfg.updateIntervalHours)}
                  onChange={(v) => onOption("updateIntervalHours", Number(v))}
                  options={[
                    { value: "1", label: "1 ч" },
                    { value: "6", label: "6 ч" },
                    { value: "24", label: "Раз в сутки" },
                  ]}
                />
              </Row>
              <Row
                title="Ставить обновления сразу"
                desc="Без вопросов скачает и применит новую версию, обход перезапустится сам"
              >
                <Switch on={cfg.autoInstallUpdates} onChange={(v) => onOption("autoInstallUpdates", v)} />
              </Row>
            </>
          )}
        </div>

        <div className="card">
          <h2 style={{ marginBottom: 4 }}>Автозапуск</h2>
          <Row
            title="Включать обход вместе с Windows"
            desc={
              snap.service.installed
                ? `Установлена служба со стратегией «${snap.service.strategy ?? "?"}»`
                : "Ставит стратегию системной службой — работает до входа в систему и без запуска приложения"
            }
          >
            <Switch on={snap.service.installed} onChange={onService} disabled={busy || !snap.dirOk} />
          </Row>
          <Row title="Запускать приложение при входе" desc="Стартует свёрнутым в трей, с правами администратора">
            <Switch on={snap.autostart} onChange={onAppAutostart} />
          </Row>
        </div>

        <div className="card">
          <h2 style={{ marginBottom: 4 }}>Сторож</h2>
          <p className="sub" style={{ fontSize: 12, marginBottom: 4 }}>
            Блокировки меняются молча: вчера стратегия работала, сегодня нет, и узнаёшь об этом,
            когда не грузится YouTube. Сторож раз в несколько минут тихо стучится в пару целей и
            говорит, если обход перестал пробивать. Упавшую сеть от упавшего обхода он отличает
            по контрольной точке, так что зря будить не должен.
          </p>
          <Row
            title="Присматривать за обходом"
            desc="Проверяет, только когда обход включён, и молчит во время проверки стратегий"
          >
            <Switch on={cfg.watchdog} onChange={(v) => onOption("watchdog", v)} disabled={busy} />
          </Row>
          {cfg.watchdog && (
            <>
              <Row title="Как часто осматривать" desc="Реже — меньше запросов в сеть, позже узнаешь о поломке">
                <Segmented
                  id="watchdog-interval"
                  value={String(cfg.watchdogIntervalMin)}
                  onChange={(v) => onOption("watchdogIntervalMin", Number(v))}
                  disabled={busy}
                  options={[
                    { value: "5", label: "5 мин" },
                    { value: "15", label: "15 мин" },
                    { value: "60", label: "Раз в час" },
                  ]}
                />
              </Row>
              <Row
                title="Пытаться починить самому"
                desc="Сначала просто перезапустит обход — чаще всего движок просто умер. Если и со второго раза не пробивает, переключится на лучшую по последней проверке"
              >
                <Switch
                  on={cfg.watchdogAutoFix}
                  onChange={(v) => onOption("watchdogAutoFix", v)}
                  disabled={busy}
                />
              </Row>
            </>
          )}
        </div>

        <div className="card">
          <h2 style={{ marginBottom: 4 }}>Движок обхода</h2>
          <Row
            title="Чем обходить блокировки"
            desc="Что включает большая кнопка на главной. Одновременно работает только один — иначе они мешают друг другу, а zapret с GoodbyeDPI ещё и делят драйвер. Что когда выбирать — на вкладке «Советы»"
          >
            <EnginePicker
              id="engine-settings"
              value={snap.engine}
              onChange={onEngine}
              disabled={busy}
              options={ENGINES.map((e) => ({ value: e.id as Engine, label: e.label, title: e.when }))}
            />
          </Row>
        </div>

        {(["xray", "singbox"] as Core[]).map((id) => (
          <CoreCard
            key={id}
            engine={id}
            state={snap[id]}
            update={coreUpdate[id]}
            checking={coreChecking[id]}
            busy={busy}
            active={snap.engine === id}
            onInstall={() => onCoreInstall(id)}
            onUpdate={() => onCoreUpdate(id)}
            onPick={() => onCorePick(id)}
            onCheck={() => onCoreCheck(id)}
            onPort={(p) => onCorePort(id, p)}
            onServer={(url) => onCoreServer(id, url)}
            onSystemProxy={(on) => onCoreSystemProxy(id, on)}
            onAutostart={(on) => onOption(id, { ...cfg[id], autostart: on })}
          />
        ))}

        <div className="card">
          <div className="row" style={{ marginBottom: 4 }}>
            <h2 style={{ flex: 1 }}>GoodbyeDPI</h2>
            <span className="sub" style={{ fontSize: 12 }}>
              {snap.engine === "goodbyedpi" ? "сейчас работает этот движок" : "запасной движок обхода"}
            </span>
          </div>
          <p className="sub" style={{ fontSize: 12, marginBottom: 4 }}>
            Тот же драйвер WinDivert, что у zapret, но настраивается ключами запуска: режимы
            −1…−9 плюс списки заблокированных доменов. Работает от администратора и не может
            быть включён одновременно с zapret.
          </p>

          {gdpi.installed ? (
            <>
              <Row
                title={gdpi.managed ? "Папкой управляет приложение" : "Подключена твоя папка"}
                desc={gdpi.dir ?? "не выбрана"}
              >
                <button className="btn sm ghost" onClick={onPickGoodbyeFolder} disabled={busy}>
                  <Folder /> Выбрать другую
                </button>
              </Row>
              <Row
                title={`Установлена версия ${gdpi.version ?? "неизвестна"}`}
                desc={
                  gdpiUpdate?.error
                    ? `Не удалось проверить: ${gdpiUpdate.error}`
                    : gdpiUpdate?.latest
                      ? gdpiUpdate.hasUpdate
                        ? `На GitHub уже ${gdpiUpdate.latest} от ${gdpiUpdate.release?.publishedAt ?? ""}`
                        : "Это последняя версия"
                      : "Беру самый свежий релиз из ValdikSS/GoodbyeDPI, включая предрелизы: стабильный там не обновлялся с 2022 года"
                }
              >
                <button className="btn sm ghost" onClick={onCheckGoodbyeUpdate} disabled={gdpiChecking}>
                  {gdpiChecking ? <Spinner /> : <Refresh />} Проверить
                </button>
                {gdpiUpdate?.hasUpdate && (
                  <button className="btn sm primary" onClick={onUpdateGoodbye} disabled={busy}>
                    {busy ? <Spinner /> : <Download />} Обновить
                  </button>
                )}
              </Row>
              <Row
                title="Список заблокированного"
                desc={
                  gdpi.blacklist != null
                    ? `В russia-blacklist.txt ${gdpi.blacklist} доменов. Обновляю из того же источника, что и сам GoodbyeDPI`
                    : "Файла russia-blacklist.txt нет — пресеты «Россия» работать не будут"
                }
              >
                <button className="btn sm" onClick={onUpdateBlacklist} disabled={busy}>
                  {busy ? <Spinner /> : <Refresh />} Обновить
                </button>
              </Row>
              <Row
                title="Включать GoodbyeDPI при запуске приложения"
                desc="Своей службы приложение для него не ставит: обход поднимается вместе с окном. Вместе с автозапуском приложения это даёт обход сразу после входа в Windows"
              >
                <Switch
                  on={cfg.goodbyeAutostart}
                  onChange={(v) => onOption("goodbyeAutostart", v)}
                  disabled={busy}
                />
              </Row>
            </>
          ) : (
            <Row
              title="GoodbyeDPI не установлен"
              desc="Скачаю последнюю сборку с GitHub вместе с драйвером и списками доменов. Или укажи папку, если она уже есть"
            >
              <button className="btn sm ghost" onClick={onPickGoodbyeFolder} disabled={busy}>
                <Folder /> Указать папку
              </button>
              <button className="btn sm primary" onClick={onInstallGoodbye} disabled={busy}>
                {busy ? <Spinner /> : <Shield />} Скачать
              </button>
            </Row>
          )}
        </div>

        <div className="card">
          <div className="row" style={{ marginBottom: 4 }}>
            <h2 style={{ flex: 1 }}>ByeDPI</h2>
            <span className="sub" style={{ fontSize: 12 }}>
              {snap.engine === "byedpi" ? "сейчас работает этот движок" : "запасной движок обхода"}
            </span>
          </div>
          <p className="sub" style={{ fontSize: 12, marginBottom: 4 }}>
            Локальный SOCKS5-прокси вместо драйвера: ломает запросы по дороге к серверу.
            Прав администратора не требует и WinDivert не занимает, но заворачивает только TCP тех
            приложений, что читают системный прокси, — голос Discord и игры остаются за zapret.
          </p>

          {bye.installed ? (
            <>
              <Row
                title={bye.managed ? "Файлом управляет приложение" : "Подключена твоя папка"}
                desc={bye.dir ?? "не выбрана"}
              >
                <button className="btn sm ghost" onClick={onPickByedpiFolder} disabled={busy}>
                  <Folder /> Выбрать другую
                </button>
              </Row>
              <Row
                title={`Установлена версия ${bye.version ?? "неизвестна"}`}
                desc={
                  byeUpdate?.error
                    ? `Не удалось проверить: ${byeUpdate.error}`
                    : byeUpdate?.latest
                      ? byeUpdate.hasUpdate
                        ? `На GitHub уже ${byeUpdate.latest} от ${byeUpdate.release?.publishedAt ?? ""}`
                        : "Это последняя версия"
                      : "Беру релизы из hufrea/byedpi"
                }
              >
                <button className="btn sm ghost" onClick={onCheckByedpiUpdate} disabled={byeChecking}>
                  {byeChecking ? <Spinner /> : <Refresh />} Проверить
                </button>
                {byeUpdate?.hasUpdate && (
                  <button className="btn sm primary" onClick={onUpdateByedpi} disabled={busy}>
                    {busy ? <Spinner /> : <Download />} Обновить
                  </button>
                )}
              </Row>
              <Row
                title="Заворачивать трафик автоматически"
                desc={
                  bye.systemProxyActive
                    ? "Системный прокси Windows сейчас направлен в ByeDPI"
                    : "Пропишу ByeDPI в системные настройки прокси при запуске обхода и уберу оттуда при остановке"
                }
              >
                <Switch on={bye.systemProxy} onChange={onSystemProxy} disabled={busy} />
              </Row>
              <Row
                title="Порт прокси"
                desc={`ByeDPI слушает 127.0.0.1:${bye.port}. Меняй, если порт уже кем-то занят`}
              >
                <input
                  className="input"
                  style={{ width: 92 }}
                  inputMode="numeric"
                  value={port}
                  onChange={(e) => setPort(e.target.value.replace(/\D/g, "").slice(0, 5))}
                />
                <button
                  className="btn sm"
                  disabled={busy || Number(port) === bye.port || !Number(port)}
                  onClick={() => onByedpiPort(Number(port))}
                >
                  Применить
                </button>
              </Row>
              <Row
                title="Включать ByeDPI при запуске приложения"
                desc="Своей службы у ByeDPI нет: прокси поднимается вместе с окном. Вместе с автозапуском приложения это даёт обход сразу после входа в Windows"
              >
                <Switch
                  on={cfg.byedpiAutostart}
                  onChange={(v) => onOption("byedpiAutostart", v)}
                  disabled={busy}
                />
              </Row>
            </>
          ) : (
            <Row
              title="ByeDPI не установлен"
              desc="Скачаю последний релиз с GitHub — это один файл ciadpi.exe. Или укажи папку, если он уже есть"
            >
              <button className="btn sm ghost" onClick={onPickByedpiFolder} disabled={busy}>
                <Folder /> Указать папку
              </button>
              <button className="btn sm primary" onClick={onInstallByedpi} disabled={busy}>
                {busy ? <Spinner /> : <Globe />} Скачать
              </button>
            </Row>
          )}
        </div>

        <div className="card">
          <h2 style={{ marginBottom: 10 }}>Инструменты zapret</h2>
          <p className="sub" style={{ marginBottom: 4, fontSize: 12 }}>
            То же, что в меню service.bat, только без консоли. Игровой фильтр и список адресов
            переключаются на главной.
          </p>

          <Row
            title="Обновить список адресов"
            desc="Свежий ipset-all.txt из репозитория — помогает, когда обход перестал ловить нужные сервисы"
          >
            <button className="btn sm" onClick={onUpdateIpset} disabled={busy || !snap.dirOk}>
              {busy ? <Spinner /> : <Refresh />} Обновить
            </button>
          </Row>

          {snap.fakes && snap.fakes.available.length > 0 && (
            <>
              <Row
                title="Фейк для Discord (UDP)"
                desc={`Сейчас: ${snap.fakes.discord ?? "не распознан"}. Меняй, если голос в Discord заикается`}
              >
                <select
                  className="input"
                  style={{ width: 210 }}
                  value={snap.fakes.discord ?? ""}
                  onChange={(e) => onSetFake("discord", e.target.value)}
                  disabled={busy}
                >
                  {!snap.fakes.discord && <option value="">не распознан</option>}
                  {snap.fakes.available.map((f) => (
                    <option key={f} value={f}>
                      {f}
                    </option>
                  ))}
                </select>
              </Row>
              <Row
                title="Фейк для игр (UDP)"
                desc={`Сейчас: ${snap.fakes.game ?? "не распознан"}. Работает вместе с игровым фильтром`}
              >
                <select
                  className="input"
                  style={{ width: 210 }}
                  value={snap.fakes.game ?? ""}
                  onChange={(e) => onSetFake("game", e.target.value)}
                  disabled={busy}
                >
                  {!snap.fakes.game && <option value="">не распознан</option>}
                  {snap.fakes.available.map((f) => (
                    <option key={f} value={f}>
                      {f}
                    </option>
                  ))}
                </select>
              </Row>
            </>
          )}

          <Row
            title="Записи в hosts"
            desc={
              hosts
                ? hosts.upToDate
                  ? `Все ${hosts.total} записей на месте`
                  : `Не хватает ${hosts.missing} из ${hosts.total} записей`
                : "Прописывает адреса Discord напрямую в обход подмены DNS"
            }
          >
            <button className="btn sm ghost" onClick={checkHosts} disabled={hostsBusy}>
              {hostsBusy ? <Spinner /> : <Refresh />} Проверить
            </button>
            {hosts && !hosts.upToDate && (
              <button className="btn sm" onClick={onApplyHosts} disabled={busy}>
                Дописать
              </button>
            )}
          </Row>

          <Row
            title="Очистить кэш Discord"
            desc="Закроет Discord и удалит кэш — стандартное лекарство, когда картинки и голос не грузятся"
          >
            <button className="btn sm" onClick={onClearDiscord} disabled={busy}>
              Очистить
            </button>
          </Row>
        </div>

        <div className="card">
          <h2 style={{ marginBottom: 4 }}>Cloudflare WARP</h2>
          <p className="sub" style={{ fontSize: 12, marginBottom: 4 }}>
            Не обход, а туннель, но живёт рядом: в России серверы Cloudflare часто недоступны,
            и WARP не подключается, пока не включишь обход. Обратное тоже верно — включённый
            WARP уводит в туннель весь трафик, включая проверку стратегий.
          </p>

          {snap.warp.installed ? (
            <Row
              title={snap.warp.connected ? "Подключён" : "Отключён"}
              desc={`${snap.warp.detail}${snap.warp.mode ? ` · режим ${snap.warp.mode}` : ""}`}
            >
              <Switch on={snap.warp.connected} onChange={onWarp} disabled={busy} />
            </Row>
          ) : (
            <Row
              title="Не установлен"
              desc="Это отдельная программа Cloudflare со своим установщиком — приложение её не ставит и не обновляет. Пока её нет, переключателя WARP не будет ни здесь, ни на главной"
            >
              <button className="btn sm primary" onClick={onInstallWarp}>
                <Globe /> Скачать
              </button>
            </Row>
          )}

          <Row
            title="Работает ли туннель на самом деле"
            desc={
              probe
                ? probe.error
                  ? probe.error
                  : probe.active
                    ? `Да: Cloudflare видит нас из ${probe.loc || "?"} (${probe.ip}), узел ${probe.colo} · проверено в ${probe.checkedAt}`
                    : `Нет: Cloudflare видит обычный адрес ${probe.ip} из ${probe.loc || "?"} · проверено в ${probe.checkedAt}`
                : "Спрошу у самого Cloudflare. Служба WARP умеет считать себя подключённой, когда трафик в туннель уже не идёт, — здесь видно, как есть"
            }
          >
            <span
              className={`pill ${probe && !probe.error ? (probe.active ? "live" : "best") : ""}`}
              style={{ whiteSpace: "nowrap" }}
            >
              {probe ? (probe.error ? "не ответил" : `warp=${probe.value}`) : "не проверяли"}
            </span>
            <button className="btn sm" onClick={checkWarp} disabled={probing}>
              {probing ? <Spinner /> : <Refresh />} Проверить
            </button>
          </Row>
          <Row
            title="Проверить серверы Cloudflare"
            desc="Их состояние показывает группа «Cloudflare» на вкладке «Проверка» — по ней видно, чинит ли обход подключение WARP"
          >
            <span className="sub" style={{ fontSize: 12 }}>
              {snap.warp.connected ? "сейчас меряется туннель, а не обход" : "готово к замеру"}
            </span>
          </Row>
        </div>

        <div className="card">
          <div className="row" style={{ marginBottom: 4 }}>
            <h2 style={{ flex: 1 }}>Диагностика системы</h2>
            {snap.conflicts.length > 0 && (
              <button className="btn sm danger" onClick={onStopConflicts} disabled={busy}>
                {busy ? <Spinner /> : null} Остановить чужие обходы
              </button>
            )}
            <button className="btn sm" onClick={runDiag} disabled={diagBusy}>
              {diagBusy ? <Spinner /> : <Shield />} Проверить
            </button>
          </div>
          <p className="sub" style={{ fontSize: 12 }}>
            Ищу причины, по которым обход может не работать: конфликты с другими обходами, прокси,
            отсутствующий драйвер. Отдельно показываю внешний адрес — если страна не твоя, трафик
            уже идёт через VPN, и проверка меряет его, а не обход.
          </p>
          <AnimatePresence>
            {checks && (
              <motion.div
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: "auto" }}
                exit={{ opacity: 0, height: 0 }}
                style={{ overflow: "hidden", marginTop: 8 }}
              >
                {checks.map((c, i) => (
                  <motion.div
                    key={c.title}
                    className="check"
                    initial={{ opacity: 0, x: -8 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ delay: i * 0.05 }}
                  >
                    <span className={`check-icon ${c.level}`}>
                      {c.level === "ok" ? "✓" : c.level === "warn" ? "!" : "×"}
                    </span>
                    <div style={{ minWidth: 0, flex: 1 }}>
                      <div style={{ fontWeight: 550 }}>{c.title}</div>
                      <div className="sub" style={{ fontSize: 12, wordBreak: "break-all" }}>
                        {c.detail}
                      </div>
                      {c.hint && <div style={{ fontSize: 12, color: "var(--warn)", marginTop: 3 }}>{c.hint}</div>}
                    </div>
                    {c.items && c.items.length > 0 && (
                      <button className="btn sm danger" onClick={onStopConflicts} disabled={busy}>
                        Остановить
                      </button>
                    )}
                  </motion.div>
                ))}
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        <div className="card" style={{ color: "var(--dim)", fontSize: 12 }}>
          Zapret Studio {snap.appVersion} — оболочка над пятью чужими программами:{" "}
          {ENGINES.map((e, i) => (
            <span key={e.id}>
              <span style={{ color: "var(--muted)" }}>{e.repo}</span>
              {i < ENGINES.length - 2 ? ", " : i === ENGINES.length - 2 ? " и " : ". "}
            </span>
          ))}
          Сам обход делают они, приложение лишь удобно ими управляет.
        </div>
      </div>
    </div>
  );
}
