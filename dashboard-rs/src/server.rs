//! HTTP 服务: 供 Kindle 拉取 dashboard.png + Web 配置管理页与 API。
//! API 契约沿用原 Python 版(dashboard.py,已删除), 契约稳定便于脚本调用。
use socket2::{Domain, Protocol, Socket, Type};
use std::io::Read;
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct Shared {
    pub port: u16,
    pub lan_ip: String,
    pub last_render: Mutex<String>,
    /// 通知渲染线程立刻刷新(Sender 放 Mutex 以便跨线程共享)
    pub refresh: Mutex<std::sync::mpsc::Sender<()>>,
}

fn json(value: serde_json::Value) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    tiny_http::Response::from_string(value.to_string())
        .with_header(header("Content-Type", "application/json").unwrap())
}

fn json_ok() -> serde_json::Value {
    serde_json::json!({"ok": true})
}

fn header(name: &str, value: &str) -> Option<tiny_http::Header> {
    tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).ok()
}

/// 取连接 Kindle 所在网段的源 IP。多网卡时不能固定连公网地址，
/// 否则可能选到另一张网卡，导致 Kindle 拿到不可达的图片地址。
pub fn lan_ip_for(peer: Option<&str>) -> String {
    let target = peer
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("{s}:2222"))
        .unwrap_or_else(|| "223.5.5.5:80".into());
    let sock = UdpSocket::bind("0.0.0.0:0").and_then(|s| s.connect(target).map(|_| s));
    match sock {
        Ok(s) => s.local_addr().map(|a| a.ip().to_string()).unwrap_or_else(|_| "127.0.0.1".into()),
        Err(_) => "127.0.0.1".into(),
    }
}

/// 局域网 Kindle 探测: 读本机 ARP 表(零流量, Kindle 定时拉图就会在表里),
/// 再对在线设备并发探测 KOReader SSH(2222)与 USBNetwork SSH(22),
/// 每台只发 2 个 TCP 包(350ms 超时), 不 ping 全网、不改设备状态。
#[derive(serde::Serialize)]
struct ScanDev {
    ip: String,
    mac: String,
    ports: Vec<u16>,
    guess: String,
}

fn parse_arp_line(line: &str) -> Option<(String, String)> {
    let toks: Vec<String> = line.split_whitespace()
        .map(|t| t.trim_matches(|c| c == '(' || c == ')').to_string())
        .collect();
    let ip = toks.iter().rev().find(|t| {
        let parts: Vec<&str> = t.split('.').collect();
        parts.len() == 4
            && parts.iter().all(|p| !p.is_empty() && p.parse::<u8>().is_ok())
    })?;
    let mac = toks.iter().find(|t| {
        let parts: Vec<&str> = t.split(|c| c == ':' || c == '-').collect();
        parts.len() == 6
            && parts.iter().all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
    })?;
    let mac = mac.replace('-', ":").to_lowercase();
    if mac == "00:00:00:00:00:00" || mac == "ff:ff:ff:ff:ff:ff" {
        return None;
    }
    Some((ip.clone(), mac))
}

/// connect 成功即端口开放; 返回服务 banner(多数 SSH 会主动发, 仅作展示)
fn probe_port(ip: &str, port: u16) -> Option<String> {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;
    let sa: SocketAddr = format!("{}:{}", ip, port).parse().ok()?;
    let mut s = TcpStream::connect_timeout(&sa, Duration::from_millis(350)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(250))).ok();
    let mut buf = [0u8; 64];
    let n = std::io::Read::read(&mut s, &mut buf).unwrap_or(0);
    Some(String::from_utf8_lossy(&buf[..n]).trim().to_string())
}

const AMAZON_OUI: &[&str] = &[
    "00:71:47", "00:86:21", "00:bb:3a", "00:f3:61", "00:fc:8b", "08:12:a5",
    "08:57:fb", "08:6a:e5", "08:7b:0f", "08:7c:39", "08:84:9d", "08:91:15",
    "08:91:a3", "08:a6:bc", "08:c2:24", "0c:43:f9", "0c:47:c9", "0c:dc:91",
    "0c:ee:99", "10:09:f9", "10:96:93", "10:ae:60", "10:bf:67", "10:ce:02",
    "14:0a:c5", "14:91:38", "18:0b:1b", "18:48:be", "18:74:2e", "1c:12:b0",
    "1c:4d:66", "1c:93:c4", "1c:fe:2b", "20:10:b1", "20:a1:71", "20:be:b8",
    "20:d4:76", "20:fe:00", "24:4c:e3", "24:ce:33", "28:24:c9", "28:73:f6",
    "28:ef:01", "2c:71:ff", "30:4d:1f", "34:25:be", "34:af:b3", "34:d2:70",
    "38:6f:f4", "38:f7:3d", "3c:5c:c4", "3c:e4:41", "40:89:c6", "40:a2:db",
    "40:a9:cf", "40:b4:cd", "40:f6:bc", "44:00:49", "44:3d:54", "44:42:01",
    "44:65:0d", "44:6d:7f", "44:7b:45", "44:b4:b2", "44:d5:cc", "48:43:dd",
    "48:5f:2d", "48:78:5e", "48:b4:23", "4c:17:44", "4c:53:fd", "4c:60:ad",
    "4c:ef:c0", "50:07:c3", "50:99:5a", "50:d4:5c", "50:dc:e7", "50:f5:da",
    "54:2b:1c", "58:09:87", "58:9a:3e", "58:a8:e8", "58:e4:88", "5c:8b:6b",
    "64:c9:f1", "64:cd:c2", "68:01:25", "68:13:f3", "68:37:e9", "68:54:fd",
    "68:9a:87", "68:9f:d4", "68:b6:91", "68:db:f5", "68:f6:3b", "6c:0c:9a",
    "6c:55:b1", "6c:56:97", "6c:68:8a", "6c:99:9d", "70:70:aa", "74:58:f3",
    "74:75:48", "74:a7:ea", "74:c2:46", "74:d4:23", "74:d6:37", "74:e2:0c",
    "74:ec:b2", "78:6c:84", "78:a0:3f", "78:e1:03", "7c:61:66", "7c:63:05",
    "7c:d5:66", "7c:ed:c6", "80:0c:f9", "80:6d:71", "80:a6:3c", "84:28:59",
    "84:48:80", "84:d6:d0", "88:29:bf", "88:71:e5", "8c:19:52", "8c:2a:85",
    "8c:4e:bb", "90:11:95", "90:23:5b", "90:39:5f", "90:a8:22", "90:f8:2e",
    "94:3a:91", "94:59:15", "94:5a:fc", "98:22:6e", "98:cc:f3", "9c:c8:e9",
    "a0:02:dc", "a0:d0:dc", "a0:d2:b1", "a4:02:b7", "a4:08:01", "a8:ca:77",
    "a8:e6:21", "ac:41:6a", "ac:63:be", "ac:cc:fc", "b0:73:9c", "b0:8b:a8",
    "b0:cf:cb", "b0:f7:c4", "b0:fc:0d", "b4:10:7a", "b4:7c:9c", "b4:b7:42",
    "b4:e4:54", "b8:5f:98", "b8:fe:96", "c0:01:95", "c0:8d:51", "c0:91:b9",
    "c0:95:cf", "c4:95:00", "c4:96:9f", "c8:6c:3d", "c8:c9:b2", "cc:22:93",
    "cc:9e:a2", "cc:af:e3", "cc:f7:35", "d4:91:0f", "d8:be:65", "d8:fb:d6",
    "dc:54:d7", "dc:91:bf", "dc:a0:d0", "e0:cb:1d", "e0:f7:28", "e8:4c:4a",
    "e8:d8:7e", "ec:0d:e4", "ec:2b:eb", "ec:31:5f", "ec:8a:c4", "ec:a1:38",
    "f0:27:2d", "f0:2f:9e", "f0:4f:7c", "f0:81:73", "f0:a2:25", "f0:d2:f1",
    "f0:f0:a4", "f4:03:2a", "f8:54:b8", "f8:fc:e1", "fc:0f:76", "fc:49:2d",
    "fc:65:de", "fc:a1:83", "fc:a6:67", "fc:d7:49", "fc:e9:d8",
];

fn is_amazon(mac: &str) -> bool {
    mac.len() >= 8 && AMAZON_OUI.contains(&&mac[..8])
}

fn scan_kindle() -> serde_json::Value {
    let text = std::process::Command::new("arp").args(["-a", "-n"]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let mut entries: Vec<(String, String)> = text.lines()
        .filter_map(parse_arp_line)
        .collect();
    entries.sort();
    entries.dedup_by(|a, b| a.0 == b.0);
    entries.truncate(128); // 大网络下限制并发规模

    let handles: Vec<_> = entries.into_iter().map(|(ip, mac)| {
        std::thread::spawn(move || {
            let ports: Vec<u16> = [2222u16, 22].into_iter()
                .filter(|p| probe_port(&ip, *p).is_some())
                .collect();
            (ip, mac, ports)
        })
    }).collect();
    let mut devs: Vec<ScanDev> = Vec::new();
    for h in handles {
        let (ip, mac, mut ports) = h.join().unwrap_or((String::new(), String::new(), Vec::new()));
        let amazon = is_amazon(&mac);
        if ports.is_empty() && !amazon {
            continue; // 非 Amazon 且无 SSH: 与找 Kindle 无关, 不列
        }
        let guess = match (amazon, ports.contains(&2222), ports.contains(&22)) {
            (true, true, _) => "Amazon 设备 · KOReader SSH 就绪".to_string(),
            (true, false, true) => "Amazon 设备 · SSH 已开, 若为 Kindle 可直接填入".to_string(),
            (true, false, false) => "Amazon 设备(按 MAC 厂商判定) · SSH 未开, 推送前需开启".to_string(),
            (false, true, _) => "开放 KOReader 端口 · 疑似 KOReader 设备".to_string(),
            (false, false, true) => "SSH 设备 · 可能为路由器等, 非 Amazon 厂商".to_string(),
            _ => String::new(),
        };
        ports.sort();
        devs.push(ScanDev { ip, mac, ports, guess });
    }
    devs.sort_by_key(|d| (!d.guess.starts_with("Amazon"),
        !d.ports.contains(&2222), d.ip.clone()));
    serde_json::json!({ "devices": devs, "scanned": devs.len() })
}

fn mime_of(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "png" => "image/png",
        "html" => "text/html; charset=utf-8",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

/// 只对外提供这几类文件(尤其避免把 out/edge_profile 里的浏览器配置暴露到局域网)
const ALLOWED_EXT: &[&str] = &["png", "html", "json"];

fn read_body(request: &mut tiny_http::Request, cap: usize) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    request.as_reader().take(cap as u64).read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

fn trigger_refresh(shared: &Shared) {
    let _ = shared.refresh.lock().unwrap().send(());
}

/// GET /api/status
fn api_status(shared: &Shared, base: &std::path::Path) -> serde_json::Value {
    let cfg = crate::config::load(base);
    serde_json::json!({
        "lan_ip": shared.lan_ip,
        "port": shared.port,
        "last_render": shared.last_render.lock().unwrap().clone(),
        "template": cfg.template,
        "orientation": cfg.orientation,
    })
}

/// GET /api/config: 当前 config.json(缺字段用内置默认值补齐)
fn api_config_get(base: &std::path::Path) -> serde_json::Value {
    let mut value = std::fs::read_to_string(base.join("config.json")).ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    // 用默认配置补齐缺失字段, webui 端即可假定字段齐全
    let defaults: serde_json::Value =
        serde_json::from_str(include_str!("../resources/config.json")).unwrap();
    if let (Some(obj), Some(def)) = (value.as_object_mut(), defaults.as_object()) {
        for (k, v) in def {
            obj.entry(k.clone()).or_insert(v.clone());
        }
    }
    value
}

/// 配置字段校验, POST /api/config 与 POST /api/preview 共用(预览所见即保存所得)
fn validate_config(obj: &serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    let city = obj.get("city").and_then(|v| v.as_str()).map(str::trim).unwrap_or("");
    if city.is_empty() {
        return Err("城市不能为空".into());
    }
    let port = obj.get("port").and_then(|v| v.as_u64()).unwrap_or(8787);
    if !(1..=65535).contains(&port) {
        return Err(format!("端口 {port} 超出范围"));
    }
    if obj.get("interval_min").and_then(|v| v.as_u64()).unwrap_or(10) == 0 {
        return Err("刷新间隔至少 1 分钟".into());
    }
    let orientation = obj.get("orientation").and_then(|v| v.as_str()).unwrap_or("portrait");
    if orientation != "portrait" && orientation != "landscape" {
        return Err(format!("未知朝向: {orientation}"));
    }
    Ok(())
}

/// POST /api/config: 校验 -> 合并保存 -> 触发刷新
fn api_config_post(base: &std::path::Path, body: &[u8]) -> Result<serde_json::Value, String> {
    let new: serde_json::Value = serde_json::from_slice(body).map_err(|e| format!("JSON 解析失败: {e}"))?;
    let obj = new.as_object().ok_or("配置必须是 JSON 对象")?;
    validate_config(obj)?;
    // 覆盖到现有文件内容上, 保留未知字段(如 todo_file)
    let mut merged = api_config_get(base);
    if let Some(dst) = merged.as_object_mut() {
        for (k, v) in obj {
            dst.insert(k.clone(), v.clone());
        }
    }
    let text = serde_json::to_string_pretty(&merged).map_err(|e| e.to_string())?;
    std::fs::write(base.join("config.json"), text).map_err(|e| e.to_string())?;
    Ok(json_ok())
}

/// POST /api/todo: 整份替换 todo.txt -> 触发刷新
fn api_todo_post(base: &std::path::Path, body: &[u8]) -> Result<serde_json::Value, String> {
    let text = std::str::from_utf8(body).map_err(|_| "待办必须是 UTF-8 文本".to_string())?;
    let file = crate::config::load(base).todo_file;
    std::fs::write(base.join(&file), text).map_err(|e| e.to_string())?;
    Ok(json_ok())
}

/// POST /api/preview: 按表单当前值拼模板 HTML(不保存配置、不截图、不转灰度),
/// 供 WebUI iframe 实时预览。与正式渲染同一套"数据注入 + 朝向替换"逻辑,
/// 待办/时钟用表单当前值回填, 其余数据走 60 秒缓存避免反复打外网。
fn api_preview(base: &std::path::Path, body: &[u8]) -> Result<String, String> {
    let new: serde_json::Value = serde_json::from_slice(body).map_err(|e| format!("JSON 解析失败: {e}"))?;
    let obj = new.as_object().ok_or("配置必须是 JSON 对象")?;
    validate_config(obj)?;
    let mut merged = api_config_get(base);
    if let Some(dst) = merged.as_object_mut() {
        for (k, v) in obj {
            dst.insert(k.clone(), v.clone());
        }
    }
    let cfg = crate::config::from_value(&merged);
    let template = if crate::render::ALLOWED_TEMPLATES.contains(&cfg.template.as_str()) {
        cfg.template.clone()
    } else {
        crate::render::ALLOWED_TEMPLATES[0].to_string()
    };
    let tpl_path = base.join(&template);
    let tpl = std::fs::read_to_string(&tpl_path)
        .map_err(|e| format!("模板读取失败 {}: {e}", tpl_path.display()))?;
    let cal = crate::lunar::Calendar::load(base).unwrap_or_else(crate::lunar::Calendar::empty);
    let mut data = crate::data::build_data_cached(base, &cfg, &cal);
    if let Some(todo) = obj.get("todo_content").and_then(|v| v.as_str()) {
        data["todo"] = serde_json::json!(todo.lines()
            .map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>());
    }
    data["time"] = serde_json::json!(chrono::Local::now().format("%H:%M").to_string());
    Ok(tpl.replace("/*__DATA__*/null", &serde_json::to_string(&data).unwrap_or_default())
        .replace("/*__ORIENTATION__*/portrait", &cfg.orientation))
}

pub fn start(out_dir: PathBuf, base: PathBuf, shared: Arc<Shared>) -> Result<(), String> {
    // 手动建 listener 以设 SO_REUSEADDR: 否则服务重启时会被旧连接的
    // TIME_WAIT 卡住约 30 秒~2 分钟(Python 版 allow_reuse_address 的同款作用)
    let addr: SocketAddr = ([0, 0, 0, 0], shared.port).into();
    let sock = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
        .map_err(|e| format!("端口 {} 监听失败: {e}", shared.port))?;
    #[cfg(unix)]
    sock.set_reuse_address(true)
        .map_err(|e| format!("端口 {} 监听失败: {e}", shared.port))?;
    sock.bind(&addr.into())
        .map_err(|e| format!("端口 {} 监听失败: {e}", shared.port))?;
    sock.listen(1024)
        .map_err(|e| format!("端口 {} 监听失败: {e}", shared.port))?;
    let listener: TcpListener = sock.into();
    let server = Arc::new(tiny_http::Server::from_listener(listener, None)
        .map_err(|e| format!("端口 {} 监听失败: {e}", shared.port))?);
    // 预览接口会同步抓数据(可达数秒), 单线程会把 /api/status 轮询和 Kindle 拉图一起卡住;
    // 开小线程池分散请求(tiny_http 的请求队列支持多消费者)
    for _ in 0..4 {
        let server = server.clone();
        let out_dir = out_dir.clone();
        let base = base.clone();
        let shared = shared.clone();
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                handle_request(request, &out_dir, &base, &shared);
            }
        });
    }
    Ok(())
}

/// tiny_http 的 respond 按值消费 request, 这里按值接收
fn handle_request(mut request: tiny_http::Request, out_dir: &std::path::Path,
                  base: &std::path::Path, shared: &Shared) {
    let method = request.method().to_string();
    let raw = request.url().split(['?', '#']).next().unwrap_or("/").to_string();
    let path = urlencoding::decode(&raw).map(|s| s.into_owned()).unwrap_or(raw);

    // ---- API ----
    match (method.as_str(), path.as_str()) {
        ("GET", "/api/status") => {
            let r = json(api_status(shared, base));
            let _ = request.respond(r);
        }
        ("GET", "/api/config") => {
            let r = json(api_config_get(base));
            let _ = request.respond(r);
        }
        ("POST", "/api/config") => {
            let r = read_body(&mut request, 256 * 1024)
                .map_err(|e| e.to_string())
                .and_then(|b| api_config_post(base, &b));
            let resp = match r {
                Ok(v) => json(v),
                Err(e) => json(serde_json::json!({"ok": false, "error": e}))
                    .with_status_code(400),
            };
            let _ = request.respond(resp);
        }
        ("POST", "/api/preview") => {
            let r = read_body(&mut request, 256 * 1024)
                .map_err(|e| e.to_string())
                .and_then(|b| api_preview(base, &b));
            let resp = match r {
                Ok(html) => tiny_http::Response::from_string(html)
                    .with_header(header("Content-Type", "text/html; charset=utf-8").unwrap()),
                Err(e) => json(serde_json::json!({"ok": false, "error": e}))
                    .with_status_code(400),
            };
            let _ = request.respond(resp);
        }
        ("GET", "/api/holidays") => {
            let cal = crate::lunar::Calendar::load(base)
                .unwrap_or_else(crate::lunar::Calendar::empty);
            let today = chrono::Local::now().date_naive();
            let days: Vec<serde_json::Value> = cal.fallback_holidays(today)
                .into_iter()
                .take(6)
                .map(|(d, name)| serde_json::json!({
                    "name": name,
                    "date": d.format("%Y-%m-%d").to_string(),
                    "days": (d - today).num_days(),
                }))
                .collect();
            let _ = request.respond(json(serde_json::json!({ "holidays": days })));
        }
        ("POST", "/api/refresh") => {
            trigger_refresh(shared);
            let _ = request.respond(json(json_ok()));
        }
        ("GET", "/api/templates") => {
            let templates: Vec<_> = crate::render::list_templates(base)
                .iter().map(|t| serde_json::to_value(t).unwrap()).collect();
            let _ = request.respond(json(serde_json::json!({ "templates": templates })));
        }
        ("GET", "/api/scan") => {
            let _ = request.respond(json(scan_kindle()));
        }
        ("GET", "/api/devices") => {
            let devices: Vec<_> = crate::render::DEVICES.iter().map(|d|
                serde_json::json!({"key": d.key, "name": d.name, "w": d.w, "h": d.h})
            ).collect();
            let _ = request.respond(json(serde_json::json!({ "devices": devices })));
        }
        ("GET", "/api/todo") => {
            let file = crate::config::load(base).todo_file;
            let content = std::fs::read_to_string(base.join(&file)).unwrap_or_default();
            let _ = request.respond(json(serde_json::json!({ "content": content })));
        }
        ("POST", "/api/todo") => {
            let r = read_body(&mut request, 256 * 1024)
                .map_err(|e| e.to_string())
                .and_then(|b| api_todo_post(base, &b));
            if r.is_ok() {
                trigger_refresh(shared);
            }
            let resp = match r {
                Ok(v) => json(v),
                Err(e) => json(serde_json::json!({"ok": false, "error": e}))
                    .with_status_code(400),
            };
            let _ = request.respond(resp);
        }
        _ => {
            serve_static(request, out_dir, base, &path);
        }
    }
}

fn serve_static(request: tiny_http::Request, out_dir: &std::path::Path,
                base: &std::path::Path, path: &str) {
    if path == "/" || path == "/index.html" {
        // webui: 磁盘优先(用户可改), 缺失用内置
        let html = std::fs::read_to_string(base.join("webui.html"))
            .unwrap_or_else(|_| include_str!("../resources/webui.html").to_string());
        let mut resp = tiny_http::Response::from_string(html)
            .with_header(header("Content-Type", "text/html; charset=utf-8").unwrap());
        if let Some(h) = header("Cache-Control", "no-store") {
            resp.add_header(h);
        }
        let _ = request.respond(resp);
    } else if path == "/webui" || path == "/webui.html" {
        let _ = request.respond(
            tiny_http::Response::empty(302).with_header(
                header("Location", "/").unwrap()));
    } else if path.contains("..") {
        let _ = request.respond(tiny_http::Response::from_string("not found".to_string()).with_status_code(404));
    } else {
        let file = out_dir.join(path.trim_start_matches(['/']));
        let ext_ok = file.extension()
            .and_then(|e| e.to_str())
            .map_or(false, |e| ALLOWED_EXT.contains(&e.to_lowercase().as_str()));
        let opened = if ext_ok { std::fs::File::open(&file) } else { Err(std::io::Error::new(std::io::ErrorKind::NotFound, "blocked")) };
        match opened {
            Ok(f) => {
                let mut resp = tiny_http::Response::from_file(f)
                    .with_header(header("Content-Type", mime_of(&file)).unwrap());
                if let Some(h) = header("Cache-Control", "no-store") {
                    resp.add_header(h);
                }
                let _ = request.respond(resp);
            }
            Err(_) => {
                let _ = request.respond(
                    tiny_http::Response::from_string("not found".to_string()).with_status_code(404));
            }
        }
    }
}
