# 开发说明（dashboard-rs）

信息屏服务的唯一实现。本文面向开发：代码结构、启动流程、渲染管线、资源机制、
API 契约、测试与打包。日常使用见根目录 [USAGE.md](../USAGE.md)。

## 代码结构（src/）

| 模块 | 职责 |
|---|---|
| `main.rs` | 入口：三种启动模式（托盘/`serve`/`once`）、资源释放与版本升级、渲染/推送线程、托盘事件循环、Windows 控制台附着 |
| `server.rs` | HTTP 服务：Kindle 拉图 + 静态文件 + Web 配置页 + 全部 API |
| `render.rs` | 渲染：模板注入 → Edge/Chrome 无头截图 → 灰度化；模板扫描与元数据解析 |
| `data.rs` | 数据抓取：天气/空气/一言/RSS/待办/倒数日 → 组装模板 JSON；各部分独立容错 |
| `extras.rs` | 扩展数据：日程(ics)、热榜聚合(github/weibo/bilibili)、Bing 每日壁纸（裁切+Floyd–Steinberg 抖动成 1-bit 灰度，按天缓存） |
| `lunar.rs` | 农历/节气/月相：查 `calendar_table.json`（月相按朔望月周期本地推算） |
| `config.rs` | 配置读取：字段级容错，单字段坏只回退该字段默认值并 `warn_once` |

外部依赖刻意少：`tiny_http`（HTTP）、`reqwest`（抓取；Windows 走 schannel，
其余 rustls）、`image`（灰度）、`tray-icon`+`winit`（托盘）、`chrono`、`serde_json`。

## 启动流程与线程模型

```
main
 ├─ KINDLE_DASHBOARD_RESTART 时睡 1s（托盘「重新启动」用）
 ├─ 解析子命令：serve / once / 无参=托盘；未知参数报错退出
 ├─ (Windows) serve/once 时 attach_parent_console()
 ├─ base_dir()：KINDLE_DASHBOARD_HOME > macOS .app 的 ~/Library/Application Support/KindleDashboard > exe 旁 resources/
 ├─ 释放内置资源 + 版本升级（见下节）
 ├─ once：render_once 后直接退出（exit 0/1）
 ├─ server::start()：起 HTTP 线程；端口被占则报错（托盘弹窗 / serve 退出码 1）
 ├─ 起渲染线程：立即渲染一次 → 循环 recv_timeout(interval)；API/托盘发信号可提前唤醒
 └─ serve：主线程 park 挂起等 Ctrl+C；托盘：进 winit 事件循环
```

- 渲染线程用 `catch_unwind` 包住，内部崩溃只记日志，下个周期重试。
- 每次渲染成功后，若 `push_after_render` 且 `kindle_host` 已设，起 SSH 推送线程
  （`PUSH_LOCK` 保证同一时刻只有一条重试链；30s 间隔重试，`push_retry_min` 分钟后放弃）。
- 日志统一走 `log_line`：stderr + `out/app.log` 双写。

## 资源嵌入与升级机制

- `EMBEDDED_RESOURCES` 用 `include_str!` 把 `resources/` 编译进二进制：首次运行释放到数据目录。
- `RESOURCE_VERSION` 与数据目录的 `resources.version` 比较，不一致则视为升级：
  **非用户文件**（模板/农历表/webui.html）内容有差异就刷新；**用户文件**
  （`config.json`/`todo.txt`）只在缺失时创建，永不覆盖。
- 改过 `resources/` 里的内置文件后，记得把 `main.rs` 的 `RESOURCE_VERSION` 加一，
  否则已部署的机器不会升级。

## 渲染管线

```
config::load → data::build_data(抓数据+农历+倒数日…) → 模板字符串替换：
  "/*__DATA__*/null"        ← 模板 JSON（serde 序列化）
  "/*__ORIENTATION__*/portrait" ← 朝向
→ 写 out/rendered.html → Edge/Chrome 无头 --screenshot（--headless=new 失败退 --headless）
→ 轮询 PNG 落盘尺寸：>10KB 且连续两次不变视为写完（90s 超时强杀）
→ image::to_luma8 转灰度回写 dashboard.png → 日志 "rendered: dashboard.png N bytes"
```

- 截图尺寸：竖版 1072×1448，横版 1448×1072（PW4 面板）。
- 浏览器探测路径在 `render.rs::BROWSER_CANDIDATES`（Windows/macOS 常见位置）。
- 无头浏览器临时配置写在 `out/edge_profile/`（在 `target/` 内，不入库）。

## 模板开发

数据目录放 `template_xx.html`，头部加一行自描述注释：

```html
<!-- kindle-template: name=名称 orientations=portrait,landscape -->
```

Web 页 `/api/templates` 与渲染即按此识别。两点注意：

1. **白名单**：`render.rs` 的 `ALLOWED`（与 `list_templates` 同款）限定了实际可用的
   模板文件名，新模板要同步加进去；白名单外回退 `template.html`。
2. 模板是纯 HTML+JS，数据从注入的 `__DATA__` JSON 读，不经过任何服务端模板引擎；
   无元数据头的文件以文件名兜底且仅竖版。

当前启用五套(白名单见 `render.rs::ALLOWED_TEMPLATES`):

| 模板 | 底色 | 内容 |
|---|---|---|
| 画框 `template_gallery.html` | 白 | 水墨月夜(月随真实相位) / Bing 壁纸 + 白纸展签 |
| 晨报 `template_morning.html` | 白 | 报纸版式: 天气头条(线稿图标+五日温度带) + 行情报价栏(1h 涨跌) + 日程/待办/倒数日分栏 + 热榜 + 报尾一言 |
| 相框夜画 `template_frame.html` | 黑 | 大月相 + 星空 + 地平线, 展签(时钟/天气/倒数日/一言) |
| 诗笺 `template_poem.html` | 黑 | 一言宋体竖排 + 成语侧注 + 月相 |
| 天穹 `template_horizon.html` | 昼夜灰阶 | 太阳/月亮按真实天文位置摆放(Meeus 低精度算法, 城市经纬度取自天气缓存), 远山 + 白纸展签(时钟/天气/日出日落) |

行情币种在 `config.json` 的 `crypto` 字段配置(CoinGecko id,最多 5 个);
成语来自内置 `idioms.json`(48 条精选,按日轮换,离线可用)。

## 农历表再生成

```bash
../.venv/bin/python gen_tables.py   # 依赖 lunar_python，生成 resources/calendar_table.json
```

覆盖公历 2024-02 ~ 2101-01；节气精确到分钟，与 lunar_python 时刻级判定一致
（用 800 个随机时刻校验过）。改完生成脚本后同样要升 `RESOURCE_VERSION`。

## HTTP API 契约

供 Web 页与脚本调用，返回 JSON；出错统一 `{"ok": false, "error": "…"}` + HTTP 400。

| 接口 | 说明 |
|---|---|
| `GET /api/status` | `lan_ip` / `port` / `last_render`（HH:MM 或 "--"）/ `template` / `orientation` |
| `GET /api/config` | 当前 `config.json`，缺字段用内置默认值补齐 |
| `POST /api/config` | 校验（city 非空、port 1–65535、interval≥1、orientation 枚举）→ 合并保存（保留未知字段）→ 自动触发刷新 |
| `POST /api/preview` | 请求体 = 表单当前配置（同 `/api/config`，可另带 `todo_content` 字符串）；返回注入数据后的模板 HTML（不保存配置、不截图、不转灰度），供 Web 页 iframe 实时预览。数据走 60 秒缓存避免反复打外网；时钟/待办按请求值取当前 || `GET /api/templates` | `{templates: [{file, name, orientations}]}` |
| `GET /api/devices` | `{devices: [{key, name, w, h}]}`，Kindle 型号 -> 竖版分辨率表 |
| `GET /api/scan` | 局域网 Kindle 探测：读本机 ARP 表，按 MAC 厂商（内置 203 个 Amazon OUI 前缀）+ KOReader/USBNetwork 端口（2222/22，每台 2 个 TCP 包）综合判定，返回 `{devices: [{ip, mac, ports, guess}]}` |
| `GET /api/todo` / `POST /api/todo` | 待办读 / 整份替换（UTF-8），保存即刷新 |
| `GET /api/holidays` | 法定节日前 6 条 `{name, date, days}`，查内置农历表，无需联网 |
| `POST /api/refresh` | 立刻刷新 |
| `GET /` | webui.html（磁盘优先，缺失用内置；`Cache-Control: no-store`） |
| `/webui`、`/webui.html` | 302 → `/` |

静态文件只允许 `out/` 下 `png/html/json` 三类扩展名，路径含 `..` 直接 404
（避免把 `out/edge_profile` 浏览器配置暴露到局域网）。监听用 `socket2` 手动建
`SO_REUSEADDR`，避免重启被 TIME_WAIT 卡 30 秒~2 分钟。HTTP 为 4 线程小线程池：
`/api/preview` 同步抓数据可达数秒，单线程会把状态轮询和 Kindle 拉图一起卡住。

## 测试

```bash
cargo test            # 农历对齐 / 银行家舍入 / 日期不补零 / 配置字段级容错
```

移植自 Python 版时逐项对齐并留了回归测试：数值舍入与 Python `round()` 一致
（.5 取偶）、日期 `%d月%d日` 不补零、农历/节气与 lunar_python 一致、天气/一言/RSS
抓取逻辑逐项对齐（仅"一言"随机句与时钟本质不同）。改动行为时先跑测试。

## 构建与打包

```bash
cargo build --release        # 本机构建；Windows release 为 GUI 子系统（无控制台黑窗）
./make-app.sh                # macOS: "Kindle Dashboard.app"（LSUIElement，只驻留菜单栏）
./make-win.sh                # 交叉编译 Windows x64 zip（需 cargo install cargo-xwin；
                             #  TLS 走 schannel + 静态链接 CRT，目标机免运行库）
```

**serve/once 的控制台输出**（Windows 特有）：release 是 GUI 子系统，进程默认无标准
句柄。`attach_parent_console()` 先查 `GetStdHandle(STD_ERROR_HANDLE)`——已被重定向
（`cargo run`、`> 文件`）则不动，输出照常可捕获；无效才 `AttachConsole(父进程)` +
`SetStdHandle(CONOUT$)`。所以 `cargo run --release -- serve` 与直接双击 exe 都不异常。

## 本地调试

- 用 `KINDLE_DASHBOARD_HOME=/tmp/xx cargo run --release -- once` 可在隔离数据目录验证，
  不碰正式数据（首次运行会自动释放一套默认资源）。
- 无终端场景看 `out/app.log`；配置问题有 `warn_once` 单次告警。
- 端口被占时先 `netstat -ano | grep 8787` 找占用进程。
