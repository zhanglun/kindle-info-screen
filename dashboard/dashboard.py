# -*- coding: utf-8 -*-
"""Kindle 墨水屏 Dashboard 渲染器
数据抓取(天气/RSS/待办/倒数日) -> 注入 HTML 模板 -> Edge 无头截图 -> HTTP 服务
用法: python dashboard.py once   (渲染一次)
      python dashboard.py serve  (循环渲染 + 提供图片下载服务)
"""
import json, ssl, socket, sys, time, re, math, shutil, subprocess, threading, datetime as dt
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path
from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler

ROOT = Path(__file__).parent
OUT = ROOT / "out"
TEMPLATE = ROOT / "template.html"
CONFIG = ROOT / "config.json"
GEO_CACHE = ROOT / "city_cache.json"
EDGE_CANDIDATES = [
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]
W, H = 1072, 1448

WMO = {0: "晴", 1: "晴间多云", 2: "多云", 3: "阴", 45: "雾", 48: "雾凇",
       51: "毛毛雨", 53: "毛毛雨", 55: "毛毛雨", 56: "冻毛毛雨", 57: "冻毛毛雨",
       61: "小雨", 63: "中雨", 65: "大雨", 66: "冻雨", 67: "冻雨",
       71: "小雪", 73: "中雪", 75: "大雪", 77: "雪粒",
       80: "阵雨", 81: "阵雨", 82: "强阵雨", 85: "阵雪", 86: "阵雪",
       95: "雷暴", 96: "雷暴冰雹", 99: "雷暴冰雹"}
WEEKDAY = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]

CTX = ssl.create_default_context()
CTX.check_hostname = False
CTX.verify_mode = ssl.CERT_NONE

def http_get(url, timeout=20):
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0 kindle-dashboard"})
    with urllib.request.urlopen(req, timeout=timeout, context=CTX) as r:
        return r.read()

# ---------- 天气 ----------
def geocode(city):
    if GEO_CACHE.exists():
        cache = json.loads(GEO_CACHE.read_text("utf-8"))
        if city in cache:
            return cache[city]
    url = ("https://geocoding-api.open-meteo.com/v1/search?name=" +
           urllib.parse.quote(city) + "&count=1&language=zh&format=json")
    data = json.loads(http_get(url))
    if not data.get("results"):
        raise RuntimeError("找不到城市: " + city)
    hit = {"lat": data["results"][0]["latitude"], "lon": data["results"][0]["longitude"],
           "name": data["results"][0].get("name", city)}
    cache = json.loads(GEO_CACHE.read_text("utf-8")) if GEO_CACHE.exists() else {}
    cache[city] = hit
    GEO_CACHE.write_text(json.dumps(cache, ensure_ascii=False), "utf-8")
    return hit

def fetch_weather(city):
    geo = geocode(city)
    url = (f"https://api.open-meteo.com/v1/forecast?latitude={geo['lat']}&longitude={geo['lon']}"
           "&current_weather=true"
           "&daily=weathercode,temperature_2m_max,temperature_2m_min,sunrise,sunset"
           "&forecast_days=5&timezone=auto")
    d = json.loads(http_get(url))
    cur = d["current_weather"]
    daily = d["daily"]
    days = []
    for i in range(len(daily["time"])):
        date = dt.date.fromisoformat(daily["time"][i])
        days.append({
            "label": "今天" if i == 0 else ("明天" if i == 1 else WEEKDAY[date.weekday()]),
            "code": WMO.get(daily["weathercode"][i], "未知"),
            "wc": daily["weathercode"][i],
            "hi": round(daily["temperature_2m_max"][i]),
            "lo": round(daily["temperature_2m_min"][i]),
        })
    w = {"city": geo["name"], "temp": round(cur["temperature"]),
         "code": WMO.get(cur["weathercode"], "未知"), "wc": cur["weathercode"],
         "wind": round(cur["windspeed"]), "days": days,
         "sunrise": daily["sunrise"][0][11:16], "sunset": daily["sunset"][0][11:16]}
    # 空气质量(同一数据源,免费无 key)
    try:
        aqi_url = (f"https://air-quality-api.open-meteo.com/v1/air-quality?latitude={geo['lat']}"
                   f"&longitude={geo['lon']}&current=pm2_5,us_aqi&timezone=auto")
        a = json.loads(http_get(aqi_url)).get("current", {})
        aqi, pm = a.get("us_aqi"), a.get("pm2_5")
        if aqi is not None:
            grade = ("优" if aqi <= 50 else "良" if aqi <= 100 else "轻度污染" if aqi <= 150
                     else "中度污染" if aqi <= 200 else "重度污染" if aqi <= 300 else "严重污染")
            w["aqi"] = round(aqi)
            w["aqi_desc"] = grade
            w["pm25"] = round(pm) if pm is not None else None
    except Exception:
        pass
    return w

def fetch_yiyan():
    d = json.loads(http_get("https://v1.hitokoto.cn/?c=d&c=i&c=k", timeout=15))
    return {"text": d.get("hitokoto", ""), "from": d.get("from", "") or "佚名",
            "who": d.get("from_who", "") or ""}

def fetch_lunar_moon():
    lunar_str = jieqi = None
    try:
        from lunar_python import Solar
        solar = Solar.fromDate(dt.datetime.now())
        lunar = solar.getLunar()
        lunar_str = lunar.toString().split("年")[-1]
        jq = lunar.getPrevJieQi()
        js = jq.getSolar()
        jq_days = (dt.date.today() - dt.date(js.getYear(), js.getMonth(), js.getDay())).days
        if jq_days == 0:
            jieqi = jq.getName()
        elif jq_days < 15:
            jieqi = jq.getName() + "后" + str(jq_days) + "天"
        else:
            jieqi = None
    except Exception:
        pass
    # 月相:朔望月周期本地推算
    epoch = dt.date(2000, 1, 6)
    age = (dt.date.today() - epoch).days % 29.530588
    phases = ["新月", "娥眉月", "上弦月", "盈凸月", "满月", "亏凸月", "下弦月", "残月"]
    name = phases[int((age / 29.530588) * 8 + 0.5) % 8]
    illum = round((1 - math.cos(2 * math.pi * age / 29.530588)) / 2 * 100)
    return {"lunar": lunar_str, "jieqi": jieqi, "moon": {"name": name, "illum": illum}}

# ---------- RSS ----------
def fetch_rss(sources, limit):
    items = []
    for src in sources:
        try:
            xml = http_get(src["url"], timeout=25)
            root = ET.fromstring(xml)
            channel = root.find("channel") if root.tag == "rss" else root
            nodes = channel.findall(".//item") if channel is not None else []
            if not nodes:
                nodes = root.findall(".//{*}entry")
            for n in nodes[:max(1, limit // max(1, len(sources)))]:
                t = n.findtext("title") or n.findtext("{*}title") or ""
                t = re.sub(r"<!\[CDATA\[|\]\]>", "", t).strip()
                if t:
                    items.append({"src": src["name"], "title": t[:32]})
        except Exception as e:
            items.append({"src": src["name"], "title": "(源暂时不可用: %s)" % type(e).__name__})
    return items[:limit]

# ---------- 待办 / 倒数日 ----------
def read_todo(path):
    f = ROOT / path
    if not f.exists():
        return []
    return [ln.strip() for ln in f.read_text("utf-8").splitlines() if ln.strip()]

def compute_countdowns(entries):
    today = dt.date.today()
    out = []
    saturday = today + dt.timedelta(days=(5 - today.weekday()) % 7 or 7)
    out.append({"name": "周末", "days": (saturday - today).days,
                "date": saturday.strftime("%m-%d")})
    for e in entries:
        try:
            d = dt.date.fromisoformat(e["date"])
            out.append({"name": e["name"], "days": (d - today).days,
                        "date": d.strftime("%m-%d")})
        except Exception:
            pass
    return sorted(out, key=lambda x: x["days"])

# ---------- 渲染 ----------
def build_data():
    cfg = json.loads(CONFIG.read_text("utf-8"))
    now = dt.datetime.now()
    data = {
        "date": "%d月%d日" % (now.month, now.day),
        "weekday": WEEKDAY[now.weekday()],
        "time": now.strftime("%H:%M"),
        "year": now.year,
        "todo": read_todo(cfg.get("todo_file", "todo.txt")),
        "countdowns": compute_countdowns(cfg.get("countdowns", [])),
        "weather": None, "rss": [],
    }
    try:
        data["weather"] = fetch_weather(cfg["city"])
    except Exception as e:
        data["weather_error"] = "天气获取失败(%s)" % type(e).__name__
    try:
        data["rss"] = fetch_rss(cfg.get("rss_sources", []), cfg.get("rss_count", 6))
    except Exception:
        pass
    try:
        data["yiyan"] = fetch_yiyan()
    except Exception:
        pass
    try:
        data.update(fetch_lunar_moon())
    except Exception:
        pass
    return cfg, data

def render_once():
    cfg, data = build_data()
    tpl = TEMPLATE.read_text("utf-8")
    html = tpl.replace("/*__DATA__*/null", json.dumps(data, ensure_ascii=False))
    rendered = OUT / "rendered.html"
    rendered.write_text(html, "utf-8")
    png = OUT / "dashboard.png"
    edge = next((p for p in EDGE_CANDIDATES if Path(p).exists()), None)
    if not edge:
        raise RuntimeError("未找到 Edge 浏览器")
    tmp_profile = OUT / "edge_profile"
    tmp_profile.mkdir(exist_ok=True)
    for headless in ("--headless=new", "--headless"):
        if png.exists():
            png.unlink()
        cmd = [edge, headless, "--disable-gpu", "--hide-scrollbars",
               "--force-device-scale-factor=1", "--default-background-color=FFFFFFFF",
               "--virtual-time-budget=4000", "--user-data-dir=" + str(tmp_profile),
               f"--window-size={W},{H}", f"--screenshot={png}", rendered.as_uri()]
        subprocess.run(cmd, capture_output=True, timeout=90)
        if png.exists() and png.stat().st_size > 10000:
            break
    if not png.exists():
        raise RuntimeError("截图失败")
    # 尽量转灰度,墨水屏显示更干净
    try:
        from PIL import Image
        img = Image.open(png).convert("L")
        img.save(png)
    except ImportError:
        pass
    print("rendered:", png, png.stat().st_size, "bytes")

def lan_ip():
    """取局域网源 IP(UDP connect 不发包), 给 Kindle 拉取地址用"""
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        s.connect(("223.5.5.5", 80))
        return s.getsockname()[0]
    except OSError:
        return "127.0.0.1"
    finally:
        s.close()

def serve():
    cfg = json.loads(CONFIG.read_text("utf-8"))
    port = cfg.get("port", 8787)
    interval = cfg.get("interval_min", 10)

    class Handler(SimpleHTTPRequestHandler):
        def __init__(self, *a, **kw):
            super().__init__(*a, directory=str(OUT), **kw)
        def log_message(self, *a):
            pass

    httpd = ThreadingHTTPServer(("0.0.0.0", port), Handler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    print(f"服务已启动: http://0.0.0.0:{port}/dashboard.png  每 {interval} 分钟刷新")
    print(f"Kindle 拉取地址: http://{lan_ip()}:{port}/dashboard.png")
    print("(填入 Kindle 端 extensions/dashboard-updater/bin/dashboard_url.conf,"
          " 如拉取失败先检查 Windows 防火墙是否放行 Python)")
    while True:
        try:
            render_once()
        except Exception as e:
            print("render error:", e)
        time.sleep(interval * 60)

if __name__ == "__main__":
    OUT.mkdir(exist_ok=True)
    mode = sys.argv[1] if len(sys.argv) > 1 else "once"
    if mode == "serve":
        serve()
    else:
        render_once()
