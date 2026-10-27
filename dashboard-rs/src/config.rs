//! 配置读取: 字段级容错 —— 单个字段缺失/类型错误只回退该字段的默认值并记一次日志,
//! 不再整份配置失效(比 py 的逐字段 .get() 更宽松, 任何手误都不影响其余配置生效)。
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug)]
pub struct RssSource {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct Countdown {
    pub name: String,
    pub date: String,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub city: String,
    pub template: String,
    /// portrait | landscape
    pub orientation: String,
    /// Kindle 型号(render::DEVICES 的 key), 决定渲染分辨率
    pub device: String,
    pub interval_min: u64,
    pub port: u16,
    pub rss_sources: Vec<RssSource>,
    pub countdowns: Vec<Countdown>,
    pub rss_count: usize,
    pub todo_file: String,
    /// 模块开关(缺省 true): weather/rss/yiyan/lunar/countdowns
    pub modules: serde_json::Value,
    /// 日程 ics 订阅链接, 非空即启用
    pub schedule_urls: Vec<String>,
    /// 热榜源: github / weibo / bilibili, 非空即启用
    pub hotlist_sources: Vec<String>,
    /// 相框夜画画面: moon=月相 | wallpaper=Bing 每日一图
    pub frame_mode: String,
    /// 横版输出旋转: cw=顺时针(默认) | ccw=逆时针, 竖板设备全屏显示横版画面
    pub rotate: String,
    /// 行情币种(CoinGecko id, 最多 5 个), 空则不显示行情
    pub crypto: Vec<String>,
    /// KOReader SSH 推送配置
    pub kindle_host: Option<String>,
    pub kindle_user: String,
    pub kindle_ssh_port: u16,
    pub kindle_update_cmd: String,
    pub push_after_render: bool,
    pub push_retry_min: u64,
}

impl Config {
    pub fn module_on(&self, key: &str) -> bool {
        self.modules.get(key).and_then(|v| v.as_bool()).unwrap_or(true)
    }
}

/// 各字段默认值(与 resources/config.json 保持一致)
fn defaults() -> Config {
    Config {
        city: "杭州".into(),
        template: "template.html".into(),
        orientation: "portrait".into(),
        device: "pw4".into(),
        interval_min: 10,
        port: 8787,
        rss_sources: vec![],
        countdowns: vec![],
        rss_count: 6,
        todo_file: "todo.txt".into(),
        modules: serde_json::json!({}),
        schedule_urls: vec![],
        hotlist_sources: vec![],
        frame_mode: "wallpaper".into(),
        rotate: "cw".into(),
        crypto: vec![
            "bitcoin".into(), "ethereum".into(), "binancecoin".into(),
            "solana".into(), "ripple".into(),
        ],
        kindle_host: None,
        kindle_user: "root".into(),
        kindle_ssh_port: 2222,
        kindle_update_cmd: "sh /mnt/us/extensions/onlinescreensaver/bin/update.sh".into(),
        push_after_render: false,
        push_retry_min: 10,
    }
}

fn str_vec(v: &Value, key: &str) -> Vec<String> {
    match v.get(key).and_then(|x| x.as_array()) {
        Some(a) => a.iter().filter_map(|e| e.as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from)).collect(),
        None => {
            warn_once(format!("字段 {key} 缺失或非数组, 使用默认值"));
            vec![]
        }
    }
}

/// 同一问题只报一次, 避免每个刷新周期刷屏
fn warn_once(msg: String) {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    if seen.lock().unwrap().insert(msg.clone()) {
        eprintln!("config: {msg}");
    }
}

fn str_field(v: &Value, key: &str, default: &str) -> String {
    match v.get(key).and_then(|x| x.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => {
            warn_once(format!("字段 {key} 缺失或非字符串, 使用默认值"));
            default.to_string()
        }
    }
}

fn num_field(v: &Value, key: &str, default: u64) -> u64 {
    match v.get(key).and_then(|x| x.as_u64()) {
        Some(n) => n,
        None => {
            warn_once(format!("字段 {key} 缺失或非整数, 使用默认值"));
            default
        }
    }
}

fn rss_sources(v: &Value) -> Vec<RssSource> {
    let arr = match v.get("rss_sources").and_then(|x| x.as_array()) {
        Some(a) => a,
        None => {
            warn_once("字段 rss_sources 缺失或非数组, 使用默认值".into());
            return vec![];
        }
    };
    let out: Vec<RssSource> = arr.iter().filter_map(|e| {
        let name = e.get("name").and_then(|x| x.as_str())?.trim();
        let url = e.get("url").and_then(|x| x.as_str())?.trim();
        (!name.is_empty() && !url.is_empty())
            .then(|| RssSource { name: name.into(), url: url.into() })
    }).collect();
    if out.len() < arr.len() {
        warn_once(format!("rss_sources 有 {} 条缺 name/url 被跳过", arr.len() - out.len()));
    }
    out
}

fn countdowns(v: &Value) -> Vec<Countdown> {
    let arr = match v.get("countdowns").and_then(|x| x.as_array()) {
        Some(a) => a,
        None => {
            warn_once("字段 countdowns 缺失或非数组, 使用默认值".into());
            return vec![];
        }
    };
    let out: Vec<Countdown> = arr.iter().filter_map(|e| {
        let name = e.get("name").and_then(|x| x.as_str())?.trim();
        let date = e.get("date").and_then(|x| x.as_str())?.trim();
        (!name.is_empty() && !date.is_empty())
            .then(|| Countdown { name: name.into(), date: date.into() })
    }).collect();
    if out.len() < arr.len() {
        warn_once(format!("countdowns 有 {} 条缺 name/date 被跳过", arr.len() - out.len()));
    }
    out
}

pub fn from_value(v: &Value) -> Config {
    let d = defaults();
    let port = num_field(v, "port", d.port as u64);
    Config {
        city: str_field(v, "city", &d.city),
        template: str_field(v, "template", &d.template),
        orientation: {
            let o = str_field(v, "orientation", &d.orientation);
            if o == "landscape" { o } else { "portrait".into() }
        },
        // device 缺失属旧配置常态, 静默回默认; 未识别的 key 由 render::device_size 兜底
        device: v.get("device").and_then(|x| x.as_str())
            .map(str::trim).filter(|s| !s.is_empty())
            .unwrap_or("pw4").to_string(),
        interval_min: num_field(v, "interval_min", d.interval_min),
        port: if (1..=65535).contains(&port) { port as u16 } else {
            warn_once(format!("字段 port={port} 超出范围, 使用默认值"));
            d.port
        },
        rss_sources: rss_sources(v),
        countdowns: countdowns(v),
        rss_count: num_field(v, "rss_count", d.rss_count as u64) as usize,
        todo_file: str_field(v, "todo_file", &d.todo_file),
        modules: v.get("modules").cloned().unwrap_or_else(|| serde_json::json!({})),
        schedule_urls: str_vec(v, "schedule_urls"),
        hotlist_sources: str_vec(v, "hotlist_sources"),
        // frame_mode 缺失时按旧 wallpaper 开关推断, 老配置语义不变
        frame_mode: match v.get("frame_mode").and_then(|x| x.as_str()) {
            Some("moon") => "moon".into(),
            Some(_) => "wallpaper".into(),
            None if v.get("wallpaper").and_then(|x| x.as_bool()).unwrap_or(false) => "wallpaper".into(),
            None => "moon".into(),
        },
        // rotate 缺失/写错静默回顺时针
        rotate: match v.get("rotate").and_then(|x| x.as_str()) {
            Some("ccw") => "ccw".into(),
            _ => "cw".into(),
        },
        crypto: v.get("crypto").and_then(|x| x.as_array()).map(|a| {
            a.iter().filter_map(|e| e.as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from))
                .take(5).collect()
        }).unwrap_or_else(|| defaults().crypto),
        kindle_host: v.get("kindle_host").and_then(|x| x.as_str())
            .map(str::trim).filter(|s| !s.is_empty()).map(String::from),
        kindle_user: str_field(v, "kindle_user", &d.kindle_user),
        kindle_ssh_port: {
            let p = num_field(v, "kindle_ssh_port", d.kindle_ssh_port as u64);
            if (1..=65535).contains(&p) { p as u16 } else { d.kindle_ssh_port }
        },
        kindle_update_cmd: str_field(v, "kindle_update_cmd", &d.kindle_update_cmd),
        push_after_render: v.get("push_after_render").and_then(|x| x.as_bool()).unwrap_or(false),
        push_retry_min: num_field(v, "push_retry_min", d.push_retry_min),
    }
}

/// 整份文件读不到/不是 JSON 时才整体回退; 字段级问题已在 from_value 内单独兜底
pub fn load(base: &Path) -> Config {
    match std::fs::read_to_string(base.join("config.json")) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(v) => from_value(&v),
            Err(e) => {
                warn_once(format!("config.json 解析失败({e}), 整份使用默认配置"));
                defaults()
            }
        },
        Err(e) => {
            warn_once(format!("config.json 读取失败({e}), 整份使用默认配置"));
            defaults()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_fields_fall_back_individually() {
        let v: Value = serde_json::from_str(r#"{
            "city": "北京",
            "port": "not-a-number",
            "interval_min": true,
            "rss_sources": [{"name": "好源", "url": "https://x.com/rss"}, {"name": ""}],
            "countdowns": [{"name": "元旦", "date": "2027-01-01"}, {"date": "坏条目"}]
        }"#).unwrap();
        let c = from_value(&v);
        assert_eq!(c.city, "北京");                    // 好字段保留
        assert_eq!(c.port, 8787);                      // 坏端口回退默认
        assert_eq!(c.interval_min, 10);
        assert_eq!(c.template, "template.html");       // 缺失字段回退默认
        assert_eq!(c.rss_sources.len(), 1);            // 坏条目跳过
        assert_eq!(c.countdowns.len(), 1);
        assert_eq!(c.rss_count, 6);
    }
}
