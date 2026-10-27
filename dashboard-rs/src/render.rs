//! 渲染: 数据注入 HTML 模板 -> 截图 -> 转灰度。
//! 截图按平台分流: macOS 优先系统 WKWebView sidecar(零安装, build.rs 编译的
//! webview_shot, 同步等待退出); 缺失或失败回退 Edge/Chrome 无头截图。
//! Windows 主路径即 Edge(系统预装); 未来 WebView2 后端在此处加分支。
//! 浏览器路径沿用原 Python 版策略: 轮询 PNG 落盘尺寸稳定即认为写完, 进程不退则强杀。
use crate::config;
use crate::lunar::Calendar;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const W: u32 = 1072;
pub const H: u32 = 1448;

/// Kindle 型号 -> 屏幕分辨率(竖版), 横版互换。未识别的 device 回退 pw4。
pub struct DeviceInfo {
    pub key: &'static str,
    pub name: &'static str,
    pub w: u32,
    pub h: u32,
}

pub const DEVICES: &[DeviceInfo] = &[
    DeviceInfo { key: "basic", name: "基础款 2016/2019(第8/10代)", w: 600, h: 800 },
    DeviceInfo { key: "pw12", name: "Paperwhite 1·2(第5/6代)", w: 758, h: 1024 },
    DeviceInfo { key: "pw3", name: "Paperwhite 3(第7代)· Voyage", w: W, h: H },
    DeviceInfo { key: "pw4", name: "Paperwhite 4(第10代)", w: W, h: H },
    DeviceInfo { key: "basic22", name: "基础款 2022/2024(第11/12代)", w: W, h: H },
    DeviceInfo { key: "pw5", name: "Paperwhite 5(第11代)", w: 1236, h: 1648 },
    DeviceInfo { key: "oasis", name: "Oasis 2/3(第8/9代)", w: 1264, h: 1680 },
    DeviceInfo { key: "scribe", name: "Scribe(10.2吋)", w: 1860, h: 2480 },
];

pub fn device_size(device: &str) -> (u32, u32) {
    DEVICES.iter().find(|d| d.key == device)
        .map(|d| (d.w, d.h))
        .unwrap_or((W, H))
}

const BROWSER_CANDIDATES: &[&str] = &[
    // Windows
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    // macOS
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
];

fn find_browser() -> Option<PathBuf> {
    BROWSER_CANDIDATES.iter().map(PathBuf::from).find(|p| p.exists())
}

/// 朝向对应的截图尺寸(按所选 Kindle 型号, 横竖互换)
pub fn dimensions(orientation: &str, device: &str) -> (u32, u32) {
    let (w, h) = device_size(device);
    if orientation == "landscape" { (h, w) } else { (w, h) }
}

/// 模板自描述信息(来自文件头部的 kindle-template 注释, 与 Python 版解析规则一致)
#[derive(serde::Serialize, Clone)]
pub struct TemplateInfo {
    pub file: String,
    pub name: String,
    pub orientations: Vec<String>,
}

/// 实际启用的模板白名单(list_templates 与 render_once 以此为准);新模板两处都要记得加
pub const ALLOWED_TEMPLATES: &[&str] = &[
    // 画框(白底水墨月夜) / 相框夜画(黑底月相) / 诗笺(黑底竖排一言)
    "template_gallery.html", "template_frame.html", "template_poem.html",
    // 天穹(日月经天) / 晨报(白底报纸)
    "template_horizon.html", "template_morning.html",
];

/// 扫描数据目录下的 template*.html, 解析头部元数据; 无元数据的文件以文件名兜底, 仅竖版
pub fn list_templates(base: &Path) -> Vec<TemplateInfo> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(base) else { return out };
    let mut files: Vec<_> = entries.flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| ALLOWED_TEMPLATES.contains(&n.as_str()))
        .collect();
    files.sort();
    for file in files {
        let default = TemplateInfo {
            name: file.trim_start_matches("template").trim_end_matches(".html").to_string(),
            file: file.clone(),
            orientations: vec!["portrait".into()],
        };
        let Ok(text) = std::fs::read_to_string(base.join(&file)) else {
            out.push(default); continue;
        };
        let Some(marker) = text.find("kindle-template:") else { out.push(default); continue };
        let rest = &text[marker + "kindle-template:".len()..];
        let line = rest.lines().next().unwrap_or("");
        let pick = |key: &str| line.split_whitespace()
            .find_map(|tok| tok.strip_prefix(key)).map(|s| s.to_string());
        let name = pick("name=").unwrap_or(default.name);
        let orientations: Vec<String> = pick("orientations=")
            .map(|s| s.split(',').map(str::to_string).filter(|o| o == "portrait" || o == "landscape").collect())
            .filter(|v: &Vec<String>| !v.is_empty())
            .unwrap_or(default.orientations);
        out.push(TemplateInfo { file, name, orientations });
    }
    out
}

fn abs_path(p: &Path) -> PathBuf {
    // Edge 的 --screenshot/--user-data-dir 与 url 转换都依赖绝对路径
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().map(|cwd| cwd.join(p)).unwrap_or_else(|_| p.to_path_buf())
    }
}

/// macOS 渲染 sidecar: build.rs 用 swiftc 编译的 WKWebView 截图工具,
/// 与主程序同目录分发(.app 的 Contents/MacOS/ 或 target/<profile>/)。
#[cfg(target_os = "macos")]
fn sidecar_path() -> Option<PathBuf> {
    std::env::current_exe().ok()
        .and_then(|exe| exe.parent().map(|d| d.join("webview_shot")))
        .filter(|p| p.is_file())
}

/// WKWebView sidecar 截图: sidecar 自带超时且写完 PNG 即退出, 同步等待即可,
/// 不需要浏览器路径那套"轮询文件尺寸"的判断。
#[cfg(target_os = "macos")]
fn shoot_webview(sidecar: &Path, html: &Path, png: &Path, w: u32, h: u32) -> Result<(), String> {
    let _ = std::fs::remove_file(png);
    let out = Command::new(sidecar)
        .arg(html)
        .arg(png)
        .args([w.to_string(), h.to_string(), "20".to_string()])
        .output()
        .map_err(|e| format!("sidecar 启动失败: {e}"))?;
    if out.status.success() && std::fs::metadata(png).map(|m| m.len() > 1_000).unwrap_or(false) {
        Ok(())
    } else {
        Err(format!("sidecar 截图失败: {}",
            String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn shoot(browser: &Path, png: &Path, url: &str, out_dir: &Path, w: u32, h: u32) -> bool {
    let png_abs = abs_path(png);
    let png: &Path = &png_abs; // 后续均按引用使用
    let profile = abs_path(&out_dir.join("edge_profile"));
    let _ = std::fs::create_dir_all(&profile);
    for headless in ["--headless=new", "--headless"] {
        let _ = std::fs::remove_file(png);
        let spawned = Command::new(browser)
            .args([headless, "--disable-gpu", "--hide-scrollbars",
                   "--force-device-scale-factor=1", "--default-background-color=FFFFFFFF",
                   "--virtual-time-budget=4000"])
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(format!("--window-size={w},{h}"))
            .arg(format!("--screenshot={}", png.display()))
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = spawned {
            let mut last: i64 = -1;
            let deadline = Instant::now() + Duration::from_secs(90);
            while Instant::now() < deadline {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    break;
                }
                let size = std::fs::metadata(png).map(|m| m.len() as i64).unwrap_or(-1);
                if size > 10_000 && size == last {
                    break; // 尺寸连续两次不变, 视为写完
                }
                last = size;
                thread::sleep(Duration::from_millis(500));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Ok(m) = std::fs::metadata(png) {
            if m.len() > 10_000 {
                return true;
            }
        }
    }
    false
}

/// 渲染一帧到 out/dashboard.png, 返回完成时刻(HH:MM)
pub fn render_once(base: &Path, out_dir: &Path, cal: &Calendar) -> Result<String, String> {
    let cfg = config::load(base);
    let data = crate::data::build_data(base, &cfg, cal);
    let template = if ALLOWED_TEMPLATES.contains(&cfg.template.as_str()) {
        &cfg.template
    } else {
        ALLOWED_TEMPLATES[0]
    };
    let tpl_path = base.join(template);
    let tpl = std::fs::read_to_string(&tpl_path)
        .map_err(|e| format!("模板读取失败 {}: {e}", tpl_path.display()))?;
    let html = tpl.replace("/*__DATA__*/null", &serde_json::to_string(&data).unwrap_or_default())
        .replace("/*__ORIENTATION__*/portrait", &cfg.orientation);
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let rendered = out_dir.join("rendered.html");
    std::fs::write(&rendered, &html).map_err(|e| e.to_string())?;

    let png = out_dir.join("dashboard.png");
    let (w, h) = dimensions(&cfg.orientation, &cfg.device);

    // 截图分流: macOS 优先系统 WKWebView sidecar(零安装、无需 Edge/Chrome),
    // sidecar 缺失(未编译出来)或运行失败时回退浏览器路径; Windows 直接走浏览器。
    #[cfg(target_os = "macos")]
    let webview_done = match sidecar_path() {
        Some(sidecar) => match shoot_webview(&sidecar, &rendered, &png, w, h) {
            Ok(()) => true,
            Err(e) => {
                crate::log_line(base, &format!("WKWebView sidecar 失败, 回退浏览器: {e}"));
                false
            }
        },
        None => false,
    };
    #[cfg(not(target_os = "macos"))]
    let webview_done = false;

    if !webview_done {
        let browser = find_browser()
            .ok_or("未找到 Edge/Chrome 浏览器(已尝试 Windows/macOS 常见安装路径)")?;
        let url = url::Url::from_file_path(abs_path(&rendered))
            .map_err(|_| "rendered.html 路径转 URL 失败".to_string())?
            .to_string();
        if !shoot(&browser, &png, &url, out_dir, w, h) {
            return Err("截图失败".into());
        }
    }
    let engine = if webview_done { "wkwebview" } else { "browser" };
    // 尽量转灰度, 墨水屏显示更干净; 横版画面旋转 90° 后输出,
    // 让竖板设备全屏显示横版内容, 方向随配置 cw(顺时针)/ccw(逆时针)
    match image::open(&png) {
        Ok(img) => {
            let oriented = if cfg.orientation == "landscape" {
                let rgba = img.to_rgba8();
                let rotated = if cfg.rotate == "ccw" {
                    image::imageops::rotate270(&rgba)
                } else {
                    image::imageops::rotate90(&rgba)
                };
                image::DynamicImage::ImageRgba8(rotated)
            } else {
                img
            };
            if let Err(e) = oriented.to_luma8().save(&png) {
                crate::log_line(base, &format!("灰度保存失败: {e}"));
            }
        }
        Err(e) => crate::log_line(base, &format!("灰度转换跳过(PNG 解码失败: {e})")),
    }
    let size = std::fs::metadata(&png).map(|m| m.len()).unwrap_or(0);
    crate::log_line(base, &format!("rendered via {engine}: dashboard.png {size} bytes"));
    // 秒级时间戳: 前端靠 last_render 变化判断渲染完成, 只到分钟会在同分钟内连续
    // 两次渲染时误报"渲染超时"
    Ok(chrono::Local::now().format("%H:%M:%S").to_string())
}
