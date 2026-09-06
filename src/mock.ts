/**
 * Заглушка для разработки интерфейса в обычном браузере.
 * В собранном приложении не используется: там всегда есть Tauri-мост.
 */
import type { AppConfig, Check, Core, CoreSettings, CoreState, Engine, GoodbyeState, Health, LogLine, Preset, ServerPing, Snapshot, StrategyResult, UpdateCheck, WarpProbe } from "./types";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const NAMES = [
  "general",
  "general (ALT)",
  "general (ALT2)",
  "general (ALT3)",
  "general (ALT4)",
  "general (ALT5)",
  "general (ALT6)",
  "general (ALT7)",
  "general (EXP)",
  "general (FAKE TLS AUTO)",
  "general (FAKE TLS AUTO ALT)",
  "general (SIMPLE FAKE)",
];

/** Те же пресеты, что и в byedpi.rs, — интерфейсу хватает названий и параметров */
const PRESETS: Preset[] = [
  ["split-disorder", "Разрыв по SNI", "Рекомендация ByeDPI для Windows: режем запрос на имени сайта и отправляем куски не по порядку", "--split 1+s --disorder 3+s"],
  ["fake-disorder", "Фейк + беспорядок", "Перед настоящим запросом уходит поддельный с коротким TTL", "--disorder 1 --fake -1 --ttl 8"],
  ["tlsrec-sni", "Разрыв TLS-записи", "Заголовок новой TLS-записи вставляется в середину имени сайта", "--tlsrec 3+s"],
  ["auto-tlsrec", "Только при блокировке", "Ломает запрос лишь после сброса или таймаута", "--auto=torst --timeout 3 --tlsrec 3+s"],
  ["split-mid-sni", "Разрыв в середине SNI", "Один разрез ровно посередине имени сайта", "--split 0+sm"],
  ["oob-sni", "OOB-байт в SNI", "В имя сайта подкладывается байт вне основного потока", "--auto=torst --timeout 3 --oob 3+s"],
].map(([id, name, desc, args]) => ({ id, name, desc, args: args.split(" "), builtin: true }));

const GOODBYE_PRESETS: Preset[] = [
  ["ru-youtube", "Россия: список + YouTube", "Основной вариант из комплекта GoodbyeDPI: режим 9 плюс поддельный ClientHello", "-9 --fake-gen 5 --blacklist russia-blacklist.txt"],
  ["ru-youtube-alt", "Россия: список + YouTube (ALT)", "Запасной вариант: режим 5 с автоподбором TTL", "-5 -e1 -q --fake-gen 5"],
  ["ru-blacklist", "Россия: только список", "Режим 9 по спискам заблокированного", "-9 --blacklist russia-blacklist.txt"],
  ["any-country", "Любая страна", "Режим 9 без списков", "-9"],
  ["mode-5", "Режим 5: автоподбор TTL", "-f 2 -e 2 --auto-ttl --reverse-frag --max-payload", "-5"],
].map(([id, name, desc, args]) => ({ id, name, desc, args: args.split(" "), builtin: true }));

/** Пресеты ядер — те же, что в xray.rs и singbox.rs */
const XRAY_PRESETS: Preset[] = [
  ["frag-tlshello", "Фрагментация TLS-приветствия", "Приветствие TLS уходит кусками по 100–200 байт с паузой в 10–20 мс", "fragment: tlshello · куски 100–200 байт · пауза 10–20 мс"],
  ["frag-small", "Мелкие куски", "То же самое, но куски по 10–20 байт: медленнее, зато проходит чаще", "fragment: tlshello · куски 10–20 байт · пауза 10–20 мс"],
  ["frag-packets", "Резать первые пакеты", "Режем первые три записи потока целиком", "fragment: пакеты 1–3 · куски 100–200 байт · пауза 10–20 мс"],
  ["frag-noise", "Фрагментация и шум", "К фрагментации добавляется случайный мусор перед UDP-пакетами", "fragment + noises: случайные 10–20 байт перед UDP"],
  ["server", "Через свой сервер", "Весь трафик уходит на сервер из твоей ссылки", "outbound по ссылке · нужен свой сервер"],
  ["server-frag", "Свой сервер и фрагментация", "Тот же туннель, но приветствие TLS к серверу тоже режется", "outbound по ссылке через фрагментирующий dialerProxy"],
].map(([id, name, desc, shape]) => ({ id, name, desc, args: [shape], builtin: true }));

const SINGBOX_PRESETS: Preset[] = [
  ["frag", "Фрагментация TLS", "Приветствие TLS уходит частями — DPI не собирает из них имя сайта", "route-options: tls_fragment"],
  ["frag-slow", "Фрагментация с паузой", "Между частями ядро выжидает полсекунды", "route-options: tls_fragment + пауза 500 мс"],
  ["record-frag", "Разрыв TLS-записи", "Приветствие раскладывается на несколько TLS-записей", "route-options: tls_record_fragment"],
  ["server", "Через свой сервер", "Весь трафик уходит на сервер из твоей ссылки", "outbound по ссылке · нужен свой сервер"],
].map(([id, name, desc, shape]) => ({ id, name, desc, args: [shape], builtin: true }));

const coreSettings = (port: number, version: string): CoreSettings => ({
  dir: null,
  managed: true,
  version,
  preset: null,
  port,
  systemProxy: true,
  autostart: false,
  server: null,
  best: null,
  lastTestAt: null,
});

const coreState = (presets: Preset[], port: number, version: string): CoreState => ({
  installed: false,
  dir: null,
  managed: true,
  managedDir: `C:\Users\Marakabo\AppData\Roaming\com.marakabo.zapret-studio\${port === 1081 ? "xray" : "singbox"}`,
  version,
  running: false,
  current: null,
  autostart: false,
  presets,
  selected: presets[0].id,
  best: null,
  lastTestAt: null,
  port,
  systemProxy: true,
  systemProxyActive: false,
  server: null,
  serverError: null,
  servers: [],
  selectedServer: null,
  subscription: null,
});

const label = (n: string) => {
  const m = n.match(/\((.+)\)/);
  return m ? m[1] : "базовая";
};

const state: Snapshot = {
  config: {
    zapretDir: "C:\\Users\\Marakabo\\AppData\\Roaming\\com.marakabo.zapret-studio\\zapret",
    managed: true,
    installedVersion: "1.10.2",
    selectedStrategy: "general (ALT2)",
    appAutostart: false,
    startMinimized: false,
    autoCheckUpdates: true,
    autostartBypass: false,
    gameFilter: "off",
    onboarded: !(typeof window !== "undefined" && window.location.search.includes("onboard")),
    bestStrategy: "general (ALT2)",
    lastTestAt: "2026-09-04 20:41:07",
    autoInstallUpdates: false,
    updateIntervalHours: 6,
    lastUpdateCheck: "2026-09-04 22:10:00",
    engine: "zapret",
    byedpiDir: "C:\Users\Marakabo\AppData\Roaming\com.marakabo.zapret-studio\byedpi",
    byedpiManaged: true,
    byedpiVersion: "0.17.3",
    byedpiPreset: "split-disorder",
    byedpiPort: 1080,
    byedpiSystemProxy: true,
    byedpiAutostart: false,
    byedpiCustom: [],
    byedpiBest: null,
    byedpiLastTestAt: null,
    savedProxy: null,
    goodbyeDir: "C:\\Users\\Marakabo\\AppData\\Roaming\\com.marakabo.zapret-studio\\goodbyedpi",
    goodbyeManaged: true,
    goodbyeVersion: "0.2.3rc3",
    goodbyePreset: "ru-youtube",
    goodbyeAutostart: false,
    goodbyeCustom: [],
    goodbyeBest: null,
    goodbyeLastTestAt: null,
    xray: coreSettings(1081, "26.3.27"),
    singbox: coreSettings(1082, "1.14.0"),
    customTargets: ["rutracker.org", "example.com:443"],
    appRepo: null,
    watchdog: true,
    watchdogIntervalMin: 15,
    watchdogAutoFix: false,
  },
  dirOk: true,
  strategies: NAMES.map((n, i) => ({
    file: `${n}.bat`,
    name: n,
    label: label(n),
    args: ["--wf-tcp=80,443", "--dpi-desync=multisplit", "--dpi-desync-split-pos=1"],
    tags: i % 3 === 0 ? ["fake", "multisplit"] : i % 3 === 1 ? ["fake", "fakedsplit"] : ["multisplit"],
  })),
  running: true,
  current: "general (ALT2)",
  service: { installed: false, running: false, strategy: null },
  autostart: false,
  testing: false,
  appVersion: "1.3.0",
  managedDir: "C:\\Users\\Marakabo\\AppData\\Roaming\\com.marakabo.zapret-studio\\zapret",
  ipsetMode: "loaded",
  fakes: {
    available: ["quic_initial_www_google_com", "stun", "stun2", "tls_clienthello_4pda_to"],
    discord: "stun",
    game: "stun2",
  },
  conflicts: [{ kind: "service", name: "ZapretDirectService" }],
  engine: "zapret",
  stray: null,
  warp: {
    installed: true,
    connected: false,
    detail: "Disconnected · Manual Disconnection",
    mode: "WarpWithDnsOverHttps",
    installUrl: "https://one.one.one.one/",
  },
  xray: coreState(XRAY_PRESETS, 1081, "26.3.27"),
  singbox: coreState(SINGBOX_PRESETS, 1082, "1.14.0"),
  checkTargets: ["rutracker.org", "example.com:443"],
  appRepo: "marakaybo/zapret-studio",
  goodbye: {
    installed: true,
    dir: "C:\\Users\\Marakabo\\AppData\\Roaming\\com.marakabo.zapret-studio\\goodbyedpi",
    managed: true,
    managedDir: "C:\\Users\\Marakabo\\AppData\\Roaming\\com.marakabo.zapret-studio\\goodbyedpi",
    version: "0.2.3rc3",
    running: false,
    current: null,
    autostart: false,
    presets: GOODBYE_PRESETS,
    selected: "ru-youtube",
    best: null,
    lastTestAt: null,
    blacklist: 12483,
  } as GoodbyeState,
  byedpi: {
    installed: true,
    dir: "C:\\Users\\Marakabo\\AppData\\Roaming\\com.marakabo.zapret-studio\\byedpi",
    managed: true,
    managedDir: "C:\\Users\\Marakabo\\AppData\\Roaming\\com.marakabo.zapret-studio\\byedpi",
    version: "0.17.3",
    running: false,
    current: null,
    port: 1080,
    systemProxy: true,
    systemProxyActive: false,
    autostart: false,
    presets: PRESETS,
    selected: "split-disorder",
    best: null,
    lastTestAt: null,
  },
  error: null,
};

const groupsFor = (score: number) => [
  { group: "Discord", ok: score > 70 ? 4 : score > 40 ? 2 : 0, total: 4 },
  { group: "YouTube", ok: score > 80 ? 4 : score > 45 ? 3 : 1, total: 4 },
  { group: "Google", ok: score > 30 ? 2 : 1, total: 2 },
  { group: "Связь", ok: 2, total: 2 },
];

const targetsFor = (score: number) =>
  [
    ["discord", "Discord", "discord.com"],
    ["gateway", "Discord", "gateway.discord.gg"],
    ["dcdn", "Discord", "cdn.discordapp.com"],
    ["dmedia", "Discord", "updates.discord.com"],
    ["yt", "YouTube", "www.youtube.com"],
    ["ytshort", "YouTube", "youtu.be"],
    ["ytimg", "YouTube", "i.ytimg.com"],
    ["ytvideo", "YouTube", "redirector.googlevideo.com"],
    ["google", "Google", "www.google.com"],
    ["gstatic", "Google", "www.gstatic.com"],
    ["dns1", "Связь", "1.1.1.1:53"],
    ["dns2", "Связь", "8.8.8.8:53"],
  ].map(([id, group, l], i) => {
    const ok = i / 12 < score / 100;
    return {
      id,
      group,
      label: l,
      ok,
      status: ok ? 200 : null,
      ms: ok ? 40 + Math.round(Math.random() * 180) : null,
      error: ok ? null : "таймаут — трафик режется",
    };
  });

const result = (name: string, score: number, baseline = false): StrategyResult => ({
  strategy: name,
  label: label(name),
  baseline,
  started: true,
  error: null,
  ok: Math.round((score / 100) * 12),
  total: 12,
  score,
  avgMs: 60 + Math.round((100 - score) * 2),
  speedKbs: 400 + score * 40,
  groups: groupsFor(score),
  targets: targetsFor(score),
});

const wait = <T,>(v: T, ms = 260) => new Promise<T>((r) => setTimeout(() => r(v), ms));

export const mockApi = {
  snapshot: () => wait(state),
  setZapretDir: (path: string) => wait({ ...state, config: { ...state.config, zapretDir: path } }),
  checkUpdate: () =>
    wait<UpdateCheck>({
      current: "1.10.2",
      latest: "1.10.3",
      hasUpdate: true,
      release: {
        version: "1.10.3",
        publishedAt: "2026-09-01",
        notes: "",
        zipUrl: "",
        size: 1508077,
      },
      error: null,
    }),
  installZapret: () => wait(state, 900),
  cancelInstall: async () => {},
  selectStrategy: (name: string) => {
    state.config.selectedStrategy = name;
    return wait({ ...state, config: { ...state.config } });
  },
  start: (name?: string) => {
    state.running = true;
    if (state.engine === "goodbyedpi") {
      const id = name ?? state.goodbye.selected ?? state.goodbye.presets[0].id;
      state.goodbye = { ...state.goodbye, running: true, current: id, selected: id };
      state.current = state.goodbye.presets.find((p) => p.id === id)?.name ?? null;
    } else if (state.engine === "byedpi") {
      const id = name ?? state.byedpi.selected ?? state.byedpi.presets[0].id;
      state.byedpi = {
        ...state.byedpi,
        running: true,
        current: id,
        selected: id,
        systemProxyActive: state.byedpi.systemProxy,
      };
      state.current = state.byedpi.presets.find((p) => p.id === id)?.name ?? null;
    } else {
      state.current = name ?? state.config.selectedStrategy;
    }
    return wait({ ...state });
  },
  stop: () => {
    state.running = false;
    state.current = null;
    state.byedpi = { ...state.byedpi, running: false, current: null, systemProxyActive: false };
    state.goodbye = { ...state.goodbye, running: false, current: null };
    return wait({ ...state });
  },
  runTests: async () => wait([] as StrategyResult[], 400),
  cancelTests: async () => {},
  installService: () => wait(state),
  removeService: () => wait(state),
  setAppAutostart: () => wait(state),
  setOption: (key: string, value: unknown) => {
    (state.config as unknown as Record<string, unknown>)[key] = value;
    return wait({ ...state, config: { ...state.config } });
  },
  ping: () => wait({ ms: 40 + Math.round(Math.random() * 60), via: "напрямую" }, 120),
  measureSpeed: () => wait(2400 + Math.random() * 900, 1400),
  logs: () =>
    wait<LogLine[]>([
      { time: "20:38:11", kind: "info", text: "Запущена стратегия «general (ALT2)»" },
      { time: "20:38:11", kind: "out", text: "windivert initialized. capture is started." },
      { time: "20:38:12", kind: "out", text: "hostlist file lists/list-general.txt loaded, 2417 hosts" },
    ]),
  clearLogs: async () => {},
  diagnostics: () =>
    wait<Check[]>([
      { title: "Драйвер WinDivert", level: "ok", detail: "Файл WinDivert64.sys на месте", hint: null, items: [] },
      { title: "Конфликты с другими обходами", level: "fail", detail: "Обнаружено: служба ZapretDirectService", hint: "Два обхода делят драйвер WinDivert и мешают друг другу.", items: [{ kind: "service", name: "ZapretDirectService" }] },
      { title: "TCP timestamps", level: "ok", detail: "Включены", hint: null, items: [] },
      { title: "Внешний адрес", level: "warn", detail: "57.129.112.87 · страна FR", hint: "Если страна не твоя, трафик уже идёт через VPN или прокси — тогда проверка стратегий меряет их, а не обход.", items: [] },
    ]),
  stopConflicts: () => {
    state.conflicts = [];
    return wait({ snapshot: { ...state }, messages: ["Служба ZapretDirectService остановлена, автозапуск отключён"] }, 700);
  },
  setGameFilter: (mode: string) => {
    state.config.gameFilter = mode as AppConfig["gameFilter"];
    return wait({ snapshot: { ...state, config: { ...state.config } }, messages: ["Обход перезапущен с новыми настройками"] });
  },
  setIpsetMode: (mode: string) => {
    state.ipsetMode = mode as Snapshot["ipsetMode"];
    return wait({ snapshot: { ...state }, messages: ["Настройка применена"] });
  },
  updateIpset: () => wait({ snapshot: { ...state }, messages: ["Список адресов обновлён: 1842 записи"] }, 800),
  setActiveFake: (kind: "discord" | "game", source: string) => {
    if (state.fakes) state.fakes = { ...state.fakes, [kind]: source };
    return wait({ snapshot: { ...state }, messages: [`Активный фейк заменён на «${source}»`] });
  },
  clearDiscordCache: () => wait({ snapshot: { ...state }, messages: ["Discord: закрыт, очищено папок — 3"] }, 900),
  hostsStatus: () => wait({ total: 24, missing: 6, upToDate: false }, 600),
  applyHosts: () => wait("Дописано записей в hosts: 6", 700),
  openFolder: async () => {},
  quit: async () => {},

  // --- ByeDPI ---
  setEngine: (engine: Engine) => {
    state.engine = engine;
    state.running = false;
    state.current = null;
    state.byedpi = { ...state.byedpi, running: false, current: null, systemProxyActive: false };
    state.goodbye = { ...state.goodbye, running: false, current: null };
    const shown = engine === "byedpi" ? "ByeDPI" : engine === "goodbyedpi" ? "GoodbyeDPI" : "zapret";
    return wait({ snapshot: { ...state }, messages: [`Обход переключён на ${shown}`] });
  },
  setByedpiDir: (path: string) => {
    state.byedpi = { ...state.byedpi, dir: path, managed: false, installed: true };
    return wait({ ...state });
  },
  checkByedpiUpdate: () =>
    wait<UpdateCheck>({
      current: "0.17.3",
      latest: "0.17.3",
      hasUpdate: false,
      release: null,
      error: null,
    }),
  installByedpi: () => {
    state.byedpi = { ...state.byedpi, installed: true, version: "0.17.3" };
    return wait({ ...state }, 900);
  },
  selectPreset: (id: string) => {
    if (state.engine === "goodbyedpi") state.goodbye = { ...state.goodbye, selected: id };
    else state.byedpi = { ...state.byedpi, selected: id };
    return wait({ ...state });
  },
  savePreset: (id: string | null, name: string, args: string) => {
    const preset: Preset = { id: id ?? `custom-${Date.now()}`, name, desc: "Свой набор параметров", args: args.split(/\s+/), builtin: false };
    const rest = state.byedpi.presets.filter((p) => p.id !== preset.id);
    state.byedpi = { ...state.byedpi, presets: [...rest, preset], selected: preset.id };
    return wait({ ...state });
  },
  deletePreset: (id: string) => {
    state.byedpi = { ...state.byedpi, presets: state.byedpi.presets.filter((p) => p.id !== id) };
    return wait({ ...state });
  },
  setByedpiPort: (port: number) => {
    state.byedpi = { ...state.byedpi, port };
    return wait({ snapshot: { ...state }, messages: [`ByeDPI переехал на порт ${port}`] });
  },
  setSystemProxy: (engine: Engine, enable: boolean) => {
    if (engine === "xray" || engine === "singbox") {
      const core = state[engine];
      state[engine] = { ...core, systemProxy: enable, systemProxyActive: enable && core.running };
    } else {
      const bye = state.byedpi;
      state.byedpi = { ...bye, systemProxy: enable, systemProxyActive: enable && bye.running };
    }
    return wait({
      snapshot: { ...state },
      messages: [enable ? `Трафик пойдёт через ${engine}` : "Системный прокси выключен"],
    });
  },
  runPresetTests: async () => wait([] as StrategyResult[], 400),

  // --- Xray и sing-box ---
  setCoreDir: (engine: Core, path: string) => {
    state[engine] = { ...state[engine], dir: path, managed: false, installed: true };
    return wait({ ...state });
  },
  checkCoreUpdate: (engine: Core) =>
    wait<UpdateCheck>({
      current: state[engine].version,
      latest: state[engine].version,
      hasUpdate: false,
      release: null,
      error: null,
    }),
  installCore: (engine: Core) => {
    state[engine] = { ...state[engine], installed: true };
    return wait({ ...state }, 900);
  },
  setCorePort: (engine: Core, port: number) => {
    state[engine] = { ...state[engine], port };
    return wait({ snapshot: { ...state }, messages: [`Ядро переехало на порт ${port}`] });
  },
  loadSubscription: (engine: Core, url: string) => {
    const servers = ["vless · de1.example.com:443 · Reality", "trojan · nl2.example.com:8443 · TLS"];
    state[engine] = { ...state[engine], subscription: url, servers, selectedServer: 0, server: servers[0] };
    return wait({ snapshot: { ...state }, messages: [`Серверов из подписки — ${servers.length}`] }, 700);
  },
  selectServer: (engine: Core, index: number) => {
    state[engine] = { ...state[engine], selectedServer: index, server: state[engine].servers[index] };
    return wait({ snapshot: { ...state }, messages: ["Сервер выбран"] }, 300);
  },
  pingServers: (engine: Core) =>
    wait<ServerPing[]>(
      state[engine].servers.map((_, index) => ({ index, ms: 40 + Math.round(Math.random() * 160) })),
      900
    ),
  setCoreServer: (engine: Core, url: string) => {
    const server = url.trim() ? `${url.split("://")[0]} · сервер из ссылки` : null;
    state[engine] = { ...state[engine], server, serverError: null };
    return wait({
      snapshot: { ...state },
      messages: [server ? `Сервер сохранён: ${server}` : "Сервер забыт"],
    });
  },

  // --- GoodbyeDPI ---
  setGoodbyeDir: (path: string) => {
    state.goodbye = { ...state.goodbye, dir: path, managed: false, installed: true };
    return wait({ ...state });
  },
  checkGoodbyeUpdate: () =>
    wait<UpdateCheck>({ current: "0.2.3rc3", latest: "0.2.3rc3", hasUpdate: false, release: null, error: null }),
  installGoodbye: () => {
    state.goodbye = { ...state.goodbye, installed: true, version: "0.2.3rc3" };
    return wait({ ...state }, 900);
  },
  stopStray: () => {
    state.stray = null;
    return wait({ snapshot: { ...state }, messages: ["Стратегия «general» остановлена"] }, 600);
  },
  warpConnect: () => {
    state.warp = { ...state.warp, connected: true, detail: "Connected" };
    return wait({ snapshot: { ...state }, messages: ["WARP подключается — на это уходит пара секунд"] }, 700);
  },
  warpDisconnect: () => {
    state.warp = { ...state.warp, connected: false, detail: "Disconnected · Manual Disconnection" };
    return wait({ snapshot: { ...state }, messages: ["WARP отключён"] }, 500);
  },
  warpCheck: () =>
    wait<WarpProbe>(
      {
        active: state.warp.connected,
        value: state.warp.connected ? "on" : "off",
        ip: "104.28.1.1",
        loc: state.warp.connected ? "NL" : "RU",
        colo: state.warp.connected ? "AMS" : "DME",
        checkedAt: new Date().toLocaleTimeString("ru-RU"),
        error: null,
      },
      700
    ),
  openUrl: () => wait(undefined, 100),
  setCustomTargets: (items: string[]) => {
    state.config = { ...state.config, customTargets: items };
    state.checkTargets = items;
    return wait({ ...state });
  },
  healthCheck: () =>
    wait<Health>(
      {
        ok: 2,
        total: 2,
        controlOk: true,
        failed: [],
        checkedAt: new Date().toLocaleTimeString("ru-RU"),
      },
      800
    ),
  exportProfile: () =>
    wait(
      JSON.stringify(
        {
          version: 1,
          engine: state.config.engine,
          selectedStrategy: state.config.selectedStrategy,
          gameFilter: state.config.gameFilter,
          customTargets: state.config.customTargets,
        },
        null,
        2
      )
    ),
  importProfile: (text: string) => {
    const p = JSON.parse(text);
    state.config = { ...state.config, engine: p.engine ?? state.config.engine };
    state.engine = state.config.engine;
    return wait({ snapshot: { ...state }, messages: ["Профиль применён"] }, 600);
  },
  checkAppUpdate: () =>
    wait<UpdateCheck>({
      current: "1.3.0",
      latest: "1.3.0",
      hasUpdate: false,
      release: null,
      error: null,
    }),
  installAppUpdate: () => wait("Устанавливаю — приложение сейчас закроется", 900),
  updateGoodbyeBlacklist: () => {
    state.goodbye = { ...state.goodbye, blacklist: 12501 };
    return wait({ snapshot: { ...state }, messages: ["Список заблокированного обновлён: 12501 доменов"] }, 800);
  },
};

/** Демонстрационные результаты — чтобы видеть экран проверки без запуска winws. */
export const mockResults: StrategyResult[] = [
  result("__baseline__", 33, true),
  result("general (ALT2)", 100),
  result("general (ALT)", 92),
  result("general", 75),
  result("general (FAKE TLS AUTO)", 58),
  result("general (SIMPLE FAKE)", 33),
];
