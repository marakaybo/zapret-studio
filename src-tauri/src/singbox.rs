//! sing-box — пятый движок (репозиторий SagerNet/sing-box).
//!
//! Ядро того же рода, что Xray, но с другим устройством конфига и другой
//! фрагментацией. Здесь она встроена в маршрутизацию: правило со свойством
//! `tls_fragment` заставляет ядро разрезать TLS-приветствие на любом
//! подходящем соединении, а `tls_record_fragment` — разложить его на
//! несколько TLS-записей. Одновременно их включать нельзя, сам sing-box
//! считает это ошибкой, поэтому они разведены по разным пресетам.
//!
//! Как и Xray, умеет увести трафик на свой сервер по ссылке `vless://` и
//! подобным. Наружу выглядит одинаково: локальный прокси на 127.0.0.1,
//! в который трафик попадает через системные настройки Windows.
//!
//! Конфиг пишем в формате sing-box 1.12 и новее (правила с `action`, DNS с
//! `type`) — приложение ставит последнюю версию само.

use crate::proxycore::{self as core, Spec};
use crate::link::ServerLink;
use crate::preset::Preset;
use serde_json::{json, Value};

pub const EXE: &str = "sing-box.exe";

pub const SPEC: Spec = Spec {
    id: "singbox",
    name: "sing-box",
    exe: EXE,
    repo: "SagerNet/sing-box",
    config_name: "zapret-studio.json",
    // -D задаёт рабочую папку: туда ядро складывает свой кэш
    args: |config, root| {
        vec![
            "run".into(),
            "-c".into(),
            config.display().to_string(),
            "-D".into(),
            root.display().to_string(),
        ]
    },
    asset: is_windows_asset,
};

/// В релизе десятки сборок; нужна обычная 64-битная под Windows —
/// «legacy» собран для Windows 7 и нам не подходит.
pub fn is_windows_asset(name: &str) -> bool {
    let n = name.to_lowercase();
    n.ends_with(".zip") && n.contains("windows-amd64") && !n.contains("legacy")
}

// ------------------------------------------------------------------ пресеты

const BUILTIN: &[core::Builtin] = &[
    (
        "frag",
        "Фрагментация TLS",
        "Приветствие TLS уходит частями — DPI не собирает из них имя сайта и пропускает соединение. Начинать стоит с неё",
        "route-options: tls_fragment",
    ),
    (
        "frag-slow",
        "Фрагментация с паузой",
        "То же самое, но между частями ядро выжидает полсекунды. Медленнее открывается, зато проходит там, где быстрая фрагментация склеивается обратно",
        "route-options: tls_fragment + пауза 500 мс",
    ),
    (
        "record-frag",
        "Разрыв TLS-записи",
        "Приветствие раскладывается на несколько TLS-записей вместо одной. Другой приём, чем фрагментация, — берёт там, где та не сработала",
        "route-options: tls_record_fragment",
    ),
    (
        "server",
        "Через свой сервер",
        "Весь трафик уходит на сервер из твоей ссылки. Это уже не обход, а туннель: работает всё, но скорость и пинг — какие у сервера",
        "outbound по ссылке · нужен свой сервер",
    ),
];

pub fn presets() -> Vec<Preset> {
    core::presets(BUILTIN)
}

// ------------------------------------------------------------------- конфиг

/// Транспорт из ссылки в том виде, в каком его ждёт sing-box.
fn transport(l: &ServerLink) -> Option<Value> {
    let path = if l.path.is_empty() { "/" } else { l.path.as_str() };
    let host = if l.host_header.is_empty() {
        if l.sni.is_empty() { l.host.as_str() } else { l.sni.as_str() }
    } else {
        l.host_header.as_str()
    };
    match l.network.as_str() {
        "ws" => Some(json!({ "type": "ws", "path": path, "headers": { "Host": host } })),
        "grpc" => Some(json!({ "type": "grpc", "service_name": l.service_name })),
        "httpupgrade" => Some(json!({ "type": "httpupgrade", "path": path, "host": host })),
        "h2" | "http" => Some(json!({ "type": "http", "path": path, "host": [host] })),
        _ => None,
    }
}

fn tls(l: &ServerLink) -> Option<Value> {
    if l.security != "tls" && l.security != "reality" {
        return None;
    }
    let sni = if l.sni.is_empty() { l.host.as_str() } else { l.sni.as_str() };
    let fingerprint = if l.fingerprint.is_empty() { "chrome" } else { l.fingerprint.as_str() };
    let mut t = json!({ "enabled": true, "server_name": sni, "insecure": l.insecure });
    if !l.alpn.is_empty() {
        t["alpn"] = json!(l.alpn);
    }
    if l.security == "reality" {
        // Reality в sing-box работает только поверх uTLS — без него ядро
        // откажется запускаться
        t["utls"] = json!({ "enabled": true, "fingerprint": fingerprint });
        t["reality"] =
            json!({ "enabled": true, "public_key": l.public_key, "short_id": l.short_id });
    } else if !l.fingerprint.is_empty() {
        t["utls"] = json!({ "enabled": true, "fingerprint": fingerprint });
    }
    Some(t)
}

fn server_outbound(l: &ServerLink) -> Result<Value, String> {
    let mut out = match l.proto.as_str() {
        "vless" => {
            let mut o = json!({
                "type": "vless",
                "tag": "proxy",
                "server": l.host,
                "server_port": l.port,
                "uuid": l.secret,
                "packet_encoding": "xudp"
            });
            if !l.flow.is_empty() {
                o["flow"] = json!(l.flow);
            }
            o
        }
        "vmess" => json!({
            "type": "vmess",
            "tag": "proxy",
            "server": l.host,
            "server_port": l.port,
            "uuid": l.secret,
            "security": "auto",
            "alter_id": 0
        }),
        "trojan" => json!({
            "type": "trojan",
            "tag": "proxy",
            "server": l.host,
            "server_port": l.port,
            "password": l.secret
        }),
        "shadowsocks" => json!({
            "type": "shadowsocks",
            "tag": "proxy",
            "server": l.host,
            "server_port": l.port,
            "method": l.method,
            "password": l.secret
        }),
        other => return Err(format!("sing-box не умеет протокол «{other}»")),
    };
    if let Some(t) = tls(l) {
        out["tls"] = t;
    }
    if let Some(t) = transport(l) {
        out["transport"] = t;
    }
    Ok(out)
}

/// Собирает конфиг под выбранный пресет. Ссылка нужна только серверному
/// пресету — фрагментация работает сама по себе.
pub fn config(id: &str, port: u16, server: Option<&ServerLink>) -> Result<Value, String> {
    let mut outbounds = vec![json!({ "type": "direct", "tag": "direct" })];
    // Локальная сеть мимо туннеля: роутеру и принтерам в нём делать нечего
    let mut rules = vec![json!({ "ip_is_private": true, "action": "route", "outbound": "direct" })];
    let mut final_out = "direct";

    match id {
        "frag" => rules.push(json!({
            "network": ["tcp"],
            "action": "route-options",
            "tls_fragment": true
        })),
        "frag-slow" => rules.push(json!({
            "network": ["tcp"],
            "action": "route-options",
            "tls_fragment": true,
            "tls_fragment_fallback_delay": "500ms"
        })),
        "record-frag" => rules.push(json!({
            "network": ["tcp"],
            "action": "route-options",
            "tls_record_fragment": true
        })),
        "server" => {
            let l = server.ok_or(
                "Для этого пресета нужен свой сервер — вставь ссылку vless:// в настройках sing-box",
            )?;
            outbounds.push(server_outbound(l)?);
            final_out = "proxy";
        }
        other => return Err(format!("неизвестный пресет sing-box: {other}")),
    }

    Ok(json!({
        "log": { "level": "warn", "timestamp": true },
        // Один резолвер системный: так ядро не спорит с настройками Windows
        "dns": { "servers": [{ "type": "local", "tag": "local" }] },
        "inbounds": [{
            "type": "mixed",
            "tag": "in",
            "listen": "127.0.0.1",
            "listen_port": port
        }],
        "outbounds": outbounds,
        "route": { "rules": rules, "final": final_out }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reality() -> ServerLink {
        crate::link::parse(
            "vless://uuid-1@srv.example.com:443?security=reality&pbk=KEY&sid=ab&sni=www.microsoft.com&flow=xtls-rprx-vision",
        )
        .unwrap()
    }

    #[test]
    fn local_presets_need_no_server() {
        for p in presets().iter().filter(|p| !core::needs_server(&p.id)) {
            let cfg = config(&p.id, 1082, None).unwrap_or_else(|e| panic!("{}: {e}", p.id));
            assert_eq!(cfg["inbounds"][0]["listen_port"], 1082);
            assert_eq!(cfg["route"]["final"], "direct");
            let rules = cfg["route"]["rules"].as_array().unwrap();
            // Правило обхода должно быть, и с условием: sing-box отказывается
            // принимать правило без единого условия
            let options = rules.iter().find(|r| r["action"] == "route-options").expect(&p.id);
            assert!(options["network"].is_array(), "{}: правило без условия", p.id);
        }
    }

    /// sing-box считает ошибкой сочетание этих двух приёмов — они не должны
    /// оказаться в одном пресете.
    #[test]
    fn never_mixes_fragment_with_record_fragment() {
        for p in presets() {
            let Ok(cfg) = config(&p.id, 1082, Some(&reality())) else { continue };
            for rule in cfg["route"]["rules"].as_array().unwrap() {
                assert!(
                    !(rule["tls_fragment"] == true && rule["tls_record_fragment"] == true),
                    "{}: {rule}",
                    p.id
                );
            }
        }
    }

    #[test]
    fn server_preset_refuses_to_start_without_a_link() {
        let err = config("server", 1082, None).expect_err("сервера нет");
        assert!(err.contains("сервер"), "{err}");
    }

    #[test]
    fn builds_reality_outbound_from_link() {
        let cfg = config("server", 1082, Some(&reality())).unwrap();
        assert_eq!(cfg["route"]["final"], "proxy");
        let out = cfg["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["tag"] == "proxy")
            .unwrap()
            .clone();
        assert_eq!(out["type"], "vless");
        assert_eq!(out["server"], "srv.example.com");
        assert_eq!(out["uuid"], "uuid-1");
        assert_eq!(out["flow"], "xtls-rprx-vision");
        assert_eq!(out["tls"]["reality"]["public_key"], "KEY");
        assert_eq!(out["tls"]["server_name"], "www.microsoft.com");
        // Reality без uTLS ядро не примет
        assert_eq!(out["tls"]["utls"]["enabled"], true);
    }

    #[test]
    fn maps_websocket_transport() {
        let l =
            crate::link::parse("trojan://pass@1.2.3.4:8443?type=ws&path=%2Fws&host=cdn.example.com")
                .unwrap();
        let cfg = config("server", 1082, Some(&l)).unwrap();
        let out = cfg["outbounds"].as_array().unwrap().iter().find(|o| o["tag"] == "proxy").unwrap().clone();
        assert_eq!(out["transport"]["type"], "ws");
        assert_eq!(out["transport"]["path"], "/ws");
        assert_eq!(out["transport"]["headers"]["Host"], "cdn.example.com");
        assert_eq!(out["password"], "pass");
        assert_eq!(out["tls"]["enabled"], true);
    }

    #[test]
    fn picks_only_the_plain_windows_x64_archive() {
        assert!(is_windows_asset("sing-box-1.14.0-windows-amd64.zip"));
        assert!(!is_windows_asset("sing-box-1.14.0-windows-amd64-legacy-windows-7.zip"));
        assert!(!is_windows_asset("sing-box-1.14.0-windows-386.zip"));
        assert!(!is_windows_asset("sing-box-1.14.0-linux-amd64.tar.gz"));
    }
}
