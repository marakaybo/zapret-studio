export type Engine = "zapret" | "byedpi" | "goodbyedpi" | "xray" | "singbox";

/** Прокси-ядра: у Xray и sing-box один и тот же набор настроек */
export type Core = "xray" | "singbox";

export interface AppConfig {
  zapretDir: string | null;
  managed: boolean;
  installedVersion: string | null;
  selectedStrategy: string | null;
  appAutostart: boolean;
  startMinimized: boolean;
  autoCheckUpdates: boolean;
  autostartBypass: boolean;
  gameFilter: "off" | "all" | "tcp" | "udp";
  onboarded: boolean;
  bestStrategy: string | null;
  lastTestAt: string | null;
  autoInstallUpdates: boolean;
  updateIntervalHours: number;
  lastUpdateCheck: string | null;
  engine: Engine;
  byedpiDir: string | null;
  byedpiManaged: boolean;
  byedpiVersion: string | null;
  byedpiPreset: string | null;
  byedpiPort: number;
  byedpiSystemProxy: boolean;
  byedpiAutostart: boolean;
  byedpiCustom: CustomPreset[];
  byedpiBest: string | null;
  byedpiLastTestAt: string | null;
  savedProxy: { enabled: boolean; server: string; bypass: string } | null;
  goodbyeDir: string | null;
  goodbyeManaged: boolean;
  goodbyeVersion: string | null;
  goodbyePreset: string | null;
  goodbyeAutostart: boolean;
  goodbyeCustom: CustomPreset[];
  goodbyeBest: string | null;
  goodbyeLastTestAt: string | null;
  xray: CoreSettings;
  singbox: CoreSettings;
  /** свои сайты для проверки, как их ввёл пользователь */
  customTargets: string[];
  /** откуда приложение берёт обновления себе, `владелец/репозиторий` */
  appRepo: string | null;
  watchdog: boolean;
  watchdogIntervalMin: number;
  watchdogAutoFix: boolean;
}

/** Короткий осмотр сторожа: пробивает ли обход прямо сейчас */
export interface Health {
  ok: number;
  total: number;
  /** контрольная точка ответила — значит, сеть на месте */
  controlOk: boolean;
  failed: string[];
  checkedAt: string;
}

/** Настройки прокси-ядра в конфиге приложения */
export interface CoreSettings {
  dir: string | null;
  managed: boolean;
  version: string | null;
  preset: string | null;
  port: number;
  systemProxy: boolean;
  autostart: boolean;
  /** ссылка на свой сервер целиком — внутри лежит uuid или пароль */
  server: string | null;
  best: string | null;
  lastTestAt: string | null;
}

/** Свой набор параметров ByeDPI — хранится строкой, как его ввёл пользователь */
export interface CustomPreset {
  id: string;
  name: string;
  args: string;
}

export interface Preset {
  id: string;
  name: string;
  desc: string;
  args: string[];
  builtin: boolean;
}

/** Общее у движков, которые настраиваются пресетами: ByeDPI и GoodbyeDPI */
export interface PresetEngine {
  installed: boolean;
  dir: string | null;
  managed: boolean;
  managedDir: string;
  version: string | null;
  running: boolean;
  /** id работающего пресета */
  current: string | null;
  autostart: boolean;
  presets: Preset[];
  selected: string | null;
  best: string | null;
  lastTestAt: string | null;
}

export interface ByeState extends PresetEngine {
  port: number;
  /** прописывать системный прокси автоматически */
  systemProxy: boolean;
  /** системный прокси прямо сейчас направлен в ByeDPI */
  systemProxyActive: boolean;
}

/** Cloudflare WARP: не обход, но уводит в туннель весь трафик, включая проверки */
export interface WarpState {
  installed: boolean;
  connected: boolean;
  detail: string;
  mode: string | null;
  /** куда отправить за установкой, если программы нет */
  installUrl: string;
}

/** Что Cloudflare отвечает о нас самих — по этому видно, работает ли туннель
 *  на самом деле, а не только по мнению службы WARP */
export interface WarpProbe {
  active: boolean;
  /** on | off | plus */
  value: string;
  ip: string;
  loc: string;
  colo: string;
  checkedAt: string;
  error: string | null;
}

/** Xray и sing-box: локальный прокси, как ByeDPI, плюс свой сервер */
export interface CoreState extends PresetEngine {
  port: number;
  systemProxy: boolean;
  systemProxyActive: boolean;
  /** короткая сводка по серверу, без uuid и паролей */
  server: string | null;
  /** ссылка сохранена, но не разбирается */
  serverError: string | null;
}

export interface GoodbyeState extends PresetEngine {
  /** сколько доменов в russia-blacklist.txt, null — файла нет */
  blacklist: number | null;
}

export interface ConflictItem {
  kind: "service" | "process";
  name: string;
}

export interface FakesInfo {
  available: string[];
  discord: string | null;
  game: string | null;
}

export interface HostsStatus {
  total: number;
  missing: number;
  upToDate: boolean;
}

export type IpsetMode = "loaded" | "none" | "any" | "missing";

export interface Strategy {
  file: string;
  name: string;
  label: string;
  args: string[];
  tags: string[];
}

export interface ServiceState {
  installed: boolean;
  running: boolean;
  strategy: string | null;
}

export interface Snapshot {
  config: AppConfig;
  dirOk: boolean;
  strategies: Strategy[];
  running: boolean;
  current: string | null;
  service: ServiceState;
  autostart: boolean;
  testing: boolean;
  appVersion: string;
  managedDir: string;
  ipsetMode: IpsetMode;
  fakes: FakesInfo | null;
  conflicts: ConflictItem[];
  engine: Engine;
  byedpi: ByeState;
  goodbye: GoodbyeState;
  xray: CoreState;
  singbox: CoreState;
  warp: WarpState;
  /** свои сайты, которые уйдут в проверку: настройки плюс targets.txt */
  checkTargets: string[];
  /** откуда приложение берёт обновления себе */
  appRepo: string;
  /** движок, который работает вопреки выбору — остаток от прошлого запуска */
  stray: string | null;
  error: string | null;
}

export interface ActionResult {
  snapshot: Snapshot;
  messages: string[];
}

export interface ReleaseInfo {
  version: string;
  publishedAt: string;
  notes: string;
  zipUrl: string;
  size: number;
}

export interface UpdateCheck {
  current: string | null;
  latest: string | null;
  hasUpdate: boolean;
  release: ReleaseInfo | null;
  error: string | null;
}

export interface Progress {
  phase: "download" | "extract" | "apply" | "done";
  percent: number;
  detail: string;
}

export interface NetSample {
  /** задержка в миллисекундах, null — не достучались */
  ms: number | null;
  /** через что мерили: напрямую или через прокси ByeDPI */
  via: string;
}

export interface LogLine {
  time: string;
  text: string;
  kind: string;
}

export interface TargetResult {
  id: string;
  group: string;
  label: string;
  ok: boolean;
  status: number | null;
  ms: number | null;
  error: string | null;
}

export interface GroupScore {
  group: string;
  ok: number;
  total: number;
}

export interface StrategyResult {
  strategy: string;
  label: string;
  baseline: boolean;
  started: boolean;
  error: string | null;
  ok: number;
  total: number;
  score: number;
  avgMs: number | null;
  speedKbs: number | null;
  groups: GroupScore[];
  targets: TargetResult[];
}

export interface TestStage {
  index: number;
  total: number;
  strategy: string;
  label: string;
  phase: "start" | "probe";
}

export interface Check {
  title: string;
  level: "ok" | "warn" | "fail";
  detail: string;
  hint: string | null;
  /** Что можно остановить кнопкой — заполнено только у проверки конфликтов */
  items: ConflictItem[];
}
