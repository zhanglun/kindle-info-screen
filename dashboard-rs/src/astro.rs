//! 天文计算: 太阳与月亮的地平坐标(高度角/方位角)。
//! 低精度算法(Meeus), 太阳误差 <0.5°, 月亮 1~2°, 对版面摆放绰绰有余。
//! 约定: 所有角度变量一律为"度"; 三角用 sin_deg/cos_deg; deg360 只做归一。
use chrono::{Datelike, Local, NaiveDateTime, Offset, Timelike};

fn sin_deg(x: f64) -> f64 { x.to_radians().sin() }
fn cos_deg(x: f64) -> f64 { x.to_radians().cos() }
fn deg360(x: f64) -> f64 { x.rem_euclid(360.0) }

/// 本地时间 -> UTC(天文公式一律用世界时)
fn to_utc(dt: &NaiveDateTime) -> NaiveDateTime {
    *dt - Local::now().offset().fix()
}

/// 儒略日(自 -4712-01-01 12:00 UT 的天数)
fn julian_day(dt: &NaiveDateTime) -> f64 {
    let (y, m) = if dt.month() <= 2 {
        (dt.year() - 1, dt.month() + 12)
    } else {
        (dt.year(), dt.month())
    };
    let d = dt.day() as f64;
    let h = dt.hour() as f64 + dt.minute() as f64 / 60.0 + dt.second() as f64 / 3600.0;
    let a = y / 100;
    let b = 2 - a + a / 4;
    (365.25 * (y + 4716) as f64).floor() + (30.6001 * (m + 1) as f64).floor()
        + d + b as f64 - 1524.5 + h / 24.0
}

/// 格林尼治平恒星时(度)
fn gmst_deg(dt: &NaiveDateTime) -> f64 {
    let jd = julian_day(dt);
    let t = (jd - 2451545.0) / 36525.0;
    deg360(280.46061837 + 360.98564736629 * (jd - 2451545.0)
        + 0.000387933 * t * t - t * t * t / 38710000.0)
}

/// 赤道坐标 -> 地平坐标: 返回 (高度角, 方位角自南起算西正)
fn equatorial_to_horizontal(
    lat: f64, lon: f64, dt: &NaiveDateTime, ra: f64, dec: f64,
) -> (f64, f64) {
    let ha = deg360(gmst_deg(dt) + lon - ra);
    let alt = (sin_deg(lat) * sin_deg(dec) + cos_deg(lat) * cos_deg(dec) * cos_deg(ha))
        .asin().to_degrees();
    let az = deg360((-sin_deg(ha) * cos_deg(dec))
        .atan2(sin_deg(dec) * cos_deg(lat) - cos_deg(dec) * sin_deg(lat) * cos_deg(ha))
        .to_degrees() + 180.0);
    (alt, az)
}

/// 太阳高度角与方位角(度)。lat 北纬为正, lon 东经为正。误差 < 0.5°。
pub fn sun_alt_az(lat: f64, lon: f64, dt: &NaiveDateTime) -> (f64, f64) {
    let utc = to_utc(dt);
    let n = julian_day(&utc) - 2451545.0;
    let l = deg360(280.460 + 0.9856474 * n);            // 平黄经
    let g = deg360(357.528 + 0.9856003 * n);            // 平近点角
    let lambda = l + 1.915 * sin_deg(g) + 0.020 * sin_deg(2.0 * g); // 视黄经
    let eps = 23.439 - 0.0000004 * n;                    // 黄赤交角
    let dec = (sin_deg(eps) * sin_deg(lambda)).asin().to_degrees();
    let ra = (cos_deg(eps) * sin_deg(lambda)).atan2(cos_deg(lambda)).to_degrees();
    equatorial_to_horizontal(lat, lon, &utc, deg360(ra), dec)
}

/// 月亮高度角与方位角(度)。低精度, 误差 1~2°。
pub fn moon_alt_az(lat: f64, lon: f64, dt: &NaiveDateTime) -> (f64, f64) {
    let utc = to_utc(dt);
    let t = (julian_day(&utc) - 2451545.0) / 36525.0;
    let lp = deg360(218.316 + 481267.881 * t
        + 6.29 * sin_deg(134.9 + 477198.87 * t)
        - 1.27 * sin_deg(259.2 - 413335.38 * t)
        + 0.66 * sin_deg(235.7 + 890534.23 * t)
        + 0.21 * sin_deg(269.9 + 954397.70 * t)
        - 0.19 * sin_deg(357.5 + 35999.05 * t)
        - 0.11 * sin_deg(186.6 + 966404.05 * t));
    let bp = 5.13 * sin_deg(93.27 + 483202.03 * t)
        + 0.28 * sin_deg(228.2 + 960400.87 * t)
        - 0.28 * sin_deg(318.3 + 6003.18 * t)
        - 0.17 * sin_deg(217.6 - 407332.20 * t);
    let eps = 23.4393 - 0.0130 * t;
    let dec = (sin_deg(bp) * cos_deg(eps) + cos_deg(bp) * sin_deg(eps) * sin_deg(lp))
        .asin().to_degrees();
    let ra = (sin_deg(lp) * cos_deg(eps) - cos_deg(bp) * sin_deg(lp))
        .atan2(cos_deg(lp)).to_degrees();
    equatorial_to_horizontal(lat, lon, &utc, deg360(ra), dec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d).unwrap().and_hms_opt(h, mi, 0).unwrap()
    }

    const HZ_LAT: f64 = 30.29; // 杭州
    const HZ_LON: f64 = 120.16;

    #[test]
    fn sun_noon_high_and_south() {
        // 杭州正午(北京时间 12:00): 太阳应高且近南方(自南方位接近 0)
        let (alt, az) = sun_alt_az(HZ_LAT, HZ_LON, &dt(2026, 10, 27, 12, 0));
        assert!(alt > 35.0 && alt < 55.0, "正午高度角 {alt}");
        assert!(az.abs() < 25.0, "正午方位自南应近 0, az={az}");
    }

    #[test]
    fn sun_midnight_below_horizon() {
        let (alt, _) = sun_alt_az(HZ_LAT, HZ_LON, &dt(2026, 10, 27, 0, 0));
        assert!(alt < -30.0, "午夜太阳应在地平线下, alt={alt}");
    }

    #[test]
    fn sun_rise_time_near_horizon() {
        // 日出时刻附近太阳高度应接近 0(数据源 config 展示 06:09)
        let (alt, _) = sun_alt_az(HZ_LAT, HZ_LON, &dt(2026, 10, 27, 6, 9));
        assert!(alt.abs() < 6.0, "日出高度角 {alt}");
    }

    #[test]
    fn sun_summer_higher_than_winter() {
        let (s, _) = sun_alt_az(HZ_LAT, HZ_LON, &dt(2026, 6, 21, 12, 0));
        let (w, _) = sun_alt_az(HZ_LAT, HZ_LON, &dt(2026, 12, 21, 12, 0));
        assert!(s > 75.0, "夏至正午 {s}");
        assert!(w > 30.0 && w < 40.0, "冬至正午 {w}");
    }

    #[test]
    fn moon_values_in_range() {
        for h in [0u32, 6, 12, 18] {
            let (alt, az) = moon_alt_az(HZ_LAT, HZ_LON, &dt(2026, 10, 27, h, 0));
            assert!((-90.0..=90.0).contains(&alt), "月高 {alt}");
            assert!((0.0..=360.0).contains(&az), "月方位 {az}");
        }
    }
}
