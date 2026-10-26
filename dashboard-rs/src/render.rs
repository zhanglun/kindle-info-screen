//! 渲染: 数据注入 HTML 模板 -> 调用系统 Edge/Chrome 无头截图 -> 转灰度。
//! 截图策略与原 Python 版一致: 轮询 PNG 落盘尺寸稳定即认为写完, 进程不退则强杀。
use crate::config;
use crate::lunar::Calendar;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const W: u32 = 1072;
pub const H: u32 = 1448;

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

/// 朝向对应的截图尺寸(PW4 面板横竖互换)
pub fn dimensions(orientation: &str) -> (u32, u32) {
    if orientation == "landscape" { (H, W) } else { (W, H) }
}

/// 模板自描述信息(来自文件头部的 kindle-template 注释, 与 Python 版解析规则一致)
#[derive(serde::Serialize, Clone)]
pub struct TemplateInfo {
    pub file: String,
    pub name: String,
    pub orientations: Vec<String>,
}

/// 扫描数据目录下的 template*.html, 解析头部元数据; 无元数据的文件以文件名兜底, 仅竖版
pub fn list_templates(base: &Path) -> Vec<TemplateInfo> {
    const ALLOWED: &[&str] = &["template.html", "template_editorial.html", "template_terminal.html"];
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(base) else { return out };
    let mut files: Vec<_> = entries.flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| ALLOWED.contains(&n.as_str()))
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

fn shoot(browser: &Path, png: &Path, url: &str, out_dir: &Path, w: u32, h: u32) -> bool {
    let profile = out_dir.join("edge_profile");
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
    const ALLOWED: &[&str] = &["template.html", "template_editorial.html", "template_terminal.html"];
    let template = if ALLOWED.contains(&cfg.template.as_str()) { &cfg.template } else { "template.html" };
    let tpl_path = base.join(template);
    let tpl = std::fs::read_to_string(&tpl_path)
        .map_err(|e| format!("模板读取失败 {}: {e}", tpl_path.display()))?;
    let html = tpl.replace("/*__DATA__*/null", &serde_json::to_string(&data).unwrap_or_default())
        .replace("/*__ORIENTATION__*/portrait", &cfg.orientation);
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let rendered = out_dir.join("rendered.html");
    std::fs::write(&rendered, &html).map_err(|e| e.to_string())?;

    let browser = find_browser().ok_or("未找到 Edge/Chrome 浏览器(已尝试 Windows/macOS 常见安装路径)")?;
    let url = url::Url::from_file_path(&rendered)
        .map_err(|_| "rendered.html 路径转 URL 失败".to_string())?
        .to_string();
    let png = out_dir.join("dashboard.png");
    let (w, h) = dimensions(&cfg.orientation);
    if !shoot(&browser, &png, &url, out_dir, w, h) {
        return Err("截图失败".into());
    }
    // 尽量转灰度, 墨水屏显示更干净
    match image::open(&png) {
        Ok(img) => {
            if let Err(e) = img.to_luma8().save(&png) {
                crate::log_line(base, &format!("灰度保存失败: {e}"));
            }
        }
        Err(e) => crate::log_line(base, &format!("灰度转换跳过(PNG 解码失败: {e})")),
    }
    let size = std::fs::metadata(&png).map(|m| m.len()).unwrap_or(0);
    crate::log_line(base, &format!("rendered: dashboard.png {size} bytes"));
    Ok(chrono::Local::now().format("%H:%M").to_string())
}
