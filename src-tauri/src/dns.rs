//! DNS: где спрашивают адрес сайта и слышно ли этот вопрос со стороны.
//!
//! Диагностика давно умела заметить подмену: провайдер отвечает на запрос
//! адресом своей заглушки, браузер уходит не туда ещё до всякого DPI, и обход
//! тут бессилен — резать нечего, соединение и так идёт не к тому серверу.
//! Совет при этом был «пропиши 1.1.1.1 или включи DNS-over-HTTPS в Windows»:
//! приложение ставило диагноз и отправляло лечиться своими силами.
//!
//! Три решения, которые стоит объяснить.
//!
//! **Кто несёт трафик — спрашиваем у самой Windows**, через `Find-NetRoute`.
//! Таблица маршрутов по умолчанию при поднятом туннеле врёт: WARP забирает
//! трафик парой половинных маршрутов, и в списке `0.0.0.0/0` его нет вовсе —
//! там остаётся физический адаптер, через который на самом деле уже ничего
//! не идёт. Спросить «каким путём ты пойдёшь вот на этот адрес» надёжнее,
//! чем толковать таблицу самим.
//!
//! **Чужой DNS не трогаем.** Резолвер на 127.х означает, что имена разрешает
//! чужая программа — WARP, AdGuard, Xray. Прописать поверх свой адрес значит
//! сломать её, а не помочь; поэтому здесь отказ с именем виновника, ровно как
//! диагностика поступает с чужими обходами.
//!
//! **Шифрование включаем тем же способом, что и сама Windows.** `DohFlags` =
//! 1 (QWORD) — «включено, шаблон автоматический»; шаблоны для Cloudflare,
//! Google и Quad9 у Windows 11 свои, встроенные, так что регистрировать их
//! не нужно. Порядок обратный привычному: сперва флаг, потом адреса — смена
//! адресов поднимает уведомление о перенастройке сети, и служба DNS
//! перечитывает флаг сама, без перезапуска.

use crate::sysutil::{out_text, run_hidden};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DNSCACHE: &str =
    r"HKLM\SYSTEM\CurrentControlSet\Services\Dnscache\InterfaceSpecificParameters";

/// Резолверы, для которых у Windows 11 уже есть свой шаблон DoH. Список
/// сверен с тем, что отдаёт `Get-DnsClientDohServerAddress`: чужой адрес
/// сюда добавлять нельзя — шифрование молча не включится.
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub v4: [&'static str; 2],
    pub v6: [&'static str; 2],
    pub note: &'static str,
}

pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "cloudflare",
        name: "Cloudflare",
        v4: ["1.1.1.1", "1.0.0.1"],
        v6: ["2606:4700:4700::1111", "2606:4700:4700::1001"],
        note: "Обычно отвечает быстрее прочих и не ведёт журнал запросов",
    },
    Provider {
        id: "google",
        name: "Google",
        v4: ["8.8.8.8", "8.8.4.4"],
        v6: ["2001:4860:4860::8888", "2001:4860:4860::8844"],
        note: "Вездесущий и надёжный, отвечает чуть медленнее Cloudflare",
    },
    Provider {
        id: "quad9",
        name: "Quad9",
        v4: ["9.9.9.9", "149.112.112.112"],
        v6: ["2620:fe::fe", "2620:fe::9"],
        note: "Заодно отсекает известные вредоносные адреса",
    },
];

pub fn provider(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Что вернула Windows. Списки адресов приходят строкой через запятую
/// намеренно: `ConvertTo-Json` в PowerShell 5.1 разворачивает массив из
/// одного элемента в простое значение, и разбор ломался бы ровно тогда,
/// когда сервер прописан один.
#[derive(Deserialize, Default)]
struct RawState {
    #[serde(default)]
    index: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    guid: String,
    #[serde(default, rename = "virtual_")]
    virtual_adapter: bool,
    #[serde(default)]
    v4: String,
    #[serde(default)]
    v6: String,
    /// Непусто — адреса прописаны руками, пусто — пришли от роутера
    #[serde(default)]
    static_ns: String,
    #[serde(default)]
    doh: String,
    /// Имя процесса, который слушает 53-й порт на 127.х
    #[serde(default)]
    owner: String,
}

/// Что показываем и на что опираемся, решая, можно ли трогать настройки.
#[derive(Serialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DnsState {
    /// Адаптер, которым Windows реально пойдёт наружу
    pub adapter: String,
    pub index: u32,
    pub guid: String,
    pub servers: Vec<String>,
    /// Адреса выданы роутером, а не прописаны руками
    pub from_dhcp: bool,
    /// Шифрование включено для всех текущих адресов
    pub encrypted: bool,
    /// Узнанный резолвер: Cloudflare, Google, Quad9
    pub provider: Option<String>,
    /// Имена разрешает чужая программа — менять настройки нельзя
    pub owner: Option<String>,
    /// Наружу ведёт виртуальный адаптер: поднят туннель
    pub tunnel: bool,
    /// Прочитать состояние не удалось
    pub error: Option<String>,
    /// Почему трогать настройки нельзя, словами. Считается здесь, а не
    /// в интерфейсе: правило одно, и вторая его копия однажды разойдётся
    /// с первой — а расходится она в сторону «кнопка нажалась, сеть легла».
    pub blocked: Option<String>,
}

/// То, что было до нас. Без этого «вернуть как было» — пустое обещание:
/// адреса от роутера и прописанные руками возвращаются по-разному.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedDns {
    pub guid: String,
    pub index: u32,
    pub adapter: String,
    pub from_dhcp: bool,
    pub v4: Vec<String>,
    pub v6: Vec<String>,
}

fn split(list: &str) -> Vec<String> {
    list.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// Спрашиваем всё разом. Состояние собирается после каждого действия
/// пользователя, а запуск PowerShell дорог: один вызов вместо пяти.
const READ_SCRIPT: &str = r#"
$ErrorActionPreference='SilentlyContinue'
[Console]::OutputEncoding=[System.Text.Encoding]::UTF8
$r=Find-NetRoute -RemoteIPAddress '203.0.113.7'|Select-Object -First 1
if(-not $r){'{}';exit}
$i=$r.InterfaceIndex
$ad=Get-NetAdapter -InterfaceIndex $i
$g=$ad.InterfaceGuid
$v4=@((Get-DnsClientServerAddress -InterfaceIndex $i -AddressFamily IPv4).ServerAddresses)
$v6=@((Get-DnsClientServerAddress -InterfaceIndex $i -AddressFamily IPv6).ServerAddresses)
$reg=Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\$g"
$doh=@()
foreach($leaf in 'Doh','Doh6'){
  $k="HKLM:\SYSTEM\CurrentControlSet\Services\Dnscache\InterfaceSpecificParameters\$g\DohInterfaceSettings\$leaf"
  if(Test-Path $k){$doh+=@(Get-ChildItem $k|ForEach-Object{$_.PSChildName})}
}
$owner=''
$loop=$v4|Where-Object{$_ -like '127.*'}|Select-Object -First 1
if($loop){
  $ep=Get-NetUDPEndpoint -LocalPort 53|Where-Object{$_.LocalAddress -eq $loop}|Select-Object -First 1
  if($ep){$pr=Get-Process -Id $ep.OwningProcess; if($pr){$owner=$pr.ProcessName}}
}
[pscustomobject]@{
  index=$i; name=[string]$ad.Name; guid=[string]$g; virtual_=[bool]$ad.Virtual
  v4=($v4 -join ','); v6=($v6 -join ','); static_ns=[string]$reg.NameServer
  doh=($doh -join ','); owner=$owner
}|ConvertTo-Json -Compress
"#;

static CACHE: Mutex<Option<(Instant, DnsState)>> = Mutex::new(None);
const CACHE_TTL: Duration = Duration::from_secs(3);

/// Забыть запомненное — после каждой своей правки.
pub fn forget() {
    *CACHE.lock().unwrap() = None;
}

fn read_now() -> DnsState {
    let out = run_hidden(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", READ_SCRIPT],
    );
    let text = match out {
        Ok(o) => out_text(&o),
        Err(e) => {
            return DnsState {
                error: Some(format!("не удалось спросить Windows: {e}")),
                ..Default::default()
            }
        }
    };
    let raw: RawState = match text.find('{').and_then(|s| serde_json::from_str(&text[s..]).ok()) {
        Some(r) => r,
        None => {
            return DnsState {
                error: Some("Windows не сказала, каким адаптером идёт трафик".into()),
                ..Default::default()
            }
        }
    };

    let servers = split(&raw.v4);
    let doh = split(&raw.doh);
    let all = [servers.clone(), split(&raw.v6)].concat();
    // Зашифровано, только если флаг стоит у каждого адреса: один незакрытый
    // резолвер и есть та дырка, ради которой всё затевалось
    let encrypted = !all.is_empty() && all.iter().all(|s| doh.contains(s));
    let known = PROVIDERS
        .iter()
        .find(|p| servers.iter().any(|s| p.v4.contains(&s.as_str())))
        .map(|p| p.name.to_string());

    let mut s = DnsState {
        adapter: raw.name,
        index: raw.index,
        guid: raw.guid,
        servers,
        from_dhcp: raw.static_ns.trim().is_empty(),
        encrypted,
        provider: known,
        owner: (!raw.owner.is_empty()).then_some(raw.owner),
        tunnel: raw.virtual_adapter,
        error: None,
        blocked: None,
    };
    s.blocked = refusal(&s);
    s
}

pub fn state() -> DnsState {
    if let Some((at, s)) = CACHE.lock().unwrap().as_ref() {
        if at.elapsed() < CACHE_TTL {
            return s.clone();
        }
    }
    let fresh = read_now();
    *CACHE.lock().unwrap() = Some((Instant::now(), fresh.clone()));
    fresh
}

/// Почему трогать настройки нельзя. Отдельной функцией, потому что спросить
/// об этом нужно дважды: чтобы погасить кнопку и чтобы не выполнить команду,
/// пришедшую в обход погашенной кнопки.
pub fn refusal(s: &DnsState) -> Option<String> {
    if let Some(e) = &s.error {
        return Some(e.clone());
    }
    if let Some(owner) = &s.owner {
        return Some(format!(
            "Имена сейчас разрешает {owner} — свой резолвер на 127.х. Прописать поверх наши адреса значит сломать его, а не помочь: сначала выключи эту программу"
        ));
    }
    if s.tunnel {
        return Some(format!(
            "Наружу ведёт виртуальный адаптер «{}» — поднят туннель, и DNS принадлежит ему. Настройки физического адаптера сейчас ни на что не влияют",
            s.adapter
        ));
    }
    if s.index == 0 || s.guid.is_empty() {
        return Some("Не видно адаптера, которым идёт трафик, — похоже, сети нет".into());
    }
    None
}

fn powershell(script: &str) -> Result<String, String> {
    let out = run_hidden(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script],
    )
    .map_err(|e| e.to_string())?;
    let text = out_text(&out);
    if !out.status.success() {
        return Err(text.trim().to_string());
    }
    Ok(text)
}

fn set_servers(index: u32, list: &[String]) -> Result<(), String> {
    let script = if list.is_empty() {
        // Сброс возвращает адаптер к тому, что даёт роутер
        format!(
            "Set-DnsClientServerAddress -InterfaceIndex {index} -ResetServerAddresses -ErrorAction Stop"
        )
    } else {
        let quoted: Vec<String> = list.iter().map(|s| format!("'{s}'")).collect();
        format!(
            "Set-DnsClientServerAddress -InterfaceIndex {index} -ServerAddresses ({}) -ErrorAction Stop",
            quoted.join(",")
        )
    };
    powershell(&script).map(|_| ()).map_err(|e| format!("не удалось прописать адреса: {e}"))
}

/// Ключ шифрования для одного адреса. IPv6 живёт в соседней ветке `Doh6`.
fn doh_key(guid: &str, addr: &str) -> String {
    let leaf = if addr.contains(':') { "Doh6" } else { "Doh" };
    format!(r"{DNSCACHE}\{guid}\DohInterfaceSettings\{leaf}\{addr}")
}

fn set_doh(guid: &str, addrs: &[&str]) -> Result<(), String> {
    for addr in addrs {
        let key = doh_key(guid, addr);
        let out =
            run_hidden("reg", &["add", &key, "/v", "DohFlags", "/t", "REG_QWORD", "/d", "1", "/f"])
                .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "не удалось включить шифрование для {addr}: {}",
                out_text(&out).trim()
            ));
        }
    }
    Ok(())
}

fn clear_doh(guid: &str, addrs: &[String]) {
    for addr in addrs {
        // Уже удалённый ключ — не ошибка: возврат должен доводиться до конца
        let _ = run_hidden("reg", &["delete", &doh_key(guid, addr), "/f"]);
    }
}

/// Снимок для «вернуть как было». Снимается до первой правки и только один
/// раз: второй вызов записал бы уже наши адреса, и возвращать стало бы некуда.
pub fn snapshot(s: &DnsState) -> SavedDns {
    SavedDns {
        guid: s.guid.clone(),
        index: s.index,
        adapter: s.adapter.clone(),
        from_dhcp: s.from_dhcp,
        v4: if s.from_dhcp { Vec::new() } else { s.servers.clone() },
        v6: Vec::new(),
    }
}

/// Включает выбранный резолвер вместе с шифрованием.
pub fn enable(p: &Provider, s: &DnsState) -> Result<(), String> {
    if let Some(no) = refusal(s) {
        return Err(no);
    }
    // Сперва флаг, потом адреса: смена адресов поднимает уведомление
    // о перенастройке сети, и служба DNS перечитывает флаг сама
    let mut all: Vec<&str> = p.v4.to_vec();
    all.extend_from_slice(&p.v6);
    set_doh(&s.guid, &all)?;

    let v4: Vec<String> = p.v4.iter().map(|x| x.to_string()).collect();
    let v6: Vec<String> = p.v6.iter().map(|x| x.to_string()).collect();
    // IPv6 задаём тоже: оставить его провайдеру значит оставить ему же
    // и подмену — запросы просто уйдут по другому семейству адресов
    set_servers(s.index, &[v4, v6].concat())?;
    let _ = run_hidden("ipconfig", &["/flushdns"]);
    forget();
    Ok(())
}

/// Возвращает то, что было до нас.
pub fn restore(saved: &SavedDns) -> Result<(), String> {
    // Чистим ключи шифрования по всем своим адресам: какой именно резолвер
    // был выбран, к моменту возврата уже неизвестно
    let ours: Vec<String> = PROVIDERS
        .iter()
        .flat_map(|p| p.v4.iter().chain(p.v6.iter()))
        .map(|s| s.to_string())
        .collect();
    clear_doh(&saved.guid, &ours);

    let back: Vec<String> =
        if saved.from_dhcp { Vec::new() } else { [saved.v4.clone(), saved.v6.clone()].concat() };
    set_servers(saved.index, &back)?;
    let _ = run_hidden("ipconfig", &["/flushdns"]);
    forget();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Адреса резолверов должны быть ровно те, для которых у Windows есть
    /// свой шаблон DoH. Опечатка в одной цифре — и шифрование молча не
    /// включится: адреса пропишутся, флаг ляжет к несуществующему серверу,
    /// а человеку приложение скажет «готово».
    #[test]
    fn provider_addresses_are_the_ones_windows_knows() {
        // Список снят с живого `Get-DnsClientDohServerAddress` на Windows 11
        let known = [
            "1.1.1.1",
            "1.0.0.1",
            "8.8.8.8",
            "8.8.4.4",
            "9.9.9.9",
            "149.112.112.112",
            "2606:4700:4700::1111",
            "2606:4700:4700::1001",
            "2001:4860:4860::8888",
            "2001:4860:4860::8844",
            "2620:fe::fe",
            "2620:fe::9",
        ];
        for p in PROVIDERS {
            for a in p.v4.iter().chain(p.v6.iter()) {
                assert!(known.contains(a), "{}: у Windows нет шаблона DoH для {a}", p.name);
            }
        }
        assert_eq!(PROVIDERS.len(), 3);
        assert!(provider("cloudflare").is_some());
        assert!(provider("нет такого").is_none());
    }

    /// IPv6 обязан уходить в соседнюю ветку: положить его в `Doh` — то же
    /// самое, что не включить шифрование вовсе.
    #[test]
    fn ipv6_goes_to_its_own_branch() {
        let g = "{GUID}";
        assert!(doh_key(g, "1.1.1.1").ends_with(r"DohInterfaceSettings\Doh\1.1.1.1"));
        assert!(doh_key(g, "2606:4700:4700::1111")
            .ends_with(r"DohInterfaceSettings\Doh6\2606:4700:4700::1111"));
    }

    /// Разбор ответа Windows: один сервер, несколько, пусто.
    #[test]
    fn reads_address_lists_in_every_shape() {
        assert_eq!(split("1.1.1.1"), vec!["1.1.1.1"]);
        assert_eq!(split("1.1.1.1,1.0.0.1"), vec!["1.1.1.1", "1.0.0.1"]);
        assert_eq!(split(" 1.1.1.1 , 1.0.0.1 "), vec!["1.1.1.1", "1.0.0.1"]);
        assert!(split("").is_empty());
        assert!(split(",,").is_empty());
    }

    /// Чужой резолвер, туннель и отсутствие сети — три случая, когда трогать
    /// настройки нельзя. Молчаливое согласие тут ломает человеку сеть.
    #[test]
    fn refuses_to_touch_someone_elses_dns() {
        let good = DnsState {
            adapter: "Ethernet".into(),
            index: 11,
            guid: "{G}".into(),
            servers: vec!["192.168.1.1".into()],
            from_dhcp: true,
            ..Default::default()
        };
        assert!(refusal(&good).is_none(), "обычный адаптер трогать можно");

        let warp = DnsState { owner: Some("warp-svc".into()), ..good.clone() };
        assert!(refusal(&warp).unwrap().contains("warp-svc"), "виновника надо назвать");

        let tun = DnsState { tunnel: true, ..good.clone() };
        assert!(refusal(&tun).is_some(), "поднятый туннель владеет DNS сам");

        let none = DnsState { index: 0, guid: String::new(), ..good.clone() };
        assert!(refusal(&none).is_some(), "без адаптера менять нечего");

        let broken = DnsState { error: Some("сломалось".into()), ..good.clone() };
        assert_eq!(refusal(&broken).as_deref(), Some("сломалось"));
    }

    /// Адреса от роутера и прописанные руками возвращаются по-разному:
    /// первые — сбросом, вторые — записью. Перепутать значит подменить
    /// человеку настройку, пообещав вернуть её.
    #[test]
    fn remembers_how_the_addresses_were_given() {
        let dhcp =
            DnsState { from_dhcp: true, servers: vec!["192.168.1.1".into()], ..Default::default() };
        let saved = snapshot(&dhcp);
        assert!(saved.from_dhcp);
        assert!(saved.v4.is_empty(), "адреса от роутера запоминать незачем — вернём сбросом");

        let manual = DnsState {
            from_dhcp: false,
            servers: vec!["77.88.8.8".into(), "77.88.8.1".into()],
            ..Default::default()
        };
        let saved = snapshot(&manual);
        assert!(!saved.from_dhcp);
        assert_eq!(saved.v4, vec!["77.88.8.8", "77.88.8.1"], "свои адреса вернём как были");
    }

    /// Разбор живого ответа Windows. Всё выше проверяет наши собственные
    /// правила на выдуманных данных; здесь — что мы вообще поняли машину,
    /// на которой работаем. Под переменной, потому что на чужой сборке
    /// сети может не быть вовсе.
    #[test]
    fn reads_the_real_machine() {
        if std::env::var("ZAPRET_NET_TEST").is_err() {
            return;
        }
        forget();
        let s = state();
        println!("адаптер:    {} (индекс {}, туннель: {})", s.adapter, s.index, s.tunnel);
        println!("guid:       {}", s.guid);
        println!("серверы:    {:?}", s.servers);
        println!("от роутера: {}", s.from_dhcp);
        println!("шифрование: {}", s.encrypted);
        println!("резолвер:   {:?}", s.provider);
        println!("владелец:   {:?}", s.owner);
        println!("отказ:      {:?}", refusal(&s));
        assert!(s.error.is_none(), "состояние не прочиталось: {:?}", s.error);
        assert!(!s.adapter.is_empty(), "имя адаптера обязано быть");
        assert!(s.index > 0, "индекс адаптера обязан быть");
        assert!(s.guid.starts_with('{'), "guid обязан быть guid-ом: {}", s.guid);
    }

    /// Состояние спрашивается после каждого действия, а запуск PowerShell
    /// дорог — второй вызов подряд обязан прийти из памяти.
    #[test]
    fn keeps_the_answer_for_a_few_seconds() {
        forget();
        let _ = state();
        assert!(CACHE.lock().unwrap().is_some(), "ответ должен запоминаться");
        forget();
        assert!(CACHE.lock().unwrap().is_none(), "forget обязан очищать память");
    }
}
