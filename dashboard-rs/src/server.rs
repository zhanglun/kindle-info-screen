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

/// POST /api/config: 校验 -> 合并保存 -> 触发刷新
fn api_config_post(base: &std::path::Path, body: &[u8]) -> Result<serde_json::Value, String> {
    let new: serde_json::Value = serde_json::from_slice(body).map_err(|e| format!("JSON 解析失败: {e}"))?;
    let obj = new.as_object().ok_or("配置必须是 JSON 对象")?;
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
    let server = tiny_http::Server::from_listener(listener, None)
        .map_err(|e| format!("端口 {} 监听失败: {e}", shared.port))?;
    std::thread::spawn(move || {
        for mut request in server.incoming_requests() {
            let method = request.method().to_string();
            let raw = request.url().split(['?', '#']).next().unwrap_or("/").to_string();
            let path = urlencoding::decode(&raw).map(|s| s.into_owned()).unwrap_or(raw);

            // ---- API ----
            match (method.as_str(), path.as_str()) {
                ("GET", "/api/status") => {
                    let r = json(api_status(&shared, &base));
                    let _ = request.respond(r);
                    continue;
                }
                ("GET", "/api/config") => {
                    let r = json(api_config_get(&base));
                    let _ = request.respond(r);
                    continue;
                }
                ("POST", "/api/config") => {
                    let r = read_body(&mut request, 256 * 1024)
                        .map_err(|e| e.to_string())
                        .and_then(|b| api_config_post(&base, &b));
                    let resp = match r {
                        Ok(v) => json(v),
                        Err(e) => json(serde_json::json!({"ok": false, "error": e}))
                            .with_status_code(400),
                    };
                    let _ = request.respond(resp);
                    continue;
                }
                ("GET", "/api/holidays") => {
                    let cal = crate::lunar::Calendar::load(&base)
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
                    continue;
                }
                ("POST", "/api/refresh") => {
                    trigger_refresh(&shared);
                    let _ = request.respond(json(json_ok()));
                    continue;
                }
                ("GET", "/api/templates") => {
                    let templates: Vec<_> = crate::render::list_templates(&base)
                        .iter().map(|t| serde_json::to_value(t).unwrap()).collect();
                    let _ = request.respond(json(serde_json::json!({ "templates": templates })));
                    continue;
                }
                ("GET", "/api/todo") => {
                    let file = crate::config::load(&base).todo_file;
                    let content = std::fs::read_to_string(base.join(&file)).unwrap_or_default();
                    let _ = request.respond(json(serde_json::json!({ "content": content })));
                    continue;
                }
                ("POST", "/api/todo") => {
                    let r = read_body(&mut request, 256 * 1024)
                        .map_err(|e| e.to_string())
                        .and_then(|b| api_todo_post(&base, &b));
                    if r.is_ok() {
                        trigger_refresh(&shared);
                    }
                    let resp = match r {
                        Ok(v) => json(v),
                        Err(e) => json(serde_json::json!({"ok": false, "error": e}))
                            .with_status_code(400),
                    };
                    let _ = request.respond(resp);
                    continue;
                }
                _ => {}
            }

            // ---- 静态资源 ----
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
    });
    Ok(())
}
