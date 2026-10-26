# -*- coding: utf-8 -*-
"""Kindle 墨水屏 Dashboard 渲染器
数据抓取(天气/RSS/待办/倒数日) -> 注入 HTML 模板 -> Edge 无头截图 -> HTTP 服务
用法: python dashboard.py once              (渲染一次)
      python dashboard.py serve             (循环渲染 + 提供图片下载 + SSH 推送 Kindle)
      python dashboard.py push              (渲染一次并立即推送 Kindle)
      python dashboard.py push-deploy-key   (首次配置:生成密钥并部署到 Kindle)
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
    # Windows
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    # macOS
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
]
W, H = 1072, 1448

WMO = {0: "晴", 1: "晴间多云", 2: "多云", 3: "阴", 45: "雾", 48: "雾凇",
       51: "毛毛雨", 53: "毛毛雨", 55: "毛毛雨", 56: "冻毛毛雨", 57: "冻毛毛雨",
       61: "小雨", 63: "中雨", 65: "大雨", 66: "冻雨", 67: "冻雨",
       71: "小雪", 73: "中雪", 75: "大雪", 77: "雪粒",
       80: "阵雨", 81: "阵雨", 82: "强阵雨", 85: "阵雪", 86: "阵雪",
       95: "雷暴", 96: "雷暴冰雹", 99: "雷暴冰雹"}
WEEKDAY = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]

REFRESH = threading.Event()  # Web/API 触发的"立刻刷新"

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


# ---------- 日程(ics 订阅) ----------
def fetch_schedule(urls):
    """极简 VEVENT 解析: 只取今日(或跨日含今日)事件, RRULE 暂不支持。
    TZID 时区按本地墙钟近似(Z 后缀按 UTC 转)。"""
    import datetime as _dt
    today = _dt.date.today()
    out = []
    for u in urls:
        try:
            body = http_get(u, timeout=25).decode("utf-8", "replace")
        except Exception:
            continue
        lines = re.sub(r"\r\n[ \t]", "", body).splitlines()
        cur = None
        for ln in lines:
            if ln.strip() == "BEGIN:VEVENT":
                cur = {}
            elif ln.strip() == "END:VEVENT":
                if cur is not None:
                    out.append(cur)
                cur = None
            elif cur is not None and ":" in ln:
                key, val = ln.split(":", 1)
                k = key.split(";")[0].upper()
                if k in ("DTSTART", "DTEND", "SUMMARY") and k not in cur:
                    cur[k] = val.strip()
    def parse_dt(v):
        v = v.strip()
        utc = v.endswith("Z")
        v = v.rstrip("Z")
        try:
            if "T" in v:
                d = _dt.datetime.strptime(v, "%Y%m%dT%H%M%S")
            else:
                return _dt.datetime.strptime(v, "%Y%m%d"), True
        except ValueError:
            return None, False
        if utc:
            d = d + _dt.timedelta(hours=8)  # 近似: UTC 按 CST(+8) 本地化
        return d, False
    picked = []
    for ev in out:
        if "DTSTART" not in ev:
            continue
        start, all_day = parse_dt(ev["DTSTART"])
        if start is None:
            continue
        end = start
        if "DTEND" in ev:
            end, _ = parse_dt(ev["DTEND"])
        start_d, end_d = start.date(), end.date()
        if end.time() == _dt.time(0, 0) and not all_day:
            end_d = end_d - _dt.timedelta(days=1)  # DTEND 排他
        if not (start_d <= today <= max(end_d, start_d)):
            continue
        title = (ev.get("SUMMARY") or "(无标题)").replace("\\,", ",")
        out_ev = {"title": title[:36], "all_day": all_day,
                  "time": "" if all_day else start.strftime("%H:%M")}
        out_ev["_s"] = (0 if all_day else 1, start)
        picked.append(out_ev)
    picked.sort(key=lambda e: e["_s"])
    for e in picked:
        e.pop("_s", None)
    return picked


# ---------- 热榜聚合 ----------
def _hot_github(c):
    import datetime as _dt
    since = (_dt.date.today() - _dt.timedelta(days=7)).isoformat()
    body = http_get("https://api.github.com/search/repositories?q=created:>" + since +
                    "&sort=stars&order=desc&per_page=5", timeout=25)
    d = json.loads(body)
    return [{"title": (r.get("full_name") or ""), "hot": "★%s" % r.get("stargazers_count", 0)}
            for r in d.get("items", [])[:5]]

def _hot_weibo(c):
    body = http_get("https://weibo.com/ajax/side/hotSearch", timeout=25)
    d = json.loads(body)
    items = (d.get("data") or {}).get("realtime") or []
    return [{"title": (i.get("word") or "")[:30], "hot": str(i.get("num", ""))}
            for i in items[:5]]

def _hot_bilibili(c):
    body = http_get("https://api.bilibili.com/x/web-interface/popular?ps=5", timeout=25)
    d = json.loads(body)
    items = ((d.get("data") or {}).get("list")) or []
    return [{"title": (i.get("title") or "")[:30], "hot": "▶%s" % i.get("stat", {}).get("view", "")}
            for i in items[:5]]

HOTLIST_FETCHERS = {"github": _hot_github, "weibo": _hot_weibo, "bilibili": _hot_bilibili}
HOTLIST_NAMES = {"github": "GitHub", "weibo": "微博", "bilibili": "B站"}

def fetch_hotlist(sources):
    out = []
    for s in sources:
        fn = HOTLIST_FETCHERS.get(s)
        if not fn:
            continue
        try:
            out.append({"src": HOTLIST_NAMES[s], "items": fn(None)})
        except Exception:
            out.append({"src": HOTLIST_NAMES[s], "items": []})
    return out


# ---------- 每日壁纸(Bing, 抖动灰度) ----------
def fetch_wallpaper(out_dir):
    """下载 Bing 每日一图, cover 裁切到屏尺寸并 Floyd-Steinberg 抖动成 1-bit 灰度。
    按日期缓存, 当天只下载一次。"""
    import io
    from PIL import Image, ImageOps
    png = Path(out_dir) / "wallpaper.png"
    stamp = Path(out_dir) / "wallpaper.date"
    today = dt.date.today().isoformat()
    if png.exists() and stamp.exists() and stamp.read_text().strip() == today:
        return True
    meta = json.loads(http_get("https://www.bing.com/HPImageArchive.aspx?format=js&idx=0&n=1",
                               timeout=20))
    imgs = meta.get("images") or []
    if not imgs:
        return False
    url = "https://www.bing.com" + imgs[0].get("url", "")
    body = http_get(url, timeout=40)
    img = Image.open(io.BytesIO(body)).convert("L")
    img = ImageOps.fit(img, (W, H), Image.LANCZOS)
    dithered = img.convert("1", dither=Image.FLOYDSTEINBERG)
    dithered.save(png)
    stamp.write_text(today)
    return True


# ---------- 法定节假日(接口优先, 农历库兜底) ----------
def fallback_holidays(today):
    """法定节日推算: 元旦/劳动节/国庆固定, 春节/端午/中秋由农历, 清明由节气表"""
    from lunar_python import Lunar
    out = []
    for y in (today.year, today.year + 1):
        out += [(dt.date(y, 1, 1), "元旦"), (dt.date(y, 5, 1), "劳动节"),
                (dt.date(y, 10, 1), "国庆节")]
        try:
            for m, dd, name in ((1, 1, "春节"), (5, 5, "端午"), (8, 15, "中秋")):
                so = Lunar.fromYmd(y, m, dd).getSolar()
                out.append((dt.date(so.getYear(), so.getMonth(), so.getDay()), name))
        except Exception:
            pass
        try:
            tb = Lunar.fromDate(dt.datetime(y, 6, 1)).getJieQiTable()
            so = tb.get("清明")
            if so:
                out.append((dt.date(so.getYear(), so.getMonth(), so.getDay()), "清明"))
        except Exception:
            pass
    return sorted([(d, n) for d, n in out if d >= today])


def fetch_holidays(base_dir, today):
    """法定节日由内置农历/节气表推算(与 lunar_python 逐日核对), 无需联网;
    第三方接口只覆盖已公布年份且被 Cloudflare 拦截, 已弃用。"""
    return fallback_holidays(today)


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
    mods = cfg.get("modules", {})
    if mods.get("weather", True):
        try:
            data["weather"] = fetch_weather(cfg["city"])
        except Exception as e:
            data["weather_error"] = "天气获取失败(%s)" % type(e).__name__
    if mods.get("rss", True):
        try:
            data["rss"] = fetch_rss(cfg.get("rss_sources", []), cfg.get("rss_count", 6))
        except Exception:
            pass
    if mods.get("yiyan", True):
        try:
            data["yiyan"] = fetch_yiyan()
        except Exception:
            pass
    if mods.get("lunar", True):
        try:
            data.update(fetch_lunar_moon())
        except Exception:
            pass
    if mods.get("countdowns", True):
        custom = compute_countdowns(cfg.get("countdowns", []))
        holidays = fetch_holidays(ROOT, today)[:4]
        hdays = [{"name": n, "days": (d - today).days,
                  "date": d.strftime("%m-%d"), "holiday": True} for d, n in holidays]
        names = {h["name"] for h in hdays}
        custom = [c for c in custom if c["name"] not in names]  # 重名以接口为准
        merged = sorted(hdays + custom, key=lambda x: x["days"])
        data["countdowns"] = merged
    else:
        data["countdowns"] = []
    schedule_urls = cfg.get("schedule_urls") or []
    if schedule_urls:
        try:
            data["schedule"] = fetch_schedule(schedule_urls)
        except Exception:
            pass
    hot_sources = cfg.get("hotlist_sources") or []
    if hot_sources:
        try:
            data["hotlist"] = fetch_hotlist(hot_sources)
        except Exception:
            pass
    if cfg.get("wallpaper"):
        try:
            if fetch_wallpaper(OUT):
                data["wallpaper"] = {"file": "wallpaper.png", "credit": "Bing"}
        except Exception as e:
            print("wallpaper error:", e)
    return cfg, data

def shoot(edge, png, url, w=W, h=H):
    """无头截图。Windows 上 Edge 截完自己退出;
    macOS 上写完截图进程常驻不退,轮询图片落盘后主动结束。"""
    tmp_profile = OUT / "edge_profile"
    tmp_profile.mkdir(exist_ok=True)
    for headless in ("--headless=new", "--headless"):
        if png.exists():
            png.unlink()
        cmd = [edge, headless, "--disable-gpu", "--hide-scrollbars",
               "--force-device-scale-factor=1", "--default-background-color=FFFFFFFF",
               "--virtual-time-budget=4000", "--user-data-dir=" + str(tmp_profile),
               f"--window-size={w},{h}", f"--screenshot={png}", url]
        proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        last_size, deadline = -1, time.time() + 90
        while time.time() < deadline:
            if proc.poll() is not None:
                break
            try:
                size = png.stat().st_size
            except OSError:
                size = -1
            if size > 10000 and size == last_size:  # 尺寸连续两次不变,视为写完
                break
            last_size = size
            time.sleep(0.5)
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
        if png.exists() and png.stat().st_size > 10000:
            return True
    return False

def list_templates():
    """扫描当前提供的三套模板元数据(与 Rust 版解析规则一致)"""
    allowed = {"template.html", "template_editorial.html", "template_terminal.html"}
    out = []
    for f in sorted(ROOT.glob("template*.html")):
        if f.name not in allowed:
            continue;
        text = f.read_text("utf-8")
        info = {"file": f.name,
                "name": f.name.removeprefix("template").removesuffix(".html") or f.name,
                "orientations": ["portrait"]}
        m = re.search(r"kindle-template:\s*([^\n>]+)", text)
        if m:
            toks = dict(t.split("=", 1) for t in m.group(1).split() if "=" in t)
            info["name"] = toks.get("name", info["name"])
            ori = [o for o in toks.get("orientations", "").split(",") if o in ("portrait", "landscape")]
            if ori:
                info["orientations"] = ori
        out.append(info)
    return out


def render_once():
    cfg, data = build_data()
    w, h = (H, W) if cfg.get("orientation") == "landscape" else (W, H)
    tpl_name = cfg.get("template", TEMPLATE.name)
    if tpl_name not in {"template.html", "template_editorial.html", "template_terminal.html"}:
        tpl_name = TEMPLATE.name
    tpl_path = ROOT / tpl_name
    tpl = tpl_path.read_text("utf-8")
    html = tpl.replace("/*__DATA__*/null", json.dumps(data, ensure_ascii=False)) \
             .replace("/*__ORIENTATION__*/portrait", cfg.get("orientation", "portrait"))
    rendered = OUT / "rendered.html"
    rendered.write_text(html, "utf-8")
    png = OUT / "dashboard.png"
    edge = next((p for p in EDGE_CANDIDATES if Path(p).exists()), None)
    if not edge:
        raise RuntimeError("未找到 Edge/Chrome 浏览器(已尝试 Windows/macOS 常见安装路径)")
    if not shoot(edge, png, rendered.as_uri(), w, h):
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

# ---------- 主动推送:SSH 通知 Kindle 立即拉图 ----------
SSH_KEY = Path.home() / ".ssh" / "kindle_id_rsa"  # 专用 RSA 密钥(老 dropbear 不认 ed25519)
_push_lock = threading.Lock()

def _ssh_push_once(cfg):
    cmd = ["ssh", "-p", str(cfg.get("kindle_ssh_port", 2222)),
           "-i", str(SSH_KEY), "-o", "IdentitiesOnly=yes", "-o", "BatchMode=yes",
           "-o", "ConnectTimeout=5", "-o", "StrictHostKeyChecking=accept-new",
           "-o", "PubkeyAcceptedAlgorithms=+ssh-rsa",
           f"{cfg.get('kindle_user', 'root')}@{cfg['kindle_host']}",
           cfg.get("kindle_update_cmd", "sh /mnt/us/extensions/onlinescreensaver/bin/update.sh")]
    try:
        return subprocess.run(cmd, capture_output=True, timeout=60).returncode == 0
    except Exception:
        return False

def push_to_kindle(retry_minutes=None, quiet=False):
    """让 Kindle 立即执行 update.sh 拉最新图。
    Kindle 睡眠时网络不通,每 30 秒重试,醒来即补推;
    超过 retry_minutes 分钟仍失败则放弃(反正下一个渲染周期还会再推)。
    返回 True/False。"""
    cfg = json.loads(CONFIG.read_text("utf-8"))
    if not cfg.get("kindle_host"):
        return False
    if retry_minutes is None:
        retry_minutes = cfg.get("push_retry_min", 10)
    if not SSH_KEY.exists():  # 密钥还没部署,不重试
        if not quiet:
            print("push 跳过: 未找到", SSH_KEY, "(先运行 python dashboard.py push-deploy-key)")
        return False
    deadline = time.time() + retry_minutes * 60
    while True:
        if _ssh_push_once(cfg):
            if not quiet:
                print("push 成功: Kindle 已触发拉取", time.strftime("%H:%M:%S"))
            return True
        if time.time() >= deadline:
            if not quiet:
                print(f"push 放弃: {retry_minutes} 分钟内连不上 Kindle(可能在睡眠,醒来后按 Kindle 上的「立即更新信息屏」即可)")
            return False
        time.sleep(30)

def _push_worker():
    # serve 模式下后台跑;同一时刻只保留一条重试链,新渲染到来时旧链
    # 拉的也是同一张最新图,直接跳过即可
    if not _push_lock.acquire(blocking=False):
        return
    try:
        push_to_kindle(quiet=True)
    finally:
        _push_lock.release()

def deploy_key():
    """生成专用密钥并通过密码登录部署到 Kindle(只需做一次)"""
    if not SSH_KEY.exists():
        print("生成专用密钥", SSH_KEY)
        subprocess.run(["ssh-keygen", "-t", "rsa", "-b", "2048", "-N", "",
                        "-C", "kindle-dashboard", "-f", str(SSH_KEY)], check=True)
    cfg = json.loads(CONFIG.read_text("utf-8"))
    host = cfg.get("kindle_host")
    if not host:
        print("config.json 缺 kindle_host"); return
    user = cfg.get("kindle_user", "root")
    print(f"\n接下来会要求输入一次 {user}@{host} 的密码(Kindle 上 OpenSSH 的 root 密码)")
    cmd = ["ssh-copy-id", "-p", str(cfg.get("kindle_ssh_port", 2222)),
           "-i", str(SSH_KEY) + ".pub", "-o",
           "PubkeyAcceptedAlgorithms=+ssh-rsa", f"{user}@{host}"]
    ok = subprocess.run(cmd).returncode == 0
    print("\n部署完成,测试免密推送..." if ok else "\n部署失败,检查密码/网络后重试")
    if ok:
        push_to_kindle(retry_minutes=1)

LAST_RENDER = ["--"]


def read_config():
    try:
        return json.loads(CONFIG.read_text("utf-8"))
    except Exception:
        return {}


def save_config(new):
    """校验并合并保存(与 Rust 版同一套规则), 返回错误信息或 None"""
    if not isinstance(new, dict):
        return "配置必须是 JSON 对象"
    if not str(new.get("city", "")).strip():
        return "城市不能为空"
    port = new.get("port", 8787)
    if not isinstance(port, int) or not (1 <= port <= 65535):
        return f"端口 {port} 超出范围"
    if new.get("interval_min", 10) in (0, None):
        return "刷新间隔至少 1 分钟"
    if new.get("orientation", "portrait") not in ("portrait", "landscape"):
        return f"未知朝向: {new.get('orientation')}"
    merged = read_config()
    merged.update(new)
    CONFIG.write_text(json.dumps(merged, ensure_ascii=False, indent=2), "utf-8")
    return None


def serve():
    cfg = read_config()
    port = cfg.get("port", 8787)
    interval = cfg.get("interval_min", 10)
    webui = ROOT / "webui.html"

    class Handler(SimpleHTTPRequestHandler):
        def __init__(self, *a, **kw):
            super().__init__(*a, directory=str(OUT), **kw)
        def log_message(self, *a):
            pass

        def _json(self, obj, code=200):
            body = json.dumps(obj, ensure_ascii=False).encode("utf-8")
            self.send_response(code)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def _body(self):
            n = min(int(self.headers.get("Content-Length", 0) or 0), 256 * 1024)
            return self.rfile.read(n)

        def _api(self):
            """返回 (已处理?, 响应已写出?) —— 与 Rust 版同一套契约"""
            path = self.path.split("?")[0]
            if path == "/api/status":
                c = read_config()
                self._json({"lan_ip": lan_ip(), "port": port,
                            "last_render": LAST_RENDER[0],
                            "template": c.get("template", "template.html"),
                            "orientation": c.get("orientation", "portrait")})
                return True
            if path == "/api/config":
                value = read_config() or {}
                defaults = json.loads((ROOT / "config.json").read_text("utf-8")) \
                    if (ROOT / "config.json").exists() else {}
                for k, v in defaults.items():
                    value.setdefault(k, v)
                self._json(value)
                return True
            if path == "/api/templates":
                self._json({"templates": list_templates()})
                return True
            if path == "/api/todo":
                c = read_config()
                f = ROOT / c.get("todo_file", "todo.txt")
                self._json({"content": f.read_text("utf-8") if f.exists() else ""})
                return True
            return False

        def _api_holidays(self):
            today = dt.date.today()
            days = fetch_holidays(ROOT, today)[:6]
            self._json({"holidays": [
                {"name": n, "date": d.isoformat(), "days": (d - today).days}
                for d, n in days]})

        def do_GET(self):
            path = self.path.split("?")[0]
            if path in ("/", "/index.html", "/webui", "/webui.html"):
                if path in ("/webui", "/webui.html"):
                    self.send_response(302)
                    self.send_header("Location", "/")
                    self.end_headers()
                    return
                body = webui.read_bytes() if webui.exists() else b"webui.html missing"
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Cache-Control", "no-store")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return
            if self.path.startswith("/api/holidays"):
                self._api_holidays()
                return
            if self.path.startswith("/api/") and self._api():
                return
            super().do_GET()

        def do_POST(self):
            path = self.path.split("?")[0]
            if path == "/api/config":
                try:
                    new = json.loads(self._body())
                except Exception as e:
                    self._json({"ok": False, "error": f"JSON 解析失败: {e}"}, 400)
                    return
                err = save_config(new)
                if err:
                    self._json({"ok": False, "error": err}, 400)
                    return
                REFRESH.set()
                self._json({"ok": True})
                return
            if path == "/api/todo":
                text = self._body().decode("utf-8", "replace")
                c = read_config()
                (ROOT / c.get("todo_file", "todo.txt")).write_text(text, "utf-8")
                REFRESH.set()
                self._json({"ok": True})
                return
            if path == "/api/refresh":
                REFRESH.set()
                self._json({"ok": True})
                return
            self._json({"ok": False, "error": "not found"}, 404)

    httpd = ThreadingHTTPServer(("0.0.0.0", port), Handler)
    httpd.daemon_threads = True
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    print(f"服务已启动: http://0.0.0.0:{port}/dashboard.png  每 {interval} 分钟刷新")
    print(f"Kindle 拉取地址: http://{lan_ip()}:{port}/dashboard.png")
    print(f"配置管理页面:   http://127.0.0.1:{port}/")
    print("(填入 Kindle 端 extensions/dashboard-updater/bin/dashboard_url.conf,"
          " 如拉取失败先检查本机防火墙是否放行 Python:Windows 防火墙 / macOS 首次运行弹窗点允许)")
    while True:
        try:
            render_once()
            LAST_RENDER[0] = dt.datetime.now().strftime("%H:%M")
        except Exception as e:
            print("render error:", e)
        REFRESH.wait(interval * 60)
        REFRESH.clear()


if __name__ == "__main__":
    OUT.mkdir(exist_ok=True)
    mode = sys.argv[1] if len(sys.argv) > 1 else "once"
    if mode == "serve":
        serve()
    elif mode == "push":
        render_once()
        push_to_kindle()
    elif mode == "push-deploy-key":
        deploy_key()
    else:
        render_once()
