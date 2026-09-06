import { motion } from "motion/react";
import { Bolt, Download, Folder, Globe } from "../icons";

export default function Onboarding({
  onInstall,
  onPick,
  onByedpi,
  busy,
}: {
  onInstall: () => void;
  onPick: () => void;
  onByedpi: () => void;
  busy: boolean;
}) {
  return (
    <div className="onboard">
      <motion.div
        className="onboard-inner"
        initial={{ opacity: 0, y: 18 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1] }}
      >
        <motion.div
          className="hero-mark"
          initial={{ scale: 0.6, rotate: -12, opacity: 0 }}
          animate={{ scale: 1, rotate: 0, opacity: 1 }}
          transition={{ type: "spring", stiffness: 260, damping: 18, delay: 0.05 }}
        >
          <Bolt />
        </motion.div>

        <h1 style={{ fontSize: 27 }}>Zapret Studio</h1>
        <p className="sub" style={{ marginTop: 8, maxWidth: 480, marginInline: "auto" }}>
          Обновляет стратегии обхода, запускает их в один клик и показывает,
          какая из них действительно работает у твоего провайдера.
        </p>

        <div className="grid-2" style={{ marginTop: 30, textAlign: "left" }}>
          {[
            {
              icon: <Download />,
              title: "Скачать свежий zapret",
              desc: "Возьму последний релиз с GitHub, распакую в свою папку и дальше буду обновлять сам. Ничего искать и качать вручную не придётся.",
              action: onInstall,
              primary: true,
            },
            {
              icon: <Folder />,
              title: "У меня уже есть папка",
              desc: "Подключу твою распакованную сборку — со всеми правками в списках. Обновлять её тоже смогу, настройки при этом сохраню.",
              action: onPick,
              primary: false,
            },
          ].map((c, i) => (
            <motion.button
              key={c.title}
              className="choice"
              disabled={busy}
              onClick={c.action}
              initial={{ opacity: 0, y: 22 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.14 + i * 0.09, type: "spring", stiffness: 200, damping: 22 }}
              whileHover={busy ? {} : { y: -4, borderColor: "rgba(124,107,255,0.5)" }}
              whileTap={busy ? {} : { scale: 0.985 }}
              style={{ opacity: busy ? 0.5 : 1 }}
            >
              <div className="ic">{c.icon}</div>
              <div className="t">{c.title}</div>
              <div className="d">{c.desc}</div>
              <div
                className="sub"
                style={{ marginTop: 6, fontSize: 12, color: c.primary ? "var(--accent-2)" : "var(--dim)" }}
              >
                {c.primary ? "Рекомендую →" : "Выбрать папку →"}
              </div>
            </motion.button>
          ))}
        </div>

        <motion.button
          className="choice"
          disabled={busy}
          onClick={onByedpi}
          initial={{ opacity: 0, y: 22 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.32, type: "spring", stiffness: 200, damping: 22 }}
          whileHover={busy ? {} : { y: -4, borderColor: "rgba(124,107,255,0.5)" }}
          whileTap={busy ? {} : { scale: 0.985 }}
          style={{ marginTop: 14, textAlign: "left", opacity: busy ? 0.5 : 1 }}
        >
          <div className="ic">
            <Globe />
          </div>
          <div className="t">Обойтись без драйверов — ByeDPI</div>
          <div className="d">
            Другой способ: вместо перехвата пакетов поднимается локальный SOCKS5-прокси, а
            системный прокси Windows заворачивает в него браузеры и Discord. Прав администратора
            не нужно, качать целую сборку тоже. Zapret можно подключить позже.
          </div>
          <div className="sub" style={{ marginTop: 6, fontSize: 12, color: "var(--dim)" }}>
            Поставить ByeDPI →
          </div>
        </motion.button>

        <p style={{ marginTop: 22, fontSize: 12, color: "var(--dim)" }}>
          Выбор не окончательный — движок и источник сборки переключаются в настройках в любой момент.
        </p>
      </motion.div>
    </div>
  );
}
