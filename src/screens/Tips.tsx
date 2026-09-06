import { motion } from "motion/react";
import { Alert, Bulb, Crown, Gauge, Shield } from "../icons";
import { ENGINES } from "../engines";
import type { Engine, Snapshot } from "../types";

/** Задача пользователя и движок, с которого её стоит решать. */
const CASES: { task: string; answer: string; engine: Engine | null; why: string }[] = [
  {
    task: "YouTube грузится по полчаса или показывает ошибку",
    answer: "zapret",
    engine: "zapret",
    why: "Сборка Flowseal собрана как раз под YouTube и Discord: стратегии там уже подобраны, остаётся выбрать ту, что берёт у твоего провайдера",
  },
  {
    task: "В Discord не слышно голос, хотя чат работает",
    answer: "zapret",
    engine: "zapret",
    why: "Голос идёт по UDP, а его правит только драйвер. Ни один локальный прокси — ни ByeDPI, ни Xray, ни sing-box — UDP приложений не видит",
  },
  {
    task: "Игры лагают или не подключаются к серверам",
    answer: "zapret + игровой фильтр",
    engine: "zapret",
    why: "На главной включи игровой фильтр: обход начнёт работать на портах 1024–65535. Сеть нагружается сильнее, зато игровой трафик проходит",
  },
  {
    task: "Нет прав администратора или антивирус ругается на драйвер",
    answer: "ByeDPI",
    engine: "byedpi",
    why: "Это обычная программа без драйвера и без прав администратора. Браузеры и текстовый Discord через неё работают",
  },
  {
    task: "Обходы на пакетах перестали брать — провайдер поумнел",
    answer: "Xray, потом sing-box",
    engine: "xray",
    why: "Фрагментация TLS в ядрах устроена иначе, чем разрывы в zapret и ByeDPI, и часто проходит там, где те уже нет. Сервер для неё не нужен",
  },
  {
    task: "Нужен доступ к сервису, который заблокирован для всей страны",
    answer: "Xray или sing-box со своим сервером",
    engine: "xray",
    why: "Обход блокировки не меняет твою страну — сайт по-прежнему видит российский адрес. Помогает только туннель: вставь ссылку vless:// в настройках ядра",
  },
  {
    task: "Ничего не помогает",
    answer: "Проверка",
    engine: null,
    why: "Вкладка «Проверка» прогоняет все стратегии по очереди и показывает, что реально пробивает Discord и YouTube именно у тебя. Угадывать бессмысленно: у каждого провайдера свой DPI",
  },
];

const RULES: { title: string; text: string }[] = [
  {
    title: "Одновременно работает только один движок",
    text: "Они правят одни и те же пакеты и мешают друг другу. Приложение гасит остальные само — если увидишь предупреждение «работает не тот движок», это остаток от прошлого запуска, его надо остановить.",
  },
  {
    title: "Пока включён VPN или WARP, проверка врёт",
    text: "Весь трафик уходит в туннель, включая наши запросы, — любая стратегия покажет отличный результат. Выключи туннель перед проверкой, иначе меришь его, а не обход.",
  },
  {
    title: "Обход — это не смена страны",
    text: "zapret, ByeDPI и GoodbyeDPI лечат блокировку у провайдера, но адрес остаётся твой. Сервисы, закрытые для России целиком, откроет только туннель — свой сервер в Xray или sing-box.",
  },
  {
    title: "Сначала мягкое, потом жёсткое",
    text: "Начинай с первого пресета в списке: он самый щадящий. «Всё сразу» и мелкая фрагментация ломают часть сайтов и замедляют соединение, поэтому их берут, когда мягкие уже не берут.",
  },
];

export default function Tips({ snap, onEngine }: { snap: Snapshot; onEngine: (e: Engine) => void }) {
  return (
    <div className="screen">
      <div className="head">
        <h1>Советы</h1>
        <p className="sub">
          Что чем обходить и в каком порядке пробовать. Универсального ответа нет: DPI у каждого
          провайдера свой, поэтому последнее слово всегда за вкладкой «Проверка».
        </p>
      </div>

      <div className="stack">
        <motion.div
          className="card"
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          style={{
            borderColor: "rgba(124,107,255,0.35)",
            background: "linear-gradient(90deg, rgba(124,107,255,0.12), rgba(53,214,255,0.03))",
          }}
        >
          <div className="row" style={{ alignItems: "flex-start" }}>
            <span className="rank top" style={{ flex: "none" }}>
              <Bulb />
            </span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontWeight: 600, marginBottom: 4 }}>С чего начать</div>
              <div className="sub" style={{ fontSize: 12.5, lineHeight: 1.6 }}>
                1. Возьми zapret и запусти проверку — она сама найдёт рабочую стратегию.
                <br />
                2. Не помогло или нет прав администратора — переключись на ByeDPI и проверь снова.
                <br />
                3. И там пусто — Xray с фрагментацией, затем sing-box.
                <br />
                4. Ничего не берёт или сервис закрыт для России — свой сервер в Xray.
              </div>
            </div>
          </div>
        </motion.div>

        <div className="card">
          <h2 style={{ marginBottom: 4 }}>Чем они отличаются</h2>
          <p className="sub" style={{ fontSize: 12.5, marginBottom: 10 }}>
            Первые три ломают блокировку на месте, последние два умеют ещё и увести трафик на твой
            сервер. Нажми на карточку, чтобы переключиться.
          </p>
          <div className="list">
            {ENGINES.map((e, i) => (
              <motion.div
                key={e.id}
                className={`item ${snap.engine === e.id ? "selected" : ""}`}
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: i * 0.04 }}
                style={{ flexDirection: "column", alignItems: "stretch", gap: 6 }}
                onClick={() => onEngine(e.id)}
              >
                <div className="row" style={{ gap: 8 }}>
                  <span className="name">{e.label}</span>
                  {snap.engine === e.id && <span className="pill live">выбран</span>}
                  <span className="pill">{e.admin ? "нужен админ" : "без админа"}</span>
                  <span style={{ flex: 1 }} />
                  <span className="sub" style={{ fontSize: 11.5 }}>
                    {e.repo}
                  </span>
                </div>
                <div className="sub" style={{ fontSize: 12.5 }}>
                  {e.short}
                </div>
                <div style={{ fontSize: 12.5, color: "var(--ok)" }}>Когда брать: {e.when}</div>
                <div style={{ fontSize: 12.5, color: "var(--warn)" }}>Чем платишь: {e.cost}</div>
                <div className="sub" style={{ fontSize: 11.5 }}>
                  Под обход попадает: {e.covers}
                </div>
              </motion.div>
            ))}
          </div>
        </div>

        <div className="card">
          <h2 style={{ marginBottom: 10 }}>Что при какой беде</h2>
          {CASES.map((c, i) => (
            <motion.div
              key={c.task}
              className="check"
              initial={{ opacity: 0, x: -6 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: i * 0.04 }}
            >
              <span className="check-icon ok">{c.engine ? "→" : "?"}</span>
              <div style={{ minWidth: 0, flex: 1 }}>
                <div style={{ fontWeight: 550 }}>{c.task}</div>
                <div className="sub" style={{ fontSize: 12.5, marginTop: 2 }}>
                  {c.why}
                </div>
              </div>
              {c.engine ? (
                <button
                  className="btn sm"
                  onClick={() => onEngine(c.engine as Engine)}
                  title={`Переключиться на ${c.answer}`}
                >
                  <Crown /> {c.answer}
                </button>
              ) : (
                <span className="pill" style={{ whiteSpace: "nowrap" }}>
                  <Gauge /> {c.answer}
                </span>
              )}
            </motion.div>
          ))}
        </div>

        <div className="card">
          <h2 style={{ marginBottom: 10 }}>Правила, о которые спотыкаются чаще всего</h2>
          {RULES.map((r, i) => (
            <motion.div
              key={r.title}
              className="check"
              initial={{ opacity: 0, x: -6 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: i * 0.04 }}
            >
              <span className="check-icon warn">
                <Alert />
              </span>
              <div style={{ minWidth: 0, flex: 1 }}>
                <div style={{ fontWeight: 550 }}>{r.title}</div>
                <div className="sub" style={{ fontSize: 12.5, marginTop: 2 }}>
                  {r.text}
                </div>
              </div>
            </motion.div>
          ))}
        </div>

        <div className="card">
          <div className="row" style={{ alignItems: "flex-start" }}>
            <span className="rank" style={{ flex: "none" }}>
              <Shield />
            </span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontWeight: 600, marginBottom: 4 }}>Про свой сервер</div>
              <div className="sub" style={{ fontSize: 12.5, lineHeight: 1.6 }}>
                Xray и sing-box принимают ссылку из панели или бота:{" "}
                <span className="mono">vless://</span>, <span className="mono">vmess://</span>,{" "}
                <span className="mono">trojan://</span> или <span className="mono">ss://</span>.
                Вставь её в настройках ядра и выбери пресет «Через свой сервер» — тогда весь трафик
                пойдёт в туннель. Сервер приложение не выдаёт и не подбирает: он твой, и скорость с
                пингом будут его. Пресеты с фрагментацией сервера не требуют вовсе — их можно
                пробовать сразу.
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
