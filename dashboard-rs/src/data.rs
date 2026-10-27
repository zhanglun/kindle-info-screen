//! 数据抓取: 天气/空气质量/一言/RSS/待办/倒数日 -> 组装成模板需要的 JSON。
//! 各部分独立容错(与原 Python 版一致): 单项失败不影响整页渲染。
use crate::config::Config;
use crate::lunar::{jieqi_display, moon_phase, round_py, Calendar};
use chrono::{Datelike, Local, NaiveDate};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const WEEKDAY: [&str; 7] = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];

fn wmo_text(code: i64) -> &'static str {
    match code {
        0 => "晴", 1 => "晴间多云", 2 => "多云", 3 => "阴",
        45 => "雾", 48 => "雾凇",
        51 | 53 | 55 => "毛毛雨", 56 | 57 => "冻毛毛雨",
        61 => "小雨", 63 => "中雨", 65 => "大雨", 66 | 67 => "冻雨",
        71 => "小雪", 73 => "中雪", 75 => "大雪", 77 => "雪粒",
        80 | 81 => "阵雨", 82 => "强阵雨", 85 | 86 => "阵雪",
        95 => "雷暴", 96 | 99 => "雷暴冰雹",
        _ => "未知",
    }
}

/// macOS 系统代理(启动台/launchd 启动时无 shell 代理环境变量, 读系统设置兜底)
#[cfg(target_os = "macos")]
fn system_proxy() -> Option<String> {
    use std::process::Command;
    let out = Command::new("scutil").arg("--proxy").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let get = |k: &str| text.lines()
        .find(|l| l.trim_start().starts_with(k))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string());
    let (host, port) = if text.contains("HTTPSEnable : 1") {
        (get("HTTPSProxy")?, get("HTTPSPort")?)
    } else if text.contains("HTTPEnable : 1") {
        (get("HTTPProxy")?, get("HTTPPort")?)
    } else {
        return None;
    };
    if host.is_empty() || host == "0.0.0.0" {
        return None;
    }
    Some(format!("http://{host}:{port}"))
}

#[cfg(not(target_os = "macos"))]
fn system_proxy() -> Option<String> {
    None // Windows: 直接连接或设 HTTPS_PROXY 环境变量(reqwest 默认读取)
}

/// 直连 client(忽略环境与系统代理): 供 Bing 壁纸使用, 部分代理规则对 bing.com 反而超时
pub fn client_direct() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .user_agent("Mozilla/5.0 kindle-dashboard")
        .build()
        .expect("reqwest client")
}

pub fn client() -> reqwest::blocking::Client {
    // 原 Python 版关闭了证书校验, 这里保持一致(部分 RSS 源证书不全)
    let mut b = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .user_agent("Mozilla/5.0 kindle-dashboard");
    // reqwest 默认读 HTTPS_PROXY 等环境变量; 没有时(如 launchd 启动)回落到系统代理
    if std::env::var("HTTPS_PROXY").is_err()
        && std::env::var("https_proxy").is_err()
        && std::env::var("ALL_PROXY").is_err()
        && std::env::var("all_proxy").is_err()
    {
        if let Some(url) = system_proxy() {
            eprintln!("使用系统代理 {url}");
            b = b.proxy(reqwest::Proxy::all(&url).expect("系统代理 URL 合法"));
        }
    }
    b.build().expect("reqwest client")
}

/// 与 py "%d月%d日" 一致: 不补零
fn fmt_cn_date(month: u32, day: u32) -> String {
    format!("{month}月{day}日")
}

fn err_reason(e: &reqwest::Error) -> String {
    if e.is_timeout() { "超时".into() }
    else if e.is_connect() { "连接失败".into() }
    else if e.is_decode() { "解码失败".into() }
    else if let Some(s) = e.status() { format!("HTTP {}", s.as_u16()) }
    else { "网络错误".into() }
}

pub fn http_get(c: &reqwest::blocking::Client, url: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    c.get(url).timeout(timeout).send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| Ok(r.bytes()?.to_vec()))
        .map_err(|e| err_reason(&e))
}

// ---------- 天气 ----------
fn geocode(c: &reqwest::blocking::Client, base: &Path, city: &str)
           -> Result<(f64, f64, String), String> {
    let cache_path = base.join("city_cache.json");
    let mut cache: serde_json::Map<String, Value> = std::fs::read_to_string(&cache_path)
        .ok().and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if let Some(hit) = cache.get(city) {
        if let (Some(lat), Some(lon)) = (hit["lat"].as_f64(), hit["lon"].as_f64()) {
            return Ok((lat, lon, hit["name"].as_str().unwrap_or(city).to_string()));
        }
        // 缓存条目损坏则当作未命中, 重新查询并覆盖
    }
    let url = format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=zh&format=json",
                      urlencoding::encode(city));
    let body = http_get(c, &url, Duration::from_secs(20))?;
    let data: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let hit = data["results"][0].as_object().ok_or("找不到城市")?;
    let lat = hit.get("latitude").and_then(|v| v.as_f64()).ok_or("找不到城市")?;
    let lon = hit.get("longitude").and_then(|v| v.as_f64()).ok_or("找不到城市")?;
    let out = (lat, lon, hit["name"].as_str().unwrap_or(city).to_string());
    cache.insert(city.to_string(), json!({"lat": out.0, "lon": out.1, "name": out.2}));
    let _ = std::fs::write(&cache_path, serde_json::to_string(&Value::Object(cache)).unwrap_or_default());
    Ok(out)
}

fn fetch_weather(c: &reqwest::blocking::Client, base: &Path, city: &str) -> Result<Value, String> {
    let (lat, lon, name) = geocode(c, base, city)?;
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}\
         &current_weather=true&daily=weathercode,temperature_2m_max,temperature_2m_min,sunrise,sunset\
         &forecast_days=5&timezone=auto");
    let body = http_get(c, &url, Duration::from_secs(20))?;
    let d: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    // 必需字段缺失时整块失败(对齐 py: KeyError -> weather_error), 避免渲染出残缺天气
    let cur = d["current_weather"].as_object().ok_or("缺 current_weather")?;
    let daily = &d["daily"];
    let times = daily["time"].as_array().filter(|a| !a.is_empty()).ok_or("缺 daily.time")?;
    for key in ["weathercode", "temperature_2m_max", "temperature_2m_min"] {
        daily[key].as_array()
            .filter(|a| a.len() >= times.len())
            .ok_or_else(|| format!("缺 daily.{key}"))?;
    }
    let sunrise = daily["sunrise"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str())
        .ok_or("缺 daily.sunrise")?.chars().skip(11).take(5).collect::<String>();
    let sunset = daily["sunset"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str())
        .ok_or("缺 daily.sunset")?.chars().skip(11).take(5).collect::<String>();
    let temp = cur.get("temperature").and_then(|v| v.as_f64()).map(round_py).ok_or("缺 temperature")?;
    let wind = cur.get("windspeed").and_then(|v| v.as_f64()).map(round_py).ok_or("缺 windspeed")?;
    let wc_now = cur.get("weathercode").and_then(|v| v.as_i64()).unwrap_or(-1);

    let pick = |key: &str, i: usize| daily[key].as_array().and_then(|a| a.get(i)).cloned()
        .unwrap_or(Value::Null);
    let mut days = Vec::new();
    for (i, _) in times.iter().enumerate() {
        let date = daily["time"][i].as_str()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let label = match i {
            0 => "今天".to_string(),
            1 => "明天".to_string(),
            _ => date.map(|dt| WEEKDAY[dt.weekday().num_days_from_monday() as usize].to_string())
                .unwrap_or_default(),
        };
        let wc = pick("weathercode", i).as_i64().unwrap_or(-1);
        // py 里逐项 round(None) 会抛异常, 这里同样把非数值视为整块失败
        let hi = pick("temperature_2m_max", i).as_f64().map(round_py).ok_or("缺 daily.temperature_2m_max")?;
        let lo = pick("temperature_2m_min", i).as_f64().map(round_py).ok_or("缺 daily.temperature_2m_min")?;
        days.push(json!({
            "label": label,
            "code": wmo_text(wc),
            "wc": wc,
            "hi": hi,
            "lo": lo,
        }));
    }
    let mut w = json!({
        "city": name,
        "temp": temp,
        "code": wmo_text(wc_now),
        "wc": wc_now,
        "wind": wind,
        "days": days,
        "sunrise": sunrise,
        "sunset": sunset,
    });
    // 空气质量(独立容错)
    let aqi_url = format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={lat}&longitude={lon}\
         &current=pm2_5,us_aqi&timezone=auto");
    if let Ok(body) = http_get(c, &aqi_url, Duration::from_secs(20)) {
        if let Ok(a) = serde_json::from_slice::<Value>(&body) {
            let aqi = a["current"]["us_aqi"].as_f64();
            if let Some(aqi) = aqi {
                let grade = if aqi <= 50.0 { "优" } else if aqi <= 100.0 { "良" }
                    else if aqi <= 150.0 { "轻度污染" } else if aqi <= 200.0 { "中度污染" }
                    else if aqi <= 300.0 { "重度污染" } else { "严重污染" };
                w["aqi"] = json!(round_py(aqi));
                w["aqi_desc"] = json!(grade);
                if let Some(pm) = a["current"]["pm2_5"].as_f64() {
                    w["pm25"] = json!(round_py(pm));
                }
            }
        }
    }
    Ok(w)
}

fn fetch_yiyan(c: &reqwest::blocking::Client) -> Option<Value> {
    let body = http_get(c, "https://v1.hitokoto.cn/?c=d&c=i&c=k", Duration::from_secs(15)).ok()?;
    let d: Value = serde_json::from_slice(&body).ok()?;
    Some(json!({
        "text": d.get("hitokoto").and_then(|v| v.as_str()).unwrap_or(""),
        "from": if d.get("from").and_then(|v| v.as_str()).unwrap_or("").is_empty() { "佚名".into() }
                else { d["from"].as_str().unwrap_or("佚名").to_string() },
        "who": d.get("from_who").and_then(|v| v.as_str()).unwrap_or(""),
    }))
}

fn fetch_rss(c: &reqwest::blocking::Client, sources: &[crate::config::RssSource], limit: usize) -> Vec<Value> {
    let mut items = Vec::new();
    if sources.is_empty() {
        return items;
    }
    let per = std::cmp::max(1, limit / sources.len());
    for src in sources {
        match http_get(c, &src.url, Duration::from_secs(25))
            .and_then(|b| feed_rs::parser::parse(&b[..]).map_err(|_| "RSS格式错误".to_string())) {
            Ok(feed) => {
                for entry in feed.entries.into_iter().take(per) {
                    let title = entry.title
                        .map(|t| t.content)
                        .unwrap_or_default()
                        .replace("<![CDATA[", "").replace("]]>", "")
                        .trim()
                        .to_string();
                    if !title.is_empty() {
                        items.push(json!({"src": src.name, "title": title.chars().take(32).collect::<String>()}));
                    }
                }
            }
            Err(e) => {
                items.push(json!({"src": src.name, "title": format!("(源暂时不可用: {e})")}));
            }
        }
    }
    items.truncate(limit);
    items
}

fn read_todo(base: &Path, file: &str) -> Vec<String> {
    std::fs::read_to_string(base.join(file))
        .map(|t| t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

fn compute_countdowns(today: NaiveDate, entries: &[crate::config::Countdown]) -> Vec<Value> {
    // 距周末: 与 py 版 (5 - weekday) % 7 or 7 一致; 必须用欧几里得取余,
    // 截断取余在周日会得到 -1(取 abs 后错指向周一)
    let wd = today.weekday().num_days_from_monday() as i64;
    let ahead = (5 - wd).rem_euclid(7);
    let saturday = today + chrono::Duration::days(if ahead == 0 { 7 } else { ahead });
    let mut out = Vec::new();
    out.push(json!({"name": "周末", "days": (saturday - today).num_days(),
                    "date": saturday.format("%m-%d").to_string()}));
    for e in entries {
        if let Ok(d) = NaiveDate::parse_from_str(&e.date, "%Y-%m-%d") {
            out.push(json!({"name": e.name, "days": (d - today).num_days(),
                            "date": d.format("%m-%d").to_string()}));
        }
    }
    out.sort_by_key(|v| v["days"].as_i64().unwrap_or(i64::MAX));
    out
}

/// 城市经纬度(天气地理编码的本地缓存)
fn geo_of(base: &Path, city: &str) -> Option<(f64, f64)> {
    let cache: Value = std::fs::read_to_string(base.join("city_cache.json")).ok()
        .and_then(|t| serde_json::from_str(&t).ok())?;
    let hit = cache.get(city)?;
    Some((hit["lat"].as_f64()?, hit["lon"].as_f64()?))
}

/// 抓全量数据并组装为模板 JSON(结构 dashboard.py build_data 的输出一一对应)
pub fn build_data(base: &Path, cfg: &Config, cal: &Calendar) -> Value {
    let now = Local::now();
    let today = now.date_naive();
    let c = client();

    let mut data = json!({
        "date": fmt_cn_date(now.month(), now.day()),
        "weekday": WEEKDAY[now.weekday().num_days_from_monday() as usize],
        "time": now.format("%H:%M").to_string(),
        "year": now.year(),
        "todo": read_todo(base, &cfg.todo_file),
        "countdowns": Value::Null, // 下方合并节假日与自定义倒数日
        "weather": Value::Null,
        "rss": Vec::<Value>::new(),
    });
    if cfg.module_on("weather") {
        match fetch_weather(&c, base, &cfg.city) {
            Ok(w) => data["weather"] = w,
            Err(e) => {
                data["weather_error"] = json!(format!("天气获取失败({e})"));
            }
        }
    }
    if cfg.module_on("rss") {
        data["rss"] = Value::Array(fetch_rss(&c, &cfg.rss_sources, cfg.rss_count));
    }
    if cfg.module_on("yiyan") {
        if let Some(y) = fetch_yiyan(&c) {
            data["yiyan"] = y;
        }
    }
    if cfg.module_on("lunar") {
        if let Some(lunar) = cal.lunar_str(today) {
            data["lunar"] = json!(lunar);
        }
        if let Some(jq) = jieqi_display(now.naive_local(), cal) {
            data["jieqi"] = json!(jq);
        }
        let (moon_name, illum) = moon_phase(today);
        data["moon"] = json!({"name": moon_name, "illum": illum});
    }
    // 倒数日 = 周末 + 法定节假日(接口优先/农历兜底) + 自定义, 按剩余天数排序, 同名以接口为准
    if cfg.module_on("countdowns") {
        let mut custom = compute_countdowns(today, &cfg.countdowns);
        let mut holidays: Vec<Value> = cal.fallback_holidays(today)
            .into_iter()
            .take(4)
            .map(|(d, name)| json!({
                "name": name,
                "days": (d - today).num_days(),
                "date": d.format("%m-%d").to_string(),
                "holiday": true,
            }))
            .collect();
        // 自定义项与法定节日重名时以接口为准(如手填过的 元旦/春节)
        let hnames: std::collections::HashSet<String> = holidays.iter()
            .filter_map(|v| v["name"].as_str()).map(String::from).collect();
        custom.retain(|v| v["name"].as_str().map_or(true, |n| !hnames.contains(n)));
        holidays.extend(custom);
        holidays.sort_by_key(|v| v["days"].as_i64().unwrap_or(i64::MAX));
        data["countdowns"] = Value::Array(holidays);
    } else {
        data["countdowns"] = Value::Array(Vec::new());
    }
    if !cfg.schedule_urls.is_empty() {
        data["schedule"] = Value::Array(crate::extras::fetch_schedule(&c, &cfg.schedule_urls));
    }
    if !cfg.hotlist_sources.is_empty() {
        data["hotlist"] = Value::Array(crate::extras::fetch_hotlist(&c, &cfg.hotlist_sources));
    }
    if !cfg.crypto.is_empty() {
        match crate::extras::fetch_crypto(&c, &cfg.crypto) {
            Ok(v) => data["crypto"] = v,
            Err(e) => {
                crate::log_line(base, &format!("行情获取失败: {e}"));
                data["crypto_error"] = json!(format!("行情获取失败({e})"));
            }
        }
    }
    data["idiom"] = crate::idioms::idiom_of_day(base);
    // 天穹: 太阳/月亮的地平坐标(城市经纬度来自天气地理编码缓存)
    if let Some((lat, lon)) = geo_of(base, &cfg.city) {
        let now = Local::now().naive_local();
        let (sun_alt, sun_az) = crate::astro::sun_alt_az(lat, lon, &now);
        let (moon_alt, moon_az) = crate::astro::moon_alt_az(lat, lon, &now);
        data["sky"] = json!({
            "sun_alt": (sun_alt * 10.0).round() / 10.0,
            "sun_az": (sun_az * 10.0).round() / 10.0,
            "moon_alt": (moon_alt * 10.0).round() / 10.0,
            "moon_az": (moon_az * 10.0).round() / 10.0,
        });
    }
    if cfg.frame_mode == "wallpaper" {
        let out_dir = base.join("out");
        let (w, h) = crate::render::dimensions(&cfg.orientation, &cfg.device);
        match crate::extras::fetch_wallpaper(&out_dir, w, h) {
            Ok(()) => data["wallpaper"] = json!({"file": "wallpaper.png", "credit": "Bing"}),
            Err(e) => crate::log_line(base, &format!("壁纸获取失败: {e}")),
        }
    }
    data
}

/// 预览专用: build_data 带 60 秒 TTL 缓存。WebUI 实时预览每次改字段都会请求,
/// 不缓存的话天气/RSS/一言/热榜会跟着反复打外网。key 覆盖影响数据组装的全部配置项,
/// todo 与 time 在服务端取表单/当前值回填, 不参与缓存键。
pub fn build_data_cached(base: &Path, cfg: &Config, cal: &Calendar) -> Value {
    // 5 分钟: 预览频繁触发, 天气/RSS 这类数据旧几分钟没有感知差异,
    // 换来的是绝大多数预览请求秒回(抓全套数据最慢 20+ 秒)
    static CACHE: OnceLock<Mutex<(String, Instant, Value)>> = OnceLock::new();
    let key = json!({
        "city": cfg.city,
        "modules": cfg.modules,
        "rss": cfg.rss_sources.iter().map(|s| (&s.name, &s.url)).collect::<Vec<_>>(),
        "rss_count": cfg.rss_count,
        "cd": cfg.countdowns.iter().map(|c| (&c.name, &c.date)).collect::<Vec<_>>(),
        "hot": cfg.hotlist_sources,
        "sched": cfg.schedule_urls,
        "wp": cfg.frame_mode,
        "crypto": cfg.crypto,
        "orient": cfg.orientation,
        "device": cfg.device,
        "todo_file": cfg.todo_file,
    }).to_string();
    let mut slot = CACHE.get_or_init(|| {
        Mutex::new((String::new(), Instant::now() - Duration::from_secs(300), Value::Null))
    }).lock().unwrap();
    if slot.0 == key && slot.1.elapsed() < Duration::from_secs(300) {
        return slot.2.clone();
    }
    let data = build_data(base, cfg, cal);
    // 失败结果(带 weather_error)只缓存 45 秒: 临时网络抖动不该在预览里停留 5 分钟
    let failed = data.get("weather_error").is_some();
    let start = if failed {
        Instant::now() - Duration::from_secs(300 - 45)
    } else {
        Instant::now()
    };
    *slot = (key, start, data.clone());
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn weekend(date: &str) -> (i64, String) {
        let today = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
        let first = compute_countdowns(today, &[]).remove(0);
        (first["days"].as_i64().unwrap(), first["date"].as_str().unwrap().to_string())
    }

    #[test]
    fn weekend_days_match_python() {
        // 2026-10-26 是周一, 10-31 周六, 11-01 周日
        assert_eq!(weekend("2026-10-26"), (5, "10-31".into())); // 周一
        assert_eq!(weekend("2026-10-30"), (1, "10-31".into())); // 周五
        assert_eq!(weekend("2026-10-31"), (7, "11-07".into())); // 周六 -> 下周六
        assert_eq!(weekend("2026-11-01"), (6, "11-07".into())); // 周日(回归: 截断取余曾错算成 1)
        assert_eq!(weekend("2026-11-02"), (5, "11-07".into())); // 周一
    }

    #[test]
    fn date_format_matches_python_no_padding() {
        // py "%d月%d日" % (3,5) == "3月5日"(%d 不补零)
        assert_eq!(fmt_cn_date(3, 5), "3月5日");
        assert_eq!(fmt_cn_date(10, 26), "10月26日");
        assert_eq!(fmt_cn_date(1, 1), "1月1日");
    }

    #[test]
    fn round_is_bankers_like_python() {
        assert_eq!(round_py(21.5), 22);
        assert_eq!(round_py(22.5), 22); // .5 取偶
        assert_eq!(round_py(23.5), 24);
        assert_eq!(round_py(-21.5), -22);
        assert_eq!(round_py(-0.5), 0);
        assert_eq!(round_py(2.4), 2);
        assert_eq!(round_py(2.6), 3);
    }
}
