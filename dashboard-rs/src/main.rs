//! Kindle 信息屏桌面版: 托盘常驻 + 后台定时渲染 + HTTP 供 Kindle 拉图。
//! 启动即开始服务; 托盘菜单: 打开 Web 页 / 立刻刷新 / 退出。
//! 子命令: `serve` 无托盘纯服务(Ctrl+C 退出), `once` 只渲染一次, 用于脚本/终端场景。
// release 构建在 Windows 上不弹控制台黑窗(调试构建保留)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod config;
mod extras;
mod data;
mod lunar;
mod render;
mod server;

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::process::Command;
use std::time::{Duration, Instant};

/// 首次运行时提取到数据目录的内置资源(已存在则不覆盖, 用户改动得以保留)
const EMBEDDED_RESOURCES: &[(&str, &str)] = &[
    ("config.json", include_str!("../resources/config.json")),
    ("todo.txt", include_str!("../resources/todo.txt")),
    ("template.html", include_str!("../resources/template.html")),
    ("template_editorial.html", include_str!("../resources/template_editorial.html")),
    ("template_terminal.html", include_str!("../resources/template_terminal.html")),
    ("template_zen.html", include_str!("../resources/template_zen.html")),
    ("template_mono.html", include_str!("../resources/template_mono.html")),
    ("template_split.html", include_str!("../resources/template_split.html")),
    ("template_quad.html", include_str!("../resources/template_quad.html")),
    ("template_frame.html", include_str!("../resources/template_frame.html")),
    ("template_plaque.html", include_str!("../resources/template_plaque.html")),
    ("template_broadsheet.html", include_str!("../resources/template_broadsheet.html")),
    ("template_minimal.html", include_str!("../resources/template_minimal.html")),
    ("template_timetable.html", include_str!("../resources/template_timetable.html")),
    ("template_photo.html", include_str!("../resources/template_photo.html")),
    ("template_clock.html", include_str!("../resources/template_clock.html")),
    ("calendar_table.json", include_str!("../resources/calendar_table.json")),
    ("webui.html", include_str!("../resources/webui.html")),
];

/// 内置资源版本: 升级时刷新"程序文件"(模板/农历表/webui); config.json 与 todo.txt
/// 是用户数据, 仅在缺失时创建, 永不覆盖。改过内置模板的话升级后需要重新套用。
const RESOURCE_VERSION: &str = "10";

/// 数据目录: 环境变量 > .app 内嵌运行(macOS) > 可执行文件旁的 resources/
fn base_dir() -> PathBuf {
    if let Ok(p) = std::env::var("KINDLE_DASHBOARD_HOME") {
        let dir = PathBuf::from(p);
        let _ = std::fs::create_dir_all(&dir);
        return dir;
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(exe) = std::env::current_exe() {
            if exe.to_string_lossy().contains(".app/") {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                let dir = PathBuf::from(home)
                    .join("Library/Application Support/KindleDashboard");
                let _ = std::fs::create_dir_all(&dir);
                return dir;
            }
        }
    }
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("resources")))
        .unwrap_or_else(|| PathBuf::from("resources"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 同时写入 stderr 和 out/app.log, 方便无终端运行时排查
pub fn log_line(base: &std::path::Path, msg: &str) {
    eprintln!("{msg}");
    let log = base.join("out").join("app.log");
    if let Some(parent) = log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log) {
        let ts = chrono::Local::now().format("%m-%d %H:%M:%S");
        let _ = writeln!(f, "[{ts}] {msg}");
    }
}

/// 画一个"小报纸"样式的模板图标(模板模式: 不透明=墨色, 透明=镂空, 明暗菜单栏都清晰)
fn tray_image() -> image::RgbaImage {
    const S: u32 = 32;
    let mut img = image::RgbaImage::new(S, S);
    let ink = [20u8, 20, 20, 255];
    let clear = [0u8, 0, 0, 0];
    // 圆角矩形(x 5..26, y 3..28, r 6)内填墨色, 再镂空: 报头一行 + 三行正文
    let in_round = |x: i64, y: i64| -> bool {
        let (x0, y0, x1, y1, r) = (5i64, 3, 26, 28, 6);
        if x < x0 || x > x1 || y < y0 || y > y1 {
            return false;
        }
        let cx = x.clamp(x0 + r, x1 - r);
        let cy = y.clamp(y0 + r, y1 - r);
        (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r
    };
    let cut = |x: u32, y: u32| -> bool {
        let (x, y) = (x as i64, y as i64);
        // 报头横条 + 三行正文, 宽度错落
        (y >= 8 && y <= 10 && x >= 8 && x <= 23)
        || (y >= 14 && y <= 15 && x >= 8 && x <= 23)
        || (y >= 19 && y <= 20 && x >= 8 && x <= 18)
        || (y >= 24 && y <= 25 && x >= 8 && x <= 21)
    };
    for y in 0..S {
        for x in 0..S {
            let px = if in_round(x as i64, y as i64) && !cut(x, y) { ink } else { clear };
            img.put_pixel(x, y, image::Rgba(px));
        }
    }
    img
}

/// 致命启动错误: 无界面程序也得让用户知道为什么没起来
fn fatal_notify(msg: &str) {
    eprintln!("{msg}");
    #[cfg(target_os = "macos")]
    {
        let safe = msg.replace(['"', '\\'], "'");
        let _ = std::process::Command::new("osascript")
            .args(["-e", &format!("display notification \"{safe}\" with title \"Kindle 信息屏启动失败\"")])
            .status();
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let text: Vec<u16> = std::ffi::OsStr::new(msg).encode_wide().chain(std::iter::once(0)).collect();
        let caption: Vec<u16> = std::ffi::OsStr::new("Kindle 信息屏启动失败")
            .encode_wide().chain(std::iter::once(0)).collect();
        const MB_ICONERROR: u32 = 0x0000_0010;
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_ICONERROR);
        }
    }
}

/// release 构建是 windows_subsystem="windows"(无控制台): 从终端直接启动 serve/once 时
/// 标准句柄无效, 主动附着父控制台日志才可见; 若已被重定向(cargo run / > 文件)则句柄
/// 有效, 保持原样以便捕获。附着失败(双击启动等无控制台场景)即静默。
#[cfg(windows)]
fn attach_parent_console() {
    use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
        STD_OUTPUT_HANDLE,
    };
    unsafe {
        let cur = GetStdHandle(STD_ERROR_HANDLE);
        if !cur.is_null() && cur != INVALID_HANDLE_VALUE {
            return;
        }
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        let mut path: Vec<u16> = "CONOUT$".encode_utf16().collect();
        path.push(0);
        let conout = CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if conout != INVALID_HANDLE_VALUE {
            SetStdHandle(STD_OUTPUT_HANDLE, conout);
            SetStdHandle(STD_ERROR_HANDLE, conout);
        }
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}

fn main() {
    if std::env::var_os("KINDLE_DASHBOARD_RESTART").is_some() {
        std::thread::sleep(Duration::from_secs(1));
    }
    // 子命令: serve=无托盘纯服务(对齐原 Python 版 serve), once=只渲染一次; 无参数=托盘常驻
    let mode = std::env::args().nth(1).unwrap_or_default();
    if !mode.is_empty() && mode != "serve" && mode != "once" {
        eprintln!("未知参数: {mode} (可用: serve / once; 无参数 = 托盘常驻)");
        std::process::exit(2);
    }
    if !mode.is_empty() {
        attach_parent_console();
    }
    let base = base_dir();
    const USER_FILES: &[&str] = &["config.json", "todo.txt"];
    let version_file = base.join("resources.version");
    let upgrade = std::fs::read_to_string(&version_file)
        .map(|v| v.trim() != RESOURCE_VERSION)
        .unwrap_or(true);
    for (name, content) in EMBEDDED_RESOURCES {
        let p = base.join(name);
        let user = USER_FILES.contains(name);
        if !p.exists() {
            let _ = std::fs::write(&p, content);
            log_line(&base, &format!("提取默认资源 {name}"));
        } else if upgrade && !user && std::fs::read_to_string(&p).unwrap_or_default() != *content {
            let _ = std::fs::write(&p, content);
            log_line(&base, &format!("升级: 刷新内置资源 {name}"));
        }
    }
    if upgrade {
        let _ = std::fs::write(&version_file, RESOURCE_VERSION);
    }
    let out_dir = base.join("out");
    let _ = std::fs::create_dir_all(&out_dir);

    let cfg = config::load(&base);
    let cal = lunar::Calendar::load(&base).unwrap_or_else(|| {
        log_line(&base, "calendar_table.json 缺失或损坏, 农历/节气将不显示");
        lunar::Calendar::empty()
    });

    // once 模式: 渲染一次即退出, 改模板/配置后验证效果(对齐原 Python 版 dashboard.py once)
    if mode == "once" {
        match render::render_once(&base, &out_dir, &cal) {
            Ok(t) => log_line(&base, &format!("渲染完成 ({t}): {}", out_dir.join("dashboard.png").display())),
            Err(e) => {
                log_line(&base, &format!("render error: {e}"));
                std::process::exit(1);
            }
        }
        return;
    }

    // 渲染线程的唤醒通道(立刻刷新): 服务线程经 Shared 转发
    let (tx, rx) = mpsc::channel::<()>();
    let tx_tray = tx.clone(); // 托盘"立刻刷新"菜单也要发信号
    let shared = Arc::new(server::Shared {
        port: cfg.port,
        lan_ip: server::lan_ip_for(cfg.kindle_host.as_deref()),
        last_render: Mutex::new("--".into()),
        refresh: Mutex::new(tx),
    });

    // HTTP 服务线程(Kindle 拉图 + Web 配置管理)
    if let Err(e) = server::start(out_dir.clone(), base.clone(), shared.clone()) {
        log_line(&base, &e);
        if mode.is_empty() {
            fatal_notify(&e); // 常见原因: 另一个实例还占着 8787
        } else {
            std::process::exit(1);
        }
        return;
    }
    log_line(&base, &format!("服务已启动: http://0.0.0.0:{}/dashboard.png  每 {} 分钟刷新",
                             cfg.port, cfg.interval_min));
    log_line(&base, &format!("Kindle 拉取地址: http://{}:{}/dashboard.png", shared.lan_ip, cfg.port));
    if mode == "serve" {
        log_line(&base, &format!("配置管理页面:   http://127.0.0.1:{}/  (serve 模式: Ctrl+C 退出)", cfg.port));
    }

    // 渲染线程: 立即渲染一次, 之后按间隔; 收到信号提前唤醒(立刻刷新)
    let worker_base = base.clone();
    std::thread::spawn(move || loop {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            render::render_once(&worker_base, &worker_base.join("out"), &cal)
        }));
        match r {
            Ok(Ok(t)) => {
                *shared.last_render.lock().unwrap() = t;
                let push_cfg = config::load(&worker_base);
                if push_cfg.push_after_render {
                    start_push_worker(worker_base.clone(), push_cfg);
                }
            }
            Ok(Err(e)) => log_line(&worker_base, &format!("render error: {e}")),
            Err(_) => log_line(&worker_base, "render panic: 渲染线程已捕获内部崩溃, 下个周期重试"),
        }
        let interval = config::load(&worker_base).interval_min.max(1);
        loop {
            match rx.recv_timeout(Duration::from_secs(interval * 60)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });

    // serve 模式: 不进托盘事件循环, 主线程挂起等 Ctrl+C 结束
    if mode == "serve" {
        std::thread::park();
        return;
    }
    run_tray(cfg.port, tx_tray);
}


fn start_push_worker(base: PathBuf, cfg: config::Config) {
    static PUSH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let Some(host) = cfg.kindle_host.clone() else {
        log_line(&base, "SSH 推送跳过: config.json 未设置 kindle_host");
        return;
    };
    let key = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|p| p.join(".ssh/kindle_id_rsa"));
    let Some(key) = key else {
        log_line(&base, "SSH 推送跳过: 找不到 HOME");
        return;
    };
    if !key.exists() {
        log_line(&base, &format!("SSH 推送跳过: 私钥不存在 {}", key.display()));
        return;
    }
    std::thread::spawn(move || {
        let lock = PUSH_LOCK.get_or_init(|| Mutex::new(()));
        let Ok(_guard) = lock.try_lock() else {
            log_line(&base, "SSH 推送跳过: 已有一条重试任务在运行");
            return;
        };
        let deadline = Instant::now() + Duration::from_secs(cfg.push_retry_min.saturating_mul(60));
        loop {
            let result = Command::new("ssh")
                .arg("-p").arg(cfg.kindle_ssh_port.to_string())
                .arg("-i").arg(&key)
                .args(["-o", "IdentitiesOnly=yes", "-o", "BatchMode=yes",
                       "-o", "ConnectTimeout=5", "-o", "StrictHostKeyChecking=accept-new",
                       "-o", "PubkeyAcceptedAlgorithms=+ssh-rsa"])
                .arg(format!("{}@{}", cfg.kindle_user, host))
                .arg(&cfg.kindle_update_cmd)
                .output();
            if result.as_ref().map(|r| r.status.success()).unwrap_or(false) {
                log_line(&base, &format!("SSH 推送成功: {}:{}", host, cfg.kindle_ssh_port));
                return;
            }
            if Instant::now() >= deadline {
                log_line(&base, &format!("SSH 推送放弃: {}:{} 在 {} 分钟内不可达", host,
                    cfg.kindle_ssh_port, cfg.push_retry_min));
                return;
            }
            std::thread::sleep(Duration::from_secs(30));
        }
    });
}

fn run_tray(port: u16, refresh_tx: mpsc::Sender<()>) {
    use winit::application::ApplicationHandler;
    use winit::event_loop::{ActiveEventLoop, EventLoop};

    struct TrayEv(tray_icon::menu::MenuEvent);

    struct TrayApp {
        _tray: tray_icon::TrayIcon, // 持有即存活, drop 即移除托盘图标
        refresh_tx: mpsc::Sender<()>,
        url: String,
    }

    impl ApplicationHandler<TrayEv> for TrayApp {
        fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}
        fn window_event(&mut self, _e: &ActiveEventLoop, _id: winit::window::WindowId, _ev: winit::event::WindowEvent) {}
        fn user_event(&mut self, event_loop: &ActiveEventLoop, ev: TrayEv) {
            match ev.0.id().0.as_str() {
                "open" => {
                    let _ = webbrowser::open(&self.url);
                }
                "refresh" => {
                    let _ = self.refresh_tx.send(());
                }
                "restart" => {
                    if let Ok(exe) = std::env::current_exe() {
                        let _ = Command::new(exe)
                            .env("KINDLE_DASHBOARD_RESTART", "1")
                            .spawn();
                        event_loop.exit();
                    }
                }
                "quit" => event_loop.exit(),
                _ => {}
            }
        }
    }

    let mut builder = EventLoop::<TrayEv>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        // 无 Dock 图标、无主窗口, 只驻留菜单栏
        builder.with_activation_policy(ActivationPolicy::Accessory);
    }
    let event_loop = match builder.build() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("事件循环创建失败: {e}");
            return;
        }
    };

    let proxy = event_loop.create_proxy();
    tray_icon::menu::MenuEvent::set_event_handler(Some(move |ev| {
        let _ = proxy.send_event(TrayEv(ev));
    }));

    let open_item = tray_icon::menu::MenuItem::with_id("open", "在浏览器打开 Web 页", true, None);
    let refresh_item = tray_icon::menu::MenuItem::with_id("refresh", "立刻刷新 Dashboard", true, None);
    let restart_item = tray_icon::menu::MenuItem::with_id("restart", "重新启动", true, None);
    let quit_item = tray_icon::menu::MenuItem::with_id("quit", "退出", true, None);
    let menu = tray_icon::menu::Menu::new();
    let _ = menu.append_items(&[&open_item, &refresh_item, &restart_item, &quit_item]);

    let rgba = tray_image();
    let (iw, ih) = (rgba.width(), rgba.height());
    let icon = tray_icon::Icon::from_rgba(rgba.into_raw(), iw, ih)
        .expect("托盘图标 RGBA 数据合法");

    // 模板图标: macOS 上随明暗菜单栏自动反色(with_icon_templated 仅 macOS 提供)
    let mut builder = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Kindle 信息屏");
    #[cfg(target_os = "macos")]
    { builder = builder.with_icon_templated(icon); }
    #[cfg(not(target_os = "macos"))]
    { builder = builder.with_icon(icon); }
    let tray = builder.build().expect("创建托盘图标");

    let mut app = TrayApp { _tray: tray, refresh_tx, url: format!("http://127.0.0.1:{port}/") };
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("事件循环退出: {e}");
    }
}
