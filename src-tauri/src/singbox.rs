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
//! Отдельная история — режим TUN. Он не обходит блокировки: обход остаётся
//! тем же самым (фрагментация или свой сервер). TUN меняет другое — что
//! именно попадает в ядро. Без него через прокси идёт только TCP тех
//! приложений, что читают системные настройки Windows; с ним ядро поднимает
//! виртуальный сетевой адаптер и забирает весь трафик машины, включая UDP,
//! игры и программы, которым системный прокси безразличен.
//!
//! Драйвер wintun вшит в сам `sing-box.exe`, ставить отдельно ничего не надо,
//! но нужны права администратора — они у приложения и так есть.
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
    (
        "tun-frag",
        "Весь трафик: фрагментация",
        "То же разрезание TLS, но ядро поднимает сетевой адаптер и забирает трафик всей машины — включая программы, которым системный прокси безразличен. Обходит ровно так же, зато охват шире",
        "tun + route-options: tls_fragment · нужны права администратора",
    ),
    (
        "tun-server",
        "Весь трафик: свой сервер",
        "Полноценный VPN: в туннель уходит всё, включая UDP, игры и голос. Единственный пресет, после которого «через прокси идёт только TCP» перестаёт быть правдой. Нужен свой сервер",
        "tun + outbound по ссылке · нужны права администратора",
    ),
];

/// Пресеты, которые поднимают виртуальный адаптер. Им нельзя прописывать
/// системный прокси: трафик и так весь у них, а прокси поверх туннеля —
/// это ещё один круг по той же дороге.
pub fn is_tun(id: &str) -> bool {
    id.starts_with("tun")
}

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

/// Виртуальный адаптер. Адреса взяты из примеров sing-box; IPv6 добавлен
/// намеренно — без него трафик по IPv6 пошёл бы мимо туннеля, а это ровно та
/// утечка, о которой предупреждает диагностика.
fn tun_inbound() -> Value {
    json!({
        "type": "tun",
        "tag": "tun-in",
        "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
        "mtu": 9000,
        "auto_route": true,
        // strict_route чинит утечку DNS, но ломает VirtualBox и подобное,
        // а главное — оставляет за собой правила, если ядро убить грубо.
        // Пусть лучше остаётся выключенным: цена ошибки тут — вся сеть
        "strict_route": false
    })
}

/// Собирает конфиг под выбранный пресет. Ссылка нужна только серверным
/// пресетам — фрагментация работает сама по себе.
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
        "server" | "tun-server" => {
            let l = server.ok_or(
                "Для этого пресета нужен свой сервер — вставь ссылку vless:// или подписку на вкладке пресетов",
            )?;
            outbounds.push(server_outbound(l)?);
            final_out = "proxy";
        }
        "tun-frag" => rules.push(json!({
            "network": ["tcp"],
            "action": "route-options",
            "tls_fragment": true
        })),
        other => return Err(format!("неизвестный пресет sing-box: {other}")),
    }

    let tun = is_tun(id);
    // Локальный прокси оставляем всегда, даже в режиме TUN: по нему видно,
    // что ядро поднялось, и через него же идёт проверка стратегий
    let mut inbounds = vec![json!({
        "type": "mixed",
        "tag": "in",
        "listen": "127.0.0.1",
        "listen_port": port
    })];

    let mut dns = json!({ "servers": [{ "type": "local", "tag": "local" }] });
    if tun {
        inbounds.push(tun_inbound());
        // Забрав трафик, ядро обязано отвечать и на запросы DNS: они тоже
        // идут в адаптер. Иначе не разрешится ни одно имя
        rules.insert(0, json!({ "action": "sniff" }));
        if final_out == "proxy" {
            let l = server.expect("серверный пресет без ссылки сюда не дойдёт");
            // Имена разрешаем через туннель — иначе провайдер видит, куда мы
            // ходим, даже когда сам трафик спрятан
            dns = json!({
                "servers": [
                    { "type": "udp", "tag": "remote", "server": "1.1.1.1", "detour": "proxy" },
                    { "type": "local", "tag": "local" }
                ],
                "rules": [{ "domain": [l.host.clone()], "action": "route", "server": "local" }],
                "final": "remote",
                // Сервер может быть без IPv6: пусть имена по возможности
                // разрешаются в IPv4, иначе половина сайтов не откроется
                "strategy": "prefer_ipv4"
            });
            // Адрес самого сервера — мимо туннеля, иначе получится петля
            rules.insert(
                1,
                json!({ "domain": [l.host.clone()], "action": "route", "outbound": "direct" }),
            );
        }
    }

    let mut route = json!({ "rules": rules, "final": final_out });
    if tun {
        // Без этого исходящие ядра ушли бы обратно в собственный адаптер
        route["auto_detect_interface"] = json!(true);
        route["default_domain_resolver"] = json!("local");
    }

    Ok(json!({
        "log": { "level": "warn", "timestamp": true },
        "dns": dns,
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": route
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

    /// TUN обязан забрать и IPv6, иначе он утечёт мимо туннеля — ровно то,
    /// на что ругается диагностика.
    #[test]
    fn tun_takes_the_whole_machine_including_ipv6() {
        let cfg = config("tun-frag", 1082, None).unwrap();
        let tun = cfg["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["type"] == "tun")
            .expect("адаптер должен быть");
        let addrs = tun["address"].as_array().unwrap();
        assert!(addrs.iter().any(|a| a.as_str().unwrap().contains(':')), "нет IPv6: {addrs:?}");
        assert_eq!(tun["auto_route"], true);
        // Локальный прокси остаётся: по нему приложение понимает, что ядро живо
        assert!(cfg["inbounds"].as_array().unwrap().iter().any(|i| i["type"] == "mixed"));
        assert_eq!(cfg["route"]["auto_detect_interface"], true);
    }

    /// Соединение с самим сервером не должно уходить в собственный туннель.
    #[test]
    fn tun_server_never_loops_through_itself() {
        let cfg = config("tun-server", 1082, Some(&reality())).unwrap();
        let rules = cfg["route"]["rules"].as_array().unwrap();
        let bypass = rules
            .iter()
            .find(|r| r["outbound"] == "direct" && r["domain"].is_array())
            .expect("адрес сервера должен идти мимо туннеля");
        assert_eq!(bypass["domain"][0], "srv.example.com");
        assert_eq!(cfg["route"]["final"], "proxy");
        // И его имя разрешается локально, иначе не с чего начать
        let dns_rule = &cfg["dns"]["rules"][0];
        assert_eq!(dns_rule["server"], "local");
        assert_eq!(dns_rule["domain"][0], "srv.example.com");
        assert_eq!(cfg["dns"]["final"], "remote");
    }

    #[test]
    fn tun_presets_are_recognised() {
        assert!(is_tun("tun-frag"));
        assert!(is_tun("tun-server"));
        assert!(!is_tun("frag"));
        assert!(!is_tun("server"));
        // Оба серверных пресета без ссылки собраться не должны
        for id in ["server", "tun-server"] {
            assert!(config(id, 1082, None).is_err(), "{id}");
        }
    }

    #[test]
    fn picks_only_the_plain_windows_x64_archive() {
        assert!(is_windows_asset("sing-box-1.14.0-windows-amd64.zip"));
        assert!(!is_windows_asset("sing-box-1.14.0-windows-amd64-legacy-windows-7.zip"));
        assert!(!is_windows_asset("sing-box-1.14.0-windows-386.zip"));
        assert!(!is_windows_asset("sing-box-1.14.0-linux-amd64.tar.gz"));
    }
}
