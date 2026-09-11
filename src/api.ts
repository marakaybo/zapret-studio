import { invoke } from "@tauri-apps/api/core";
import { isTauri, mockApi } from "./mock";
import type {
  ActionResult,
  Check,
  ConflictItem,
  Core,
  DnsProvider,
  Engine,
  HostsStatus,
  LogLine,
  NetSample,
  Health,
  ServerPing,
  Snapshot,
  StrategyResult,
  UpdateCheck,
  WarpProbe,
} from "./types";

const real = {
  snapshot: () => invoke<Snapshot>("snapshot"),
  setZapretDir: (path: string) => invoke<Snapshot>("set_zapret_dir", { path }),
  checkUpdate: () => invoke<UpdateCheck>("check_update"),
  installZapret: (fresh: boolean) => invoke<Snapshot>("install_zapret", { fresh }),
  cancelInstall: () => invoke<void>("cancel_install"),
  selectStrategy: (name: string) => invoke<Snapshot>("select_strategy", { name }),

  // --- ByeDPI ---
  setEngine: (engine: Engine) => invoke<ActionResult>("set_engine", { engine }),
  setByedpiDir: (path: string) => invoke<Snapshot>("set_byedpi_dir", { path }),
  checkByedpiUpdate: () => invoke<UpdateCheck>("check_byedpi_update"),
  installByedpi: (fresh: boolean) => invoke<Snapshot>("install_byedpi", { fresh }),
  selectPreset: (id: string) => invoke<Snapshot>("select_preset", { id }),
  savePreset: (id: string | null, name: string, args: string) =>
    invoke<Snapshot>("save_preset", { id, name, args }),
  deletePreset: (id: string) => invoke<Snapshot>("delete_preset", { id }),
  setByedpiPort: (port: number) => invoke<ActionResult>("set_byedpi_port", { port }),
  /** Своя настройка у каждого локального прокси: ByeDPI, Xray, sing-box */
  setSystemProxy: (engine: Engine, enable: boolean) =>
    invoke<ActionResult>("set_system_proxy", { engine, enable }),
  /** Проверка пресетов активного движка — ByeDPI или GoodbyeDPI */
  runPresetTests: (ids: string[], includeBaseline: boolean) =>
    invoke<StrategyResult[]>("run_preset_tests", { ids, includeBaseline }),

  // --- Xray и sing-box: команды общие, движок передаётся параметром ---
  setCoreDir: (engine: Core, path: string) => invoke<Snapshot>("set_core_dir", { engine, path }),
  checkCoreUpdate: (engine: Core) => invoke<UpdateCheck>("check_core_update", { engine }),
  installCore: (engine: Core, fresh: boolean) => invoke<Snapshot>("install_core", { engine, fresh }),
  setCorePort: (engine: Core, port: number) => invoke<ActionResult>("set_core_port", { engine, port }),
  /** Пустая строка — забыть сервер */
  setCoreServer: (engine: Core, url: string) => invoke<ActionResult>("set_core_server", { engine, url }),
  loadSubscription: (engine: Core, url: string) =>
    invoke<ActionResult>("load_subscription", { engine, url }),
  selectServer: (engine: Core, index: number) =>
    invoke<ActionResult>("select_server", { engine, index }),
  pingServers: (engine: Core) => invoke<ServerPing[]>("ping_servers", { engine }),

  // --- GoodbyeDPI ---
  setGoodbyeDir: (path: string) => invoke<Snapshot>("set_goodbye_dir", { path }),
  checkGoodbyeUpdate: () => invoke<UpdateCheck>("check_goodbye_update"),
  installGoodbye: (fresh: boolean) => invoke<Snapshot>("install_goodbye", { fresh }),
  updateGoodbyeBlacklist: () => invoke<ActionResult>("update_goodbye_blacklist"),

  start: (name?: string) => invoke<Snapshot>("start_bypass", { name: name ?? null }),
  stop: () => invoke<Snapshot>("stop_bypass"),
  runTests: (names: string[], includeBaseline: boolean) =>
    invoke<StrategyResult[]>("run_tests", { names, includeBaseline }),
  cancelTests: () => invoke<void>("cancel_tests"),
  setCustomTargets: (items: string[]) => invoke<Snapshot>("set_custom_targets", { items }),
  healthCheck: () => invoke<Health>("health_check"),

  /** Профиль настроек текстом — им делятся с друзьями */
  exportProfile: () => invoke<string>("export_profile"),
  importProfile: (text: string) => invoke<ActionResult>("import_profile", { text }),

  // --- обновление самого приложения ---
  checkAppUpdate: () => invoke<UpdateCheck>("check_app_update"),
  installAppUpdate: () => invoke<string>("install_app_update"),
  installService: (name?: string) => invoke<Snapshot>("install_service", { name: name ?? null }),
  removeService: () => invoke<Snapshot>("remove_service"),
  setAppAutostart: (enable: boolean) => invoke<Snapshot>("set_app_autostart", { enable }),
  setOption: (key: string, value: unknown) => invoke<Snapshot>("set_option", { key, value }),
  ping: () => invoke<NetSample>("ping"),
  measureSpeed: () => invoke<number>("measure_speed"),
  logs: () => invoke<LogLine[]>("get_logs"),
  clearLogs: () => invoke<void>("clear_logs"),
  diagnostics: () => invoke<Check[]>("diagnostics"),
  stopStray: () => invoke<ActionResult>("stop_stray"),
  warpConnect: () => invoke<ActionResult>("warp_connect"),
  warpDisconnect: () => invoke<ActionResult>("warp_disconnect"),
  warpCheck: () => invoke<WarpProbe>("warp_check"),
  dnsProviders: () => invoke<DnsProvider[]>("dns_providers"),
  dnsSet: (provider: string) => invoke<ActionResult>("dns_set", { provider }),
  dnsRestore: () => invoke<ActionResult>("dns_restore"),
  dnsCheck: () => invoke<Check>("dns_check"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  stopConflicts: (items?: ConflictItem[]) =>
    invoke<ActionResult>("stop_conflicts", { items: items ?? null }),
  setGameFilter: (mode: string) => invoke<ActionResult>("set_game_filter", { mode }),
  setIpsetMode: (mode: string) => invoke<ActionResult>("set_ipset_mode", { mode }),
  updateIpset: () => invoke<ActionResult>("update_ipset"),
  setActiveFake: (kind: "discord" | "game", source: string) =>
    invoke<ActionResult>("set_active_fake", { kind, source }),
  clearDiscordCache: () => invoke<ActionResult>("clear_discord_cache"),
  hostsStatus: () => invoke<HostsStatus>("hosts_status"),
  applyHosts: () => invoke<string>("apply_hosts"),
  openFolder: () => invoke<void>("open_folder"),
  quit: () => invoke<void>("quit_app"),
};

export const api: typeof real = isTauri ? real : (mockApi as unknown as typeof real);

export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as Error).message);
  return String(e);
}
