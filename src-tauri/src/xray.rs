//! Xray — четвёртый движок (репозиторий XTLS/Xray-core).
//!
//! Он умеет две разные вещи, и приложение даёт обе.
//!
//! Первая — фрагментация: Xray поднимает локальный SOCKS5 и режет исходящее
//! TLS-приветствие на куски, между которыми делает паузу. DPI видит обрывки,
//! в которых не собирается имя сайта, и пропускает соединение. Сервер для
//! этого не нужен — работает у всех сразу, как ByeDPI.
//!
//! Вторая — туннель: если у пользователя есть свой сервер (строка
//! `vless://…`, `vmess://…`, `trojan://…`, `ss://…`), тот же Xray уводит
//! трафик туда целиком. Тогда это уже не обход блокировки, а VPN — и
//! проверка стратегий мерит качество сервера, а не обхода.
//!
//! Отдельным пресетом сделано и то и другое сразу: фрагментация применяется
//! к соединению с самим сервером. Помогает, когда провайдер научился рвать
//! именно подключение к зарубежным серверам.

use crate::proxycore::{self as core, Spec};
use crate::link::ServerLink;
use crate::preset::Preset;
use serde_json::{json, Value};

pub const EXE: &str = "xray.exe";

pub const SPEC: Spec = Spec {
    id: "xray",
    name: "Xray",
    exe: EXE,
    repo: "XTLS/Xray-core",
    config_name: "zapret-studio.json",
    args: |config, _root| vec!["run".into(), "-c".into(), config.display().to_string()],
    asset: is_windows_asset,
};

/// В релизе лежат сборки под все системы — нам нужна 64-битная под Windows.
pub fn is_windows_asset(name: &str) -> bool {
    let n = name.to_lowercase();
    n == "xray-windows-64.zip"
}

// ------------------------------------------------------------------ пресеты

/// Значения фрагментации взяты из рабочих конфигов сообщества: `tlshello`
/// режет именно приветствие TLS (там и лежит имя сайта), а «1-3» — первые
/// записи потока, если DPI смотрит шире.
const BUILTIN: &[core::Builtin] = &[
    (
        "frag-tlshello",
        "Фрагментация TLS-приветствия",
        "Приветствие TLS уходит кусками по 100–200 байт с паузой в 10–20 мс — DPI не собирает из них имя сайта. Начинать стоит с неё",
        "fragment: tlshello · куски 100–200 байт · пауза 10–20 мс",
    ),
    (
        "frag-small",
        "Мелкие куски",
        "То же самое, но куски по 10–20 байт: медленнее открывается, зато проходит там, где крупные куски DPI ещё склеивает",
        "fragment: tlshello · куски 10–20 байт · пауза 10–20 мс",
    ),
    (
        "frag-packets",
        "Резать первые пакеты",
        "Режем не приветствие, а первые три записи потока целиком. Помогает, когда DPI смотрит не только на TLS",
        "fragment: пакеты 1–3 · куски 100–200 байт · пауза 10–20 мс",
    ),
    (
        "frag-noise",
        "Фрагментация и шум",
        "К фрагментации добавляется случайный мусор перед UDP-пакетами — против DPI, который опознаёт QUIC по первым байтам",
        "fragment + noises: случайные 10–20 байт перед UDP",
    ),
    (
        "server",
        "Через свой сервер",
        "Весь трафик уходит на сервер из твоей ссылки. Это уже не обход, а туннель: работает всё, но скорость и пинг — какие у сервера",
        "outbound по ссылке · нужен свой сервер",
    ),
    (
        "server-frag",
        "Свой сервер и фрагментация",
        "Тот же туннель, но приветствие TLS к самому серверу тоже режется на куски. Для случаев, когда провайдер рвёт именно подключение к зарубежным адресам",
        "outbound по ссылке через фрагментирующий dialerProxy",
    ),
];

pub fn presets() -> Vec<Preset> {
    core::presets(BUILTIN)
}

// ------------------------------------------------------------------- конфиг

fn inbound(port: u16) -> Value {
    json!({
        "tag": "socks-in",
        "listen": "127.0.0.1",
        "port": port,
        "protocol": "socks",
        "settings": { "auth": "noauth", "udp": true },
        // Без sniffing фрагментация была бы бесполезной: Xray должен видеть,
        // что внутри соединения именно TLS, и где у него приветствие
        "sniffing": { "enabled": true, "destOverride": ["http", "tls", "quic"] }
    })
}

fn fragment(packets: &str, length: &str, interval: &str, noise: bool) -> Value {
    let mut settings = json!({
        "domainStrategy": "AsIs",
        "fragment": { "packets": packets, "length": length, "interval": interval }
    });
    if noise {
        settings["noises"] =
            json!([{ "type": "rand", "packet": "10-20", "delay": "10-16" }]);
    }
    json!({ "tag": "fragment", "protocol": "freedom", "settings": settings })
}

fn direct() -> Value {
    json!({ "tag": "direct", "protocol": "freedom", "settings": { "domainStrategy": "AsIs" } })
}

/// Транспорт и шифрование из ссылки в том виде, в каком их ждёт Xray.
fn stream(l: &ServerLink, dialer: Option<&str>) -> Value {
    // «h2» в ссылках и «http» в конфиге — одно и то же
    let network = match l.network.as_str() {
        "h2" | "http" => "http",
        "" => "tcp",
        other => other,
    };
    let mut s = json!({ "network": network, "security": l.security });

    let sni = if l.sni.is_empty() { l.host.as_str() } else { l.sni.as_str() };
    let fingerprint = if l.fingerprint.is_empty() { "chrome" } else { l.fingerprint.as_str() };
    let host = if l.host_header.is_empty() { sni } else { l.host_header.as_str() };
    let path = if l.path.is_empty() { "/" } else { l.path.as_str() };

    match l.security.as_str() {
        "reality" => {
            s["realitySettings"] = json!({
                "serverName": sni,
                "fingerprint": fingerprint,
                "publicKey": l.public_key,
                "shortId": l.short_id,
                "spiderX": "/"
            });
        }
        "tls" => {
            let mut tls = json!({ "serverName": sni, "allowInsecure": l.insecure });
            if !l.fingerprint.is_empty() {
                tls["fingerprint"] = json!(l.fingerprint);
            }
            if !l.alpn.is_empty() {
                tls["alpn"] = json!(l.alpn);
            }
            s["tlsSettings"] = tls;
        }
        _ => {}
    }

    match network {
        "ws" => s["wsSettings"] = json!({ "path": path, "headers": { "Host": host } }),
        "httpupgrade" => s["httpupgradeSettings"] = json!({ "path": path, "host": host }),
        "grpc" => s["grpcSettings"] = json!({ "serviceName": l.service_name }),
        "http" => s["httpSettings"] = json!({ "path": path, "host": [host] }),
        _ => {}
    }

    // Через dialerProxy соединение с сервером само идёт через другой
    // outbound — так фрагментация достаётся и подключению к серверу
    if let Some(tag) = dialer {
        s["sockopt"] = json!({ "dialerProxy": tag });
    }
    s
}

fn server_outbound(l: &ServerLink, dialer: Option<&str>) -> Result<Value, String> {
    let settings = match l.proto.as_str() {
        "vless" => {
            let mut user = json!({ "id": l.secret, "encryption": "none" });
            if !l.flow.is_empty() {
                user["flow"] = json!(l.flow);
            }
            json!({ "vnext": [{ "address": l.host, "port": l.port, "users": [user] }] })
        }
        "vmess" => json!({
            "vnext": [{
                "address": l.host,
                "port": l.port,
                "users": [{ "id": l.secret, "alterId": 0, "security": "auto" }]
            }]
        }),
        "trojan" => {
            json!({ "servers": [{ "address": l.host, "port": l.port, "password": l.secret }] })
        }
        "shadowsocks" => json!({
            "servers": [{
                "address": l.host,
                "port": l.port,
                "method": l.method,
                "password": l.secret
            }]
        }),
        other => return Err(format!("Xray не умеет протокол «{other}»")),
    };

    Ok(json!({
        "tag": "proxy",
        "protocol": l.proto,
        "settings": settings,
        "streamSettings": stream(l, dialer)
    }))
}

/// Собирает конфиг под выбранный пресет. Ссылка нужна только серверным
/// пресетам — остальные работают сами по себе.
pub fn config(id: &str, port: u16, server: Option<&ServerLink>) -> Result<Value, String> {
    let mut root = json!({
        "log": { "loglevel": "warning" },
        "inbounds": [inbound(port)],
        "outbounds": []
    });

    let outbounds: Vec<Value> = match id {
        "frag-tlshello" => vec![fragment("tlshello", "100-200", "10-20", false)],
        "frag-small" => vec![fragment("tlshello", "10-20", "10-20", false)],
        "frag-packets" => vec![fragment("1-3", "100-200", "10-20", false)],
        "frag-noise" => vec![fragment("tlshello", "100-200", "10-20", true)],
        "server" | "server-frag" => {
            let l = server.ok_or(
                "Для этого пресета нужен свой сервер — вставь ссылку vless:// в настройках Xray",
            )?;
            let dialer = (id == "server-frag").then_some("fragment");
            let mut list = vec![server_outbound(l, dialer)?];
            if dialer.is_some() {
                list.push(fragment("tlshello", "100-200", "10-20", false));
            }
            list.push(direct());
            list
        }
        other => return Err(format!("неизвестный пресет Xray: {other}")),
    };

    root["outbounds"] = Value::Array(outbounds);
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reality() -> ServerLink {
        crate::link::parse(
            "vless://uuid-1@srv.example.com:443?security=reality&pbk=KEY&sid=ab&fp=chrome&sni=www.microsoft.com&flow=xtls-rprx-vision#RU",
        )
        .unwrap()
    }

    #[test]
    fn local_presets_need_no_server() {
        for p in presets().iter().filter(|p| !core::needs_server(&p.id)) {
            let cfg = config(&p.id, 1081, None).unwrap_or_else(|e| panic!("{}: {e}", p.id));
            assert_eq!(cfg["inbounds"][0]["port"], 1081);
            assert_eq!(cfg["outbounds"][0]["protocol"], "freedom");
            assert!(
                cfg["outbounds"][0]["settings"]["fragment"].is_object(),
                "{}: пресет без фрагментации бесполезен",
                p.id
            );
        }
    }

    #[test]
    fn server_presets_refuse_to_start_without_a_link() {
        for p in presets().iter().filter(|p| core::needs_server(&p.id)) {
            let err = config(&p.id, 1081, None).expect_err(&p.id);
            assert!(err.contains("сервер"), "{}: {err}", p.id);
        }
    }

    #[test]
    fn builds_reality_outbound_from_link() {
        let cfg = config("server", 1081, Some(&reality())).unwrap();
        let out = &cfg["outbounds"][0];
        assert_eq!(out["protocol"], "vless");
        assert_eq!(out["settings"]["vnext"][0]["address"], "srv.example.com");
        assert_eq!(out["settings"]["vnext"][0]["users"][0]["flow"], "xtls-rprx-vision");
        assert_eq!(out["streamSettings"]["security"], "reality");
        assert_eq!(out["streamSettings"]["realitySettings"]["publicKey"], "KEY");
        assert_eq!(out["streamSettings"]["realitySettings"]["serverName"], "www.microsoft.com");
        // без dialerProxy соединение с сервером идёт напрямую
        assert!(out["streamSettings"]["sockopt"].is_null());
    }

    /// Пресет «сервер + фрагментация» должен и правда связать одно с другим.
    #[test]
    fn server_fragment_routes_the_dial_through_the_fragment_outbound() {
        let cfg = config("server-frag", 1081, Some(&reality())).unwrap();
        assert_eq!(cfg["outbounds"][0]["streamSettings"]["sockopt"]["dialerProxy"], "fragment");
        let tags: Vec<&str> =
            cfg["outbounds"].as_array().unwrap().iter().map(|o| o["tag"].as_str().unwrap()).collect();
        assert!(tags.contains(&"fragment"), "{tags:?}");
    }

    #[test]
    fn maps_websocket_transport() {
        let l = crate::link::parse("trojan://pass@1.2.3.4:8443?type=ws&path=%2Fws&host=cdn.example.com")
            .unwrap();
        let cfg = config("server", 1081, Some(&l)).unwrap();
        let s = &cfg["outbounds"][0]["streamSettings"];
        assert_eq!(s["network"], "ws");
        assert_eq!(s["wsSettings"]["path"], "/ws");
        assert_eq!(s["wsSettings"]["headers"]["Host"], "cdn.example.com");
        assert_eq!(cfg["outbounds"][0]["settings"]["servers"][0]["password"], "pass");
    }

    #[test]
    fn picks_only_the_windows_x64_archive() {
        assert!(is_windows_asset("Xray-windows-64.zip"));
        assert!(!is_windows_asset("Xray-windows-32.zip"));
        assert!(!is_windows_asset("Xray-windows-64.zip.dgst"));
        assert!(!is_windows_asset("Xray-linux-64.zip"));
    }
}
