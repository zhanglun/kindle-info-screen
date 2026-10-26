//! 农历日期 / 节气 / 月相。
//! 农历与节气查 resources/calendar_table.json(由 gen_tables.py 用 lunar_python 预生成,
//! 覆盖公历 2024-02 ~ 2101-01), 月相按朔望月周期本地推算(与原 Python 版一致)。
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use std::collections::HashMap;

const MONTH_NAMES: [&str; 12] = [
    "正月", "二月", "三月", "四月", "五月", "六月",
    "七月", "八月", "九月", "十月", "冬月", "腊月",
];
const DAY_NAMES: [&str; 30] = [
    "初一", "初二", "初三", "初四", "初五", "初六", "初七", "初八", "初九", "初十",
    "十一", "十二", "十三", "十四", "十五", "十六", "十七", "十八", "十九", "二十",
    "廿一", "廿二", "廿三", "廿四", "廿五", "廿六", "廿七", "廿八", "廿九", "三十",
];

pub struct Calendar {
    /// 农历 2024 年正月初一对应的公历日期
    epoch: NaiveDate,
    /// 按时间顺序的 (每月天数, "农历年-月" 标识, 闰月带 L 后缀)
    months: Vec<(i64, String)>,
    /// 公历年 -> [(节气名, 精确到分钟的时刻)] 按时间顺序
    terms: HashMap<i32, Vec<(String, NaiveDateTime)>>,
}

#[derive(serde::Deserialize)]
struct RawTable {
    epoch: String,
    months: Vec<(i64, String)>,
    terms: HashMap<String, Vec<(String, String)>>,
}

impl Calendar {
    /// 空表: 所有查询返回 None(农历/节气静默缺席, 不影响渲染)
    pub fn empty() -> Calendar {
        Calendar { epoch: NaiveDate::from_ymd_opt(2024, 2, 10).unwrap(), months: Vec::new(), terms: HashMap::new() }
    }

    pub fn load(base: &std::path::Path) -> Option<Calendar> {
        let text = std::fs::read_to_string(base.join("calendar_table.json")).ok()?;
        let raw: RawTable = serde_json::from_str(&text).ok()?;
        let epoch = NaiveDate::parse_from_str(&raw.epoch, "%Y-%m-%d").ok()?;
        let mut terms = HashMap::new();
        for (year, list) in raw.terms {
            let mut v = Vec::with_capacity(list.len());
            for (name, mdt) in list {
                // "MM-DDTHH:MM" 补上年份解析
                let d = NaiveDateTime::parse_from_str(&format!("{year}-{mdt}"), "%Y-%m-%dT%H:%M").ok()?;
                v.push((name, d));
            }
            v.sort_by_key(|(_, d)| *d);
            terms.insert(year.parse::<i32>().ok()?, v);
        }
        Some(Calendar { epoch, months: raw.months, terms })
    }

    /// 农历 年-月-日 -> 公历(闰月用 label 后缀 L 存储顺序, 普通月先于闰月匹配)
    pub fn lunar_ymd(&self, y: i32, m: u32, d: u32) -> Option<NaiveDate> {
        let mut off = 0i64;
        for (len, label) in &self.months {
            let (ly, lm) = {
                let core = label.trim_end_matches('L');
                let mut it = core.split('-');
                (it.next()?, it.next()?)
            };
            if ly.parse::<i32>().ok()? == y && lm.parse::<u32>().ok()? == m {
                let start = self.epoch.checked_add_days(chrono::Days::new(off as u64))?;
                return start.checked_add_days(chrono::Days::new((d as u64).saturating_sub(1)));
            }
            off += len;
        }
        None
    }

    /// 法定节日推算(接口不可用时的兜底): 元旦/劳动节/国庆固定, 春节/端午/中秋由农历, 清明由节气表
    pub fn fallback_holidays(&self, today: NaiveDate) -> Vec<(NaiveDate, String)> {
        let mut v: Vec<(NaiveDate, &str)> = Vec::new();
        for y in [today.year(), today.year() + 1] {
            if let Some(d) = NaiveDate::from_ymd_opt(y, 1, 1) { v.push((d, "元旦")); }
            if let Some(d) = self.lunar_ymd(y, 1, 1) { v.push((d, "春节")); }
            if let Some(d) = self.lunar_ymd(y, 5, 5) { v.push((d, "端午")); }
            if let Some(d) = self.lunar_ymd(y, 8, 15) { v.push((d, "中秋")); }
            if let Some(d) = NaiveDate::from_ymd_opt(y, 5, 1) { v.push((d, "劳动节")); }
            if let Some(d) = NaiveDate::from_ymd_opt(y, 10, 1) { v.push((d, "国庆节")); }
            if let Some(ts) = self.terms.get(&y) {
                if let Some((_, dt)) = ts.iter().find(|(n, _)| n == "清明") {
                    v.push((dt.date(), "清明"));
                }
            }
        }
        v.into_iter()
            .filter(|(d, _)| *d >= today)
            .map(|(d, n)| (d, n.to_string()))
            .collect()
    }

    /// 农历日期字符串, 如 "九月十七" / "闰六月廿三"(与 lunar_python toString 的月日部分一致)
    pub fn lunar_str(&self, today: NaiveDate) -> Option<String> {
        let n = (today - self.epoch).num_days();
        if n < 0 {
            return None;
        }
        let mut off = 0i64;
        for (len, label) in &self.months {
            if n < off + len {
                let (_y, m) = label.trim_end_matches('L').split_once('-')?;
                let m: usize = m.parse().ok()?;
                let leap = label.ends_with('L');
                let name = MONTH_NAMES.get(m.checked_sub(1)?)?;
                let day = DAY_NAMES.get((n - off) as usize)?;
                return Some(format!("{}{}{}", if leap { "闰" } else { "" }, name, day));
            }
            off += len;
        }
        None
    }

    /// 最近一个已到节气(与 lunar_python 一致按时刻判定): (名称, 距今天数)。表外时间返回 None。
    pub fn prev_jieqi(&self, now: NaiveDateTime) -> Option<(String, i64)> {
        let today = now.date();
        let mut best: Option<(NaiveDateTime, &str)> = None;
        for year in [today.year() - 1, today.year()] {
            for (name, t) in self.terms.get(&year).into_iter().flatten() {
                if *t <= now && best.map_or(true, |(bt, _)| *t > bt) {
                    best = Some((*t, name));
                }
            }
        }
        best.map(|(t, name)| (name.to_string(), (today - t.date()).num_days()))
    }
}

/// 与 Python round() 一致: 四舍五入, 恰为 .5 时取偶(银行家舍入)
pub fn round_py(v: f64) -> i64 {
    if (v - v.trunc()).abs() == 0.5 {
        let t = v.trunc() as i64;
        if t % 2 == 0 { t } else { t + v.signum() as i64 }
    } else {
        v.round() as i64
    }
}

/// 月相(名称, 照亮百分比), 与原 Python 版同一套推算
pub fn moon_phase(today: NaiveDate) -> (String, i64) {
    const SYNODIC: f64 = 29.530588;
    let epoch = NaiveDate::from_ymd_opt(2000, 1, 6).unwrap();
    let age = (today - epoch).num_days() as f64 % SYNODIC;
    let phases = ["新月", "娥眉月", "上弦月", "盈凸月", "满月", "亏凸月", "下弦月", "残月"];
    let idx = ((age / SYNODIC) * 8.0 + 0.5) as usize % 8;
    let illum = round_py((1.0 - (2.0 * std::f64::consts::PI * age / SYNODIC).cos()) / 2.0 * 100.0);
    (phases[idx].to_string(), illum)
}

/// 与 dashboard.py 相同的节气展示规则: 当天显示名称, 15 天内显示 "X后N天"
pub fn jieqi_display(now: NaiveDateTime, cal: &Calendar) -> Option<String> {
    match cal.prev_jieqi(now) {
        Some((name, days)) if days == 0 => Some(name),
        Some((name, days)) if days < 15 => Some(format!("{name}后{days}天")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cal() -> Calendar {
        Calendar::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").as_path()).unwrap()
    }
    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn lunar_dates_match_lunar_python() {
        let cal = cal();
        assert_eq!(cal.lunar_str(d("2026-10-26")).unwrap(), "九月十七");
        assert_eq!(cal.lunar_str(d("2025-07-25")).unwrap(), "闰六月初一"); // 2025 闰六月首日
        assert_eq!(cal.lunar_str(d("2025-08-22")).unwrap(), "闰六月廿九");
        assert!(cal.lunar_str(d("2024-02-09")).is_none()); // 表起点(2024-02-10)之前
    }

    #[test]
    fn jieqi_matches_lunar_python() {
        let cal = cal();
        let dt = |s: &str| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        assert_eq!(cal.prev_jieqi(dt("2026-10-26 09:00")).unwrap(), ("霜降".into(), 3));
        assert_eq!(cal.prev_jieqi(dt("2025-07-25 08:00")).unwrap(), ("大暑".into(), 3));
        // 时刻级: 霜降时刻(17:37)前算寒露, 之后算霜降 —— 与 lunar_python 一致
        assert_eq!(cal.prev_jieqi(dt("2026-10-23 00:01")).unwrap(), ("寒露".into(), 15));
        assert_eq!(cal.prev_jieqi(dt("2026-10-23 23:59")).unwrap(), ("霜降".into(), 0));
        assert_eq!(cal.prev_jieqi(dt("2026-01-01 00:00")).unwrap(), ("冬至".into(), 11));
        // 展示规则: 当天显示名称, 超过 15 天不展示
        assert_eq!(jieqi_display(dt("2026-10-23 23:59"), &cal).unwrap(), "霜降");
        assert_eq!(jieqi_display(dt("2026-10-23 00:01"), &cal), None); // 寒露后15天
    }

    #[test]
    fn moon_phase_matches_python() {
        assert_eq!(moon_phase(d("2026-10-26")), ("满月".into(), 100));
    }
}

#[cfg(test)]
mod holiday_tests {
    use super::*;

    fn cal() -> Calendar {
        Calendar::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").as_path()).unwrap()
    }

    #[test]
    fn fallback_holidays_match_lunar_python() {
        let cal = cal();
        let all = {
            let mut v = cal.fallback_holidays(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
            v.sort();
            v
        };
        let get = |name: &str, year: i32| all.iter()
            .find(|(d, n)| *n == name && d.year() == year)
            .map(|(d, _)| d.to_string());
        // 基准值取自 lunar_python(Solar.fromYmd(...).getLunar())
        assert_eq!(get("春节", 2025).as_deref(), Some("2025-01-29"));
        assert_eq!(get("端午", 2025).as_deref(), Some("2025-05-31"));
        assert_eq!(get("中秋", 2025).as_deref(), Some("2025-10-06"));
        assert_eq!(get("春节", 2026).as_deref(), Some("2026-02-17"));
        assert_eq!(get("清明", 2026).as_deref(), Some("2026-04-05"));
        assert_eq!(get("元旦", 2026).as_deref(), Some("2026-01-01"));
    }
}
