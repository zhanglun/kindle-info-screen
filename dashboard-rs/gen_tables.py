# -*- coding: utf-8 -*-
"""用 lunar_python 预生成农历/节气数据表, 供 Rust 端离线查询。
输出 resources/calendar_table.json:
  epoch   农历 2024 年正月初一对应的公历日期
  months  从该日起每个月的天数与农历月标识, 按时间顺序
  terms   每个公历年份的 24 节气(名称 + MM-DD), 按时间顺序
"""
import json
import datetime as dt
from pathlib import Path

from lunar_python import Lunar, LunarYear

OUT = Path(__file__).parent / "resources" / "calendar_table.json"
START_YEAR, END_YEAR = 2024, 2100  # 农历年范围(覆盖公历约 2024-02 ~ 2101-01)


def solar_date(lunar_y, lunar_m):
    """农历某年某月(负数=闰月)初一的公历日期"""
    s = Lunar.fromYmd(lunar_y, lunar_m, 1).getSolar()
    return dt.date(s.getYear(), s.getMonth(), s.getDay())


def main():
    # 1) 按时间顺序列出农历 2024.1 ~ 2100.12 的所有月份
    keys = []  # (lunar_year, month, is_leap)
    for y in range(START_YEAR, END_YEAR + 1):
        leap = LunarYear.fromYear(y).getLeapMonth()
        for m in range(1, 13):
            keys.append((y, m, False))
            if leap == m:
                keys.append((y, m, True))

    dates = [solar_date(y, -m if leap else m) for (y, m, leap) in keys]
    epoch = dates[0]
    assert epoch == dt.date(2024, 2, 10), f"epoch 意外: {epoch}"

    months = []
    for i, ((y, m, leap), d) in enumerate(zip(keys, dates)):
        nxt = dates[i + 1] if i + 1 < len(dates) else solar_date(y + 1, 1)
        length = (nxt - d).days
        assert 29 <= length <= 30, f"{y}-{m} 天数异常: {length}"
        label = f"{y}-{m}L" if leap else f"{y}-{m}"
        months.append([length, label])

    # 2) 节气: 每个公历年收集落在该年内的节气(精确到分钟, 与 lunar_python 时刻级判定一致)
    terms = {}
    for gy in range(START_YEAR, END_YEAR + 1):
        table = {}
        for probe in (dt.date(gy, 1, 1), dt.date(gy, 6, 1), dt.date(gy, 12, 31)):
            lunar = Lunar.fromDate(dt.datetime(probe.year, probe.month, probe.day))
            for name, solar in lunar.getJieQiTable().items():
                # 当年 12 月冬至的 key 是 DONG_ZHI(中文「冬至」指向最近一个已过的冬至), 统一映射
                name = {"DONG_ZHI": "冬至"}.get(name, name)
                sd = dt.date(solar.getYear(), solar.getMonth(), solar.getDay())
                if sd.year == gy:
                    table[sd] = (name, solar.getHour(), solar.getMinute())
        terms[str(gy)] = [[name, f"{d.strftime('%m-%d')}T{h:02d}:{mi:02d}"]
                          for d, (name, h, mi) in sorted(table.items())]

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps({
        "epoch": epoch.isoformat(),
        "months": months,
        "terms": terms,
    }, ensure_ascii=False, separators=(",", ":")), "utf-8")
    total = sum(m[0] for m in months)
    print(f"OK: {len(months)} 个月共 {total} 天, 节气覆盖 {START_YEAR}-{END_YEAR}, "
          f"{OUT} ({OUT.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
