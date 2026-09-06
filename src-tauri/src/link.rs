//! Разбор ссылок на прокси-серверы: `vless://`, `vmess://`, `trojan://`, `ss://`.
//!
//! Xray и sing-box отличаются от остальных движков тем, что умеют не только
//! ломать запросы на месте, но и увести трафик на чужой сервер. Сервер
//! пользователь приносит сам — обычно строкой из панели или бота. Форматы
//! этих строк не стандартизованы: у каждой панели свои мелкие вольности,
//! поэтому разбираем терпимо — чего не поняли, оставляем пустым, а не роняем
//! всю ссылку.
//!
//! Дальше из одной и той же разобранной ссылки `xray.rs` и `singbox.rs`
//! собирают свои, совершенно разные по форме конфиги.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServerLink {
    /// vless | vmess | trojan | shadowsocks
    pub proto: String,
    pub host: String,
    pub port: u16,
    /// uuid у vless и vmess, пароль у trojan и shadowsocks
    pub secret: String,
    /// метод шифрования shadowsocks
    pub method: String,
    /// xtls-rprx-vision и подобное
    pub flow: String,
    /// none | tls | reality
    pub security: String,
    pub sni: String,
    /// отпечаток браузера для uTLS: chrome, firefox, safari…
    pub fingerprint: String,
    /// открытый ключ Reality
    pub public_key: String,
    pub short_id: String,
    pub alpn: Vec<String>,
    /// tcp | ws | grpc | httpupgrade | h2
    pub network: String,
    pub path: String,
    /// заголовок Host у ws и h2
    pub host_header: String,
    /// имя сервиса gRPC
    pub service_name: String,
    /// не проверять сертификат — так делают панели с самоподписанным TLS
    pub insecure: bool,
    /// как сервер назвали в ссылке (то, что после #)
    pub label: String,
}

impl ServerLink {
    /// Короткая строка для интерфейса: по ней видно, что за сервер подключён,
    /// но не видно ни uuid, ни пароля — их показывать незачем.
    pub fn summary(&self) -> String {
        let mut out = format!("{} · {}:{}", self.proto, self.host, self.port);
        if self.security == "reality" {
            out.push_str(" · Reality");
        } else if self.security == "tls" {
            out.push_str(" · TLS");
        }
        if !self.network.is_empty() && self.network != "tcp" {
            out.push_str(" · ");
            out.push_str(&self.network);
        }
        if !self.label.is_empty() {
            out.push_str(&format!(" · «{}»", self.label));
        }
        out
    }
}

// ---------------------------------------------------------- вспомогательное

/// base64 без внешней зависимости: принимаем и обычный алфавит, и URL-safe,
/// и строку без выравнивающих «=» — панели присылают всё это вперемешку.
pub fn b64(input: &str) -> Result<Vec<u8>, String> {
    let mut bits: u32 = 0;
    let mut have = 0;
    let mut out = Vec::new();
    for ch in input.chars() {
        let v = match ch {
            'A'..='Z' => ch as u32 - 'A' as u32,
            'a'..='z' => ch as u32 - 'a' as u32 + 26,
            '0'..='9' => ch as u32 - '0' as u32 + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            '=' | '\n' | '\r' | ' ' | '\t' => continue,
            other => return Err(format!("в base64 попал лишний символ «{other}»")),
        };
        bits = (bits << 6) | v;
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push((bits >> have) as u8);
        }
    }
    Ok(out)
}

fn b64_text(input: &str) -> Result<String, String> {
    String::from_utf8(b64(input)?).map_err(|_| "base64 внутри ссылки — не текст".to_string())
}

/// Раскодирование %XX. Полноценный urldecode тут не нужен: в ссылках
/// встречаются пути, имена и пароли, а не форма из браузера.
pub fn percent(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn query_map(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (k.to_ascii_lowercase(), percent(v)),
            None => (p.to_ascii_lowercase(), String::new()),
        })
        .collect()
}

fn get(map: &[(String, String)], key: &str) -> String {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_default()
}

/// Хост и порт с оглядкой на IPv6: `[2001:db8::1]:443`.
fn split_host_port(text: &str) -> Result<(String, u16), String> {
    let (host, port) = if let Some(rest) = text.strip_prefix('[') {
        let (h, tail) = rest.split_once(']').ok_or("в адресе IPv6 не закрыта скобка")?;
        (h.to_string(), tail.trim_start_matches(':').to_string())
    } else {
        let (h, p) = text.rsplit_once(':').ok_or("в ссылке нет порта")?;
        (h.to_string(), p.to_string())
    };
    if host.is_empty() {
        return Err("в ссылке нет адреса сервера".into());
    }
    let port: u16 = port.parse().map_err(|_| format!("«{port}» — это не порт"))?;
    if port == 0 {
        return Err("порт не может быть нулём".into());
    }
    Ok((host, port))
}

/// Отрезает `#подпись` и `?параметры`, возвращая (тело, параметры, подпись).
fn split_parts(rest: &str) -> (String, Vec<(String, String)>, String) {
    let (body, label) = match rest.split_once('#') {
        Some((b, l)) => (b, percent(l)),
        None => (rest, String::new()),
    };
    let (body, query) = match body.split_once('?') {
        Some((b, q)) => (b, query_map(q)),
        None => (body, Vec::new()),
    };
    (body.to_string(), query, label)
}

/// Общие для vless и trojan параметры транспорта — они пишутся одинаково.
fn apply_query(link: &mut ServerLink, q: &[(String, String)]) {
    link.network = match get(q, "type").as_str() {
        "" => "tcp".into(),
        other => other.to_string(),
    };
    link.security = match get(q, "security").as_str() {
        "" | "none" => "none".into(),
        other => other.to_string(),
    };
    link.sni = {
        let sni = get(q, "sni");
        if sni.is_empty() { get(q, "peer") } else { sni }
    };
    link.fingerprint = get(q, "fp");
    link.public_key = get(q, "pbk");
    link.short_id = get(q, "sid");
    link.flow = get(q, "flow");
    link.path = get(q, "path");
    link.host_header = get(q, "host");
    link.service_name = {
        let name = get(q, "servicename");
        if name.is_empty() { get(q, "path") } else { name }
    };
    link.alpn = get(q, "alpn")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let insecure = get(q, "allowinsecure");
    link.insecure = insecure == "1" || insecure.eq_ignore_ascii_case("true");
    // Reality без открытого ключа не соберётся, а панели иногда пишут
    // security=tls, хотя pbk на месте
    if !link.public_key.is_empty() {
        link.security = "reality".into();
    }
}

// ---------------------------------------------------------------- протоколы

fn parse_userinfo(proto: &str, rest: &str) -> Result<ServerLink, String> {
    let (body, query, label) = split_parts(rest);
    let (user, addr) = body
        .rsplit_once('@')
        .ok_or("в ссылке нет «@» — не понять, где ключ, а где адрес")?;
    let (host, port) = split_host_port(addr)?;
    let secret = percent(user);
    if secret.is_empty() {
        return Err(match proto {
            "vless" => "в ссылке нет uuid".to_string(),
            _ => "в ссылке нет пароля".to_string(),
        });
    }
    let mut link =
        ServerLink { proto: proto.into(), host, port, secret, label, ..Default::default() };
    apply_query(&mut link, &query);
    // У trojan шифрование всегда TLS, даже если в ссылке об этом молчат
    if proto == "trojan" && link.security == "none" {
        link.security = "tls".into();
    }
    Ok(link)
}

/// `ss://base64(метод:пароль)@host:port#подпись` (SIP002) и старый формат,
/// где в base64 завёрнута вся строка целиком.
fn parse_ss(rest: &str) -> Result<ServerLink, String> {
    let (body, query, label) = split_parts(rest);
    let (userinfo, addr) = match body.rsplit_once('@') {
        Some((u, a)) => (percent(u), a.to_string()),
        None => {
            // старый формат: base64 спрятал и метод, и адрес
            let decoded = b64_text(&body)?;
            let (u, a) = decoded
                .rsplit_once('@')
                .ok_or("не разобрать ss-ссылку: после base64 нет «@»")?;
            (u.to_string(), a.to_string())
        }
    };
    // Внутри может лежать и открытый «метод:пароль», и он же в base64
    let creds = match userinfo.split_once(':') {
        Some(_) => userinfo.clone(),
        None => b64_text(&userinfo)?,
    };
    let (method, password) = creds
        .split_once(':')
        .ok_or("не разобрать ss-ссылку: нет пары «метод:пароль»")?;
    let (host, port) = split_host_port(&addr)?;

    Ok(ServerLink {
        proto: "shadowsocks".into(),
        host,
        port,
        secret: password.to_string(),
        method: method.to_string(),
        network: "tcp".into(),
        security: "none".into(),
        insecure: get(&query, "allowinsecure") == "1",
        label,
        ..Default::default()
    })
}

/// `vmess://base64(json)` — единственный формат, где параметры лежат объектом.
fn parse_vmess(rest: &str) -> Result<ServerLink, String> {
    let body = rest.split('#').next().unwrap_or(rest);
    let text = b64_text(body)?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("внутри vmess-ссылки не JSON: {e}"))?;

    // Поля пишут то строкой, то числом — читаем и так, и так
    let s = |key: &str| -> String {
        match v.get(key) {
            Some(serde_json::Value::String(x)) => x.clone(),
            Some(serde_json::Value::Number(n)) => n.to_string(),
            _ => String::new(),
        }
    };
    let host = s("add");
    if host.is_empty() {
        return Err("в vmess-ссылке нет адреса сервера".into());
    }
    let port: u16 =
        s("port").parse().map_err(|_| "в vmess-ссылке неверный порт".to_string())?;
    let secret = s("id");
    if secret.is_empty() {
        return Err("в vmess-ссылке нет uuid".into());
    }
    let tls = s("tls");
    Ok(ServerLink {
        proto: "vmess".into(),
        host,
        port,
        secret,
        network: match s("net").as_str() {
            "" => "tcp".into(),
            other => other.to_string(),
        },
        security: if tls.is_empty() || tls == "none" { "none".into() } else { tls },
        sni: {
            let sni = s("sni");
            if sni.is_empty() { s("host") } else { sni }
        },
        fingerprint: s("fp"),
        path: s("path"),
        host_header: s("host"),
        service_name: s("path"),
        alpn: s("alpn").split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
        label: s("ps"),
        ..Default::default()
    })
}

pub fn parse(raw: &str) -> Result<ServerLink, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Ссылка пустая".into());
    }
    let (scheme, rest) = raw
        .split_once("://")
        .ok_or("Не похоже на ссылку: нет «://». Жду vless://, vmess://, trojan:// или ss://")?;
    match scheme.to_ascii_lowercase().as_str() {
        "vless" => parse_userinfo("vless", rest),
        "trojan" => parse_userinfo("trojan", rest),
        "ss" => parse_ss(rest),
        "vmess" => parse_vmess(rest),
        other => Err(format!("Не знаю протокол «{other}» — умею vless, vmess, trojan и ss")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Кодирование нужно только тестам: приложение ссылки читает, а не пишет.
    fn to_b64(text: &str) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let b = text.as_bytes();
        let mut out = String::new();
        for chunk in b.chunks(3) {
            let n = ((chunk[0] as u32) << 16)
                | ((*chunk.get(1).unwrap_or(&0) as u32) << 8)
                | (*chunk.get(2).unwrap_or(&0) as u32);
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(A[((n >> (18 - i * 6)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    #[test]
    fn reads_vless_reality_link() {
        let link = parse(
            "vless://11111111-2222-3333-4444-555555555555@example.com:443?type=tcp&security=reality&pbk=KEY&fp=chrome&sni=www.microsoft.com&sid=ab12&flow=xtls-rprx-vision#%D0%9C%D0%BE%D0%B9",
        )
        .unwrap();
        assert_eq!(link.proto, "vless");
        assert_eq!(link.host, "example.com");
        assert_eq!(link.port, 443);
        assert_eq!(link.security, "reality");
        assert_eq!(link.public_key, "KEY");
        assert_eq!(link.short_id, "ab12");
        assert_eq!(link.flow, "xtls-rprx-vision");
        assert_eq!(link.sni, "www.microsoft.com");
        assert_eq!(link.label, "Мой");
        // В сводке для интерфейса секретов быть не должно
        assert!(!link.summary().contains("11111111"), "{}", link.summary());
    }

    #[test]
    fn reads_trojan_over_websocket() {
        let link =
            parse("trojan://p%40ss@1.2.3.4:8443?type=ws&path=%2Fws&host=cdn.example.com#RU").unwrap();
        assert_eq!(link.secret, "p@ss");
        assert_eq!(link.network, "ws");
        assert_eq!(link.path, "/ws");
        assert_eq!(link.host_header, "cdn.example.com");
        // trojan всегда под TLS, даже когда в ссылке об этом не сказано
        assert_eq!(link.security, "tls");
    }

    #[test]
    fn reads_both_shadowsocks_shapes() {
        // SIP002: base64 только в имени пользователя
        let sip = parse("ss://YWVzLTI1Ni1nY206cGFzcw==@1.2.3.4:8388#node").unwrap();
        assert_eq!(sip.method, "aes-256-gcm");
        assert_eq!(sip.secret, "pass");
        assert_eq!(sip.port, 8388);
        // старый формат: base64 на всю строку
        let old = parse(&format!("ss://{}#node", to_b64("aes-256-gcm:pass@1.2.3.4:8388"))).unwrap();
        assert_eq!(old.method, sip.method);
        assert_eq!(old.host, sip.host);
        assert_eq!(old.port, sip.port);
    }

    #[test]
    fn reads_vmess_json() {
        let json = r#"{"v":"2","ps":"tokyo","add":"1.2.3.4","port":"443","id":"uuid-here","net":"ws","tls":"tls","host":"cdn.example.com","path":"/x"}"#;
        let link = parse(&format!("vmess://{}", to_b64(json))).unwrap();
        assert_eq!(link.proto, "vmess");
        assert_eq!(link.host, "1.2.3.4");
        assert_eq!(link.network, "ws");
        assert_eq!(link.security, "tls");
        assert_eq!(link.label, "tokyo");
        assert_eq!(link.path, "/x");
    }

    #[test]
    fn keeps_ipv6_address_together() {
        let link = parse("vless://uuid@[2001:db8::1]:443?security=tls").unwrap();
        assert_eq!(link.host, "2001:db8::1");
        assert_eq!(link.port, 443);
    }

    /// Ошибка должна объяснять, что не так, а не просто «не разобрал».
    #[test]
    fn explains_what_is_wrong() {
        for (raw, expect) in [
            ("", "пустая"),
            ("просто текст", "://"),
            ("wireguard://x@y:1", "протокол"),
            ("vless://uuid@example.com", "порт"),
            ("vless://example.com:443", "@"),
        ] {
            let err = parse(raw).expect_err(raw);
            assert!(err.contains(expect), "для «{raw}» получили: {err}");
        }
    }
}
