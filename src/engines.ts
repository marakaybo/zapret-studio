/**
 * Справочник движков — один на всё приложение.
 *
 * Раньше подписи и описания были рассыпаны по экранам, и после каждого
 * нового движка их приходилось дописывать в трёх местах. Теперь и
 * переключатель, и настройки, и вкладка «Советы» берут их отсюда.
 */
import type { Engine, Snapshot } from "./types";

export interface EngineInfo {
  id: Engine;
  label: string;
  /** Одной строкой: что это такое */
  short: string;
  /** Когда именно его стоит выбрать */
  when: string;
  /** Чем платим за этот способ */
  cost: string;
  /** Нужны ли права администратора */
  admin: boolean;
  /** Что попадает под обход */
  covers: string;
  repo: string;
}

export const ENGINES: EngineInfo[] = [
  {
    id: "zapret",
    label: "zapret",
    short: "Правит пакеты драйвером WinDivert — видит весь трафик, включая UDP и игры",
    when: "Основной выбор. Единственный, кто чинит голос в Discord и игровой трафик, потому что работает с UDP",
    cost: "Нужны права администратора и установка драйвера; с GoodbyeDPI одновременно не работает",
    admin: true,
    covers: "весь трафик компьютера, TCP и UDP",
    repo: "Flowseal/zapret-discord-youtube",
  },
  {
    id: "byedpi",
    label: "ByeDPI",
    short: "Локальный SOCKS5: ломает запросы по дороге к серверу, без драйверов",
    when: "Когда нет прав администратора или драйвер конфликтует с антивирусом. Хорош для браузера и текста в Discord",
    cost: "Голос Discord и игры так не обойти: через прокси идёт только TCP тех приложений, что читают системные настройки",
    admin: false,
    covers: "TCP приложений, читающих системный прокси",
    repo: "hufrea/byedpi",
  },
  {
    id: "goodbyedpi",
    label: "GoodbyeDPI",
    short: "Тот же драйвер, что у zapret, но настраивается ключами и списками доменов",
    when: "Если zapret не берёт. Умеет работать только по списку заблокированных доменов — остальной трафик не трогает",
    cost: "Права администратора и тот же WinDivert: с zapret одновременно не запустится",
    admin: true,
    covers: "весь трафик компьютера, в основном TCP",
    repo: "ValdikSS/GoodbyeDPI",
  },
  {
    id: "xray",
    label: "Xray",
    short: "Прокси-ядро: режет TLS-приветствие на куски, а с твоим сервером работает как VPN",
    when: "Когда обходы на пакетах перестали брать. Фрагментация работает сразу, без сервера; со ссылкой vless:// уводит трафик в туннель",
    cost: "Как и ByeDPI — только через системный прокси. Туннель добавляет пинг: скорость будет как у сервера",
    admin: false,
    covers: "TCP приложений, читающих системный прокси",
    repo: "XTLS/Xray-core",
  },
  {
    id: "singbox",
    label: "sing-box",
    short: "Второе прокси-ядро с другой фрагментацией и своим набором приёмов",
    when: "Запасной вариант к Xray: фрагментация здесь устроена иначе и иногда берёт там, где Xray уже нет",
    cost: "То же, что у Xray: только системный прокси, а туннель — по скорости сервера",
    admin: false,
    covers: "TCP приложений, читающих системный прокси",
    repo: "SagerNet/sing-box",
  },
];

export const engineInfo = (id: Engine): EngineInfo =>
  ENGINES.find((e) => e.id === id) ?? ENGINES[0];

/** Прокси-ядра настраиваются одинаково, поэтому экраны спрашивают именно так */
export const isCore = (id: Engine): id is "xray" | "singbox" =>
  id === "xray" || id === "singbox";

/** Установленная версия выбранного движка — для подписи в боковом меню */
export function engineVersion(snap: Snapshot): string {
  const v =
    snap.engine === "byedpi"
      ? snap.byedpi.version
      : snap.engine === "goodbyedpi"
        ? snap.goodbye.version
        : snap.engine === "xray"
          ? snap.xray.version
          : snap.engine === "singbox"
            ? snap.singbox.version
            : snap.config.installedVersion;
  return `${engineInfo(snap.engine).label} ${v ?? "—"}`;
}
