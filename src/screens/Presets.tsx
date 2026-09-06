import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { Alert, Crown, Download, Folder, Globe, Link, Pencil, Plus, Power, Refresh, Shield, Trash } from "../icons";
import { Spinner } from "../components/ui";
import { engineInfo, isCore } from "../engines";
import type { CoreState, Preset, PresetEngine, ServerPing, Snapshot } from "../types";

/** Серверы ядра: подписка, список и выбор. */
function Servers({
  core,
  busy,
  onServer,
  onSubscription,
  onSelect,
  onPing,
}: {
  core: CoreState;
  busy: boolean;
  onServer: (url: string) => void;
  onSubscription: (url: string) => void;
  onSelect: (index: number) => void;
  onPing: () => Promise<ServerPing[]>;
}) {
  const [server, setServer] = useState("");
  const [sub, setSub] = useState(core.subscription ?? "");
  const [pings, setPings] = useState<Record<number, number | null>>({});
  const [pinging, setPinging] = useState(false);

  const measure = async () => {
    setPinging(true);
    try {
      const list = await onPing();
      setPings(Object.fromEntries(list.map((p) => [p.index, p.ms])));
    } finally {
      setPinging(false);
    }
  };

  return (
    <div className="card" style={{ marginBottom: 14 }}>
      <div className="row" style={{ marginBottom: 6 }}>
        <span style={{ color: core.server ? "var(--ok)" : "var(--dim)", display: "flex" }}>
          <Link />
        </span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontWeight: 550 }}>Свой сервер</div>
          <div className="sub" style={{ fontSize: 12.5, marginTop: 2 }}>
            {core.serverError
              ? `Ссылка сохранена, но не разбирается: ${core.serverError}`
              : core.server
                ? `${core.server} — пресеты «через свой сервер» готовы`
                : "Нужен только пресетам «через свой сервер». Фрагментация работает и без него"}
          </div>
        </div>
        {core.servers.length > 1 && (
          <button className="btn sm ghost" onClick={measure} disabled={busy || pinging}>
            {pinging ? <Spinner /> : <Refresh />} Замерить
          </button>
        )}
      </div>

      <div className="server-row">
        <input
          className="input mono"
          spellCheck={false}
          placeholder="vless://… vmess://… trojan://… ss://…"
          value={server}
          onChange={(e) => setServer(e.target.value)}
        />
        <button
          className="btn sm"
          disabled={busy || (!server.trim() && !core.server)}
          onClick={() => {
            onServer(server.trim());
            setServer("");
          }}
        >
          {busy ? <Spinner /> : <Link />} Сохранить
        </button>
      </div>

      <div className="server-row">
        <input
          className="input mono"
          spellCheck={false}
          placeholder="https://панель/подписка — заберу все серверы разом"
          value={sub}
          onChange={(e) => setSub(e.target.value)}
        />
        <button className="btn sm" disabled={busy || !sub.trim()} onClick={() => onSubscription(sub.trim())}>
          {busy ? <Spinner /> : <Download />} Загрузить
        </button>
      </div>

      {core.servers.length > 0 && (
        <div className="list" style={{ marginTop: 10 }}>
          {core.servers.map((label, i) => {
            const ms = pings[i];
            return (
              <div
                key={label + i}
                className={`item ${core.selectedServer === i ? "selected" : ""}`}
                onClick={() => !busy && onSelect(i)}
              >
                <span className={`dot ${core.selectedServer === i ? "on" : "off"}`} />
                <span className="name" style={{ flex: 1, minWidth: 0, fontSize: 12.5 }}>
                  {label}
                </span>
                {ms === undefined ? null : ms === null ? (
                  <span className="pill" style={{ color: "var(--bad)" }}>
                    не отвечает
                  </span>
                ) : (
                  <span className="pill" style={{ color: ms < 150 ? "var(--ok)" : "var(--warn)" }}>
                    {ms} мс
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

/** Форма своего пресета: название плюс строка параметров */
function Editor({
  preset,
  hint,
  sample,
  busy,
  onSave,
  onCancel,
}: {
  preset: Preset | null;
  hint: React.ReactNode;
  sample: string;
  busy: boolean;
  onSave: (id: string | null, name: string, args: string) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(preset?.name ?? "");
  const [args, setArgs] = useState(preset?.args.join(" ") ?? sample);

  return (
    <motion.div
      className="card"
      initial={{ opacity: 0, y: -10, height: 0 }}
      animate={{ opacity: 1, y: 0, height: "auto" }}
      exit={{ opacity: 0, y: -10, height: 0 }}
      style={{ overflow: "hidden", marginBottom: 14 }}
    >
      <h2 style={{ marginBottom: 10 }}>{preset ? "Изменить пресет" : "Свой пресет"}</h2>
      <input
        className="input"
        placeholder="Название — например «Мой вариант для Ростелекома»"
        value={name}
        onChange={(e) => setName(e.target.value)}
        style={{ marginBottom: 9 }}
      />
      <textarea
        className="input mono"
        rows={3}
        spellCheck={false}
        placeholder={sample}
        value={args}
        onChange={(e) => setArgs(e.target.value)}
      />
      <div className="sub" style={{ fontSize: 12, marginTop: 8 }}>
        {hint}
      </div>
      <div className="row" style={{ marginTop: 12, justifyContent: "flex-end" }}>
        <button className="btn sm ghost" onClick={onCancel}>
          Отмена
        </button>
        <button
          className="btn sm primary"
          disabled={busy}
          onClick={() => onSave(preset?.id ?? null, name, args)}
        >
          {busy ? <Spinner /> : null} Сохранить
        </button>
      </div>
    </motion.div>
  );
}

export default function Presets({
  snap,
  busy,
  onSelect,
  onStart,
  onSave,
  onDelete,
  onInstall,
  onPickFolder,
  onUpdateBlacklist,
  onServer,
  onSubscription,
  onSelectServer,
  onPingServers,
}: {
  snap: Snapshot;
  busy: boolean;
  onSelect: (id: string) => void;
  onStart: (id: string) => void;
  onSave: (id: string | null, name: string, args: string) => void;
  onDelete: (id: string) => void;
  onInstall: () => void;
  onPickFolder: () => void;
  onUpdateBlacklist: () => void;
  onServer: (url: string) => void;
  onSubscription: (url: string) => void;
  onSelectServer: (index: number) => void;
  onPingServers: () => Promise<ServerPing[]>;
}) {
  const goodbye = snap.engine === "goodbyedpi";
  // У ядер пресет — это способ собрать конфиг, а не строка ключей: свои
  // наборы им не подставить, поэтому редактор для них не показываем
  const core = isCore(snap.engine) ? snap[snap.engine] : null;
  const info = engineInfo(snap.engine);
  const view: PresetEngine = core ?? (goodbye ? snap.goodbye : snap.byedpi);
  const [editing, setEditing] = useState<Preset | null | "new">(null);

  const meta = core
    ? {
        title: `Пресеты ${info.label}`,
        intro: `Пресет — это готовый конфиг ${info.label}: как резать TLS-приветствие или куда уводить трафик.`,
        emptyTitle: `${info.label} ещё не установлен`,
        emptyDesc: `Возьму последний релиз ${info.repo} с GitHub. Или укажи папку, если ядро у тебя уже есть.`,
        about: info.short,
        sample: "",
        hint: null as React.ReactNode,
      }
    : goodbye
    ? {
        title: "Пресеты GoodbyeDPI",
        intro:
          "Пресет — это набор ключей, с которыми запускается goodbyedpi: какой режим обхода включить, слать ли поддельный ClientHello, применять ли обход только к списку заблокированных доменов.",
        emptyTitle: "GoodbyeDPI ещё не установлен",
        emptyDesc:
          "Возьму последнюю сборку ValdikSS/GoodbyeDPI с GitHub — вместе с драйвером и списками заблокированных доменов. Или укажи папку, если она уже есть.",
        about:
          "Ещё один обход на драйвере WinDivert, как zapret, но настраивается не файлами стратегий, а ключами запуска. Требует прав администратора и не может работать одновременно с zapret — драйвер у них общий.",
        sample: "-9 --blacklist %ROOT%russia-blacklist.txt",
        hint: (
          <>
            Пиши ключи так же, как в командной строке goodbyedpi.{" "}
            <span className="mono">%ROOT%</span> заменю на папку установки — так удобно ссылаться на
            списки доменов. Режимы <span className="mono">-1</span>…<span className="mono">-9</span>{" "}
            сами разворачиваются в наборы опций, полный список ключей — в README GoodbyeDPI.
          </>
        ),
      }
    : {
        title: "Пресеты ByeDPI",
        intro:
          "Пресет — это набор ключей, с которыми запускается ciadpi: где разрезать запрос, слать ли поддельный пакет, с каким TTL.",
        emptyTitle: "ByeDPI ещё не установлен",
        emptyDesc:
          "Возьму последний релиз hufrea/byedpi с GitHub — это один файл ciadpi.exe весом меньше сотни килобайт. Или укажи папку, если он у тебя уже есть.",
        about:
          "Не драйвер, а обычная программа: поднимает у тебя локальный SOCKS5-прокси и на лету ломает исходящие запросы так, что DPI не понимает, куда ты идёшь. Прав администратора не требует и с zapret не спорит за WinDivert.",
        sample: "--split 1+s --disorder 3+s",
        hint: (
          <>
            Пиши параметры так же, как в командной строке ciadpi. Адрес и порт подставлю сам —
            писать <span className="mono">--ip</span> и <span className="mono">--port</span> не
            нужно. Полный список ключей — в README ByeDPI.
          </>
        ),
      };

  if (!view.installed) {
    return (
      <div className="screen">
        <div className="head">
          <h1>{info.label}</h1>
          <p className="sub">{meta.about}</p>
        </div>
        <div className="card">
          <div className="row">
            <span className="rank" style={{ flex: "none" }}>
              {goodbye ? <Shield /> : <Globe />}
            </span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontWeight: 600 }}>{meta.emptyTitle}</div>
              <div className="sub" style={{ fontSize: 12.5, marginTop: 2 }}>
                {meta.emptyDesc}
              </div>
            </div>
            <button className="btn ghost sm" onClick={onPickFolder} disabled={busy}>
              <Folder /> Указать папку
            </button>
            <button className="btn primary" onClick={onInstall} disabled={busy}>
              {busy ? <Spinner /> : <Download />} Скачать
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="screen">
      <div className="head">
        <h1>{meta.title}</h1>
        <p className="sub">
          {meta.intro} Что сработает у твоего провайдера, заранее не знает никто — на то есть
          вкладка «Проверка».
        </p>
      </div>

      <div className="card" style={{ marginBottom: 14, padding: 14 }}>
        <div className="row">
          <span className={`dot ${view.running ? "on" : "off"}`} />
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontWeight: 550 }}>
              {goodbye
                ? view.running
                  ? "Обход работает"
                  : "Обход выключен"
                : view.running
                  ? `Прокси работает на 127.0.0.1:${core ? core.port : snap.byedpi.port}`
                  : "Прокси выключен"}
            </div>
            <div className="sub" style={{ fontSize: 12.5, marginTop: 2 }}>
              {goodbye
                ? snap.goodbye.blacklist != null
                  ? `В списке заблокированного ${snap.goodbye.blacklist} доменов — по нему работают пресеты «Россия»`
                  : "Списка заблокированного нет — работать будут только пресеты без --blacklist"
                : (core ?? snap.byedpi).systemProxyActive
                  ? `Системный прокси Windows направлен сюда — браузеры и Discord идут через ${info.label}`
                  : (core ?? snap.byedpi).systemProxy
                    ? "Системный прокси пропишу сама, как только включишь обход"
                    : `Трафик заворачивать не буду — впиши прокси 127.0.0.1:${core ? core.port : snap.byedpi.port} в приложении вручную`}
            </div>
          </div>
          {goodbye && (
            <button className="btn sm ghost" onClick={onUpdateBlacklist} disabled={busy}>
              {busy ? <Spinner /> : <Refresh />} Обновить список
            </button>
          )}
          <span className="sub" style={{ fontSize: 12 }}>
            {info.label} {view.version ?? "—"}
          </span>
        </div>
      </div>

      {core ? (
        <Servers
          core={core}
          busy={busy}
          onServer={onServer}
          onSubscription={onSubscription}
          onSelect={onSelectServer}
          onPing={onPingServers}
        />
      ) : (
        <div className="row" style={{ marginBottom: 14 }}>
          <div style={{ flex: 1 }} />
          <button className="btn sm" onClick={() => setEditing("new")} disabled={busy}>
            <Plus /> Свой пресет
          </button>
        </div>
      )}

      <AnimatePresence>
        {editing !== null && (
          <Editor
            key={editing === "new" ? "new" : editing.id}
            preset={editing === "new" ? null : editing}
            hint={meta.hint}
            sample={meta.sample}
            busy={busy}
            onCancel={() => setEditing(null)}
            onSave={(id, name, args) => {
              onSave(id, name, args);
              setEditing(null);
            }}
          />
        )}
      </AnimatePresence>

      <div className="list">
        {view.presets.map((p, i) => {
          const isSel = p.id === view.selected;
          const isRunning = view.running && view.current === p.id;
          return (
            <motion.div
              key={p.id}
              layout
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: Math.min(i * 0.02, 0.25) }}
              className={`item ${isSel ? "selected" : ""}`}
              style={{ flexDirection: "column", alignItems: "stretch", gap: 0 }}
              onClick={() => onSelect(p.id)}
            >
              <div className="row">
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="row" style={{ gap: 8 }}>
                    <span className="name">{p.name}</span>
                    {view.best === p.id && (
                      <span className="pill best" style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
                        <Crown /> лучшая
                      </span>
                    )}
                    {isRunning && <span className="pill live">работает</span>}
                    {!p.builtin && <span className="pill">своя</span>}
                  </div>
                  <div className="sub" style={{ fontSize: 12.5, marginTop: 3 }}>
                    {p.desc}
                  </div>
                  <div className="mono" style={{ fontSize: 11.5, color: "var(--dim)", marginTop: 6, userSelect: "text" }}>
                    {p.args.join(" ")}
                  </div>
                  {core && p.id.startsWith("server") && !core.server && (
                    <div
                      style={{
                        fontSize: 12,
                        marginTop: 5,
                        color: "var(--warn)",
                        display: "flex",
                        gap: 6,
                        alignItems: "center",
                      }}
                    >
                      <Alert /> Не запустится, пока не вставишь ссылку на свой сервер
                    </div>
                  )}
                </div>

                {!p.builtin && (
                  <>
                    <button
                      className="btn sm ghost"
                      title="Изменить"
                      disabled={busy}
                      onClick={(e) => {
                        e.stopPropagation();
                        setEditing(p);
                      }}
                    >
                      <Pencil />
                    </button>
                    <button
                      className="btn sm ghost"
                      title="Удалить"
                      disabled={busy}
                      onClick={(e) => {
                        e.stopPropagation();
                        onDelete(p.id);
                      }}
                    >
                      <Trash />
                    </button>
                  </>
                )}
                <button
                  className={`btn sm ${isSel ? "primary" : ""}`}
                  disabled={busy}
                  onClick={(e) => {
                    e.stopPropagation();
                    onStart(p.id);
                  }}
                >
                  <Power /> {isRunning ? "Перезапустить" : "Включить"}
                </button>
              </div>
            </motion.div>
          );
        })}
      </div>
    </div>
  );
}
