# 架构设计（dashboard-rs）

本文是 dashboard-rs 的整体架构文档：系统上下文、组件与线程模型、模块依赖、核心流程、
数据契约、关键设计决策与扩展方案。全部图示为 Mermaid，GitHub/IDE 可直接渲染。
使用说明见 [USAGE.md](../USAGE.md)，开发速查见 [DEVELOPMENT.md](./DEVELOPMENT.md)。

---

## 1. 系统定位与上下文

一句话：**桌面电脑上的一个托盘常驻程序，周期性地把「天气 + RSS + 待办 + 倒数日 +
农历」等信息渲染成一张 Kindle 墨水屏尺寸的灰度 PNG，Kindle 屏保通过局域网 HTTP
拉取显示（可选 SSH 主动推送触发）。**

系统只有两个"端"：桌面服务进程（本仓库）与 Kindle 墨水屏（`kindle-fixes/` 提供的
online_screensaver / KOReader 配置）。桌面进程对外是「数据的消费者」（抓公网 API）
+「图片的提供者」（HTTP 供图）双重身份。

```mermaid
graph TB
    subgraph home["家庭局域网"]
        subgraph pc["桌面电脑 Windows / macOS"]
            APP["kindle-dashboard 进程<br/>托盘常驻 + HTTP 服务 + 定时渲染"]
        end
        subgraph kp["Kindle Paperwhite 4 墨水屏"]
            SS["online_screensaver / KOReader<br/>定时或唤醒后拉图"]
            SCR["墨水屏显示 dashboard.png"]
        end
    end

    subgraph cloud["公网数据源"]
        WX["Open-Meteo<br/>天气预报 / 空气质量 / 城市地理编码"]
        YY["hitokoto 一言"]
        RSS["RSS 订阅源 ×N"]
        ICS["日历 ics 订阅"]
        HOT["热榜 API<br/>GitHub / 微博 / B站"]
        BING["Bing 每日壁纸"]
    end

    USER["用户"]

    USER -->|"托盘菜单<br/>Web 配置页 http://127.0.0.1:8787"| APP
    APP -->|"HTTPS 抓取，各模块独立容错"| WX
    APP --> YY
    APP --> RSS
    APP --> ICS
    APP --> HOT
    APP --> BING
    APP -->|"HTTP 供图 GET /dashboard.png"| SS
    APP -.->|"SSH 触发 update.sh（可选，渲染完成即推）"| SS
    SS --> SCR
```

**三种运行模式**（同一二进制，按子命令区分）：

| 模式 | 命令 | 主线程去向 | 场景 |
|---|---|---|---|
| 托盘常驻（默认） | 无参数 / 双击 exe | winit 托盘事件循环 | 日常使用 |
| 前台服务 | `serve` | `park()` 挂起等 Ctrl+C | 终端/脚本跑，日志直出 |
| 单次渲染 | `once` | 渲染一次即退出（0/1） | 改模板/配置后验证 |

---

## 2. 总体架构（组件视图）

进程内是「一主三辅」四个执行流：主线程（托盘事件循环）、HTTP 线程（tiny_http 串行
accept）、渲染线程（周期渲染 + 信号唤醒）、推送线程（按需创建，渲染成功后触发）。
线程间通过一个 `Arc<Shared>` 状态块 + 一条 mpsc 通道协作，全部共享量都加了锁，
没有跨线程的裸引用。

```mermaid
graph TB
    subgraph proc["kindle-dashboard 进程"]
        TRAY["主线程 · main.rs<br/>winit 托盘事件循环<br/>打开Web页 / 立刻刷新 / 重新启动 / 退出"]
        SRV["HTTP 线程 · server.rs<br/>tiny_http 串行处理<br/>API + WebUI + 供图"]
        WORK["渲染线程 · render.rs<br/>立即渲染 → 周期睡眠(interval_min)<br/>收到信号提前唤醒<br/>catch_unwind 防崩溃退出"]
        PUSH["推送线程 · main.rs<br/>ssh update.sh<br/>30s 重试，PUSH_LOCK 单飞"]

        subgraph shared["Shared（Arc，跨线程共享）"]
            S1["port / lan_ip"]
            S2["last_render: Mutex&lt;String&gt;"]
            S3["refresh: Mutex&lt;Sender&lt;()&gt;&gt;"]
        end

        CH(["mpsc channel<br/>refresh 信号"])

        TRAY -->|"「立刻刷新」send()"| CH
        SRV -->|"POST /api/refresh<br/>保存配置/待办成功后"| CH
        CH -->|"rx.recv_timeout 唤醒"| WORK
        WORK -->|"渲染成功更新"| S2
        WORK -->|"渲染成功且开启推送"| PUSH
    end

    subgraph browser["子进程"]
        EDGE["Edge / Chrome 无头<br/>--headless --screenshot"]
        SSHC["ssh 命令行<br/>BatchMode + 专用私钥"]
    end

    subgraph disk["数据目录 base_dir/resources/"]
        CFG["config.json ●用户"]
        TODO["todo.txt ●用户"]
        TPL["template_*.html ×16"]
        CAL["calendar_table.json<br/>农历/节气表(2024~2101)"]
        WEBUI["webui.html"]
        OUT["out/<br/>dashboard.png · rendered.html<br/>wallpaper.png · app.log<br/>city_cache.json · edge_profile/"]
    end

    WORK -->|"config::load 每周期重读"| CFG
    WORK -->|"build_data 读待办"| TODO
    WORK -->|"读模板 + 注入数据"| TPL
    WORK -->|"农历/节气查询"| CAL
    WORK -->|"写入 HTML/PNG/日志"| OUT
    WORK -->|"file:// URL 截图"| EDGE
    EDGE -->|"dashboard.png 落盘"| OUT
    PUSH --> SSHC
    SRV -->|"读图供 Kindle"| OUT
    SRV -->|"GET/POST 读写"| CFG
    SRV -->|"读/整份替换"| TODO
    SRV -->|"磁盘优先，缺失回内置"| WEBUI
    SRV -->|"GET /api/holidays"| CAL
```

●用户 = 升级时永不覆盖的文件（详见 §8）。

### 职责一览

| 组件 | 代码 | 职责 | 不负责 |
|---|---|---|---|
| 入口/编排 | `main.rs` | 模式分发、资源释放与升级、线程编排、托盘 UI、SSH 推送、日志 `log_line`、数据目录定位 | 业务与渲染细节 |
| HTTP 服务 | `server.rs` | 全部路由（API/WebUI/供图）、`Shared` 状态定义、请求校验、静态文件白名单 | 主动渲染（只发信号） |
| 渲染 | `render.rs` | 模板扫描/白名单、数据注入、无头截图、灰度化 | 抓数据（委托 data） |
| 数据 | `data.rs` | HTTP 客户端、天气/空气/一言/RSS/待办/倒数日、组装模板 JSON | 模板与展示 |
| 扩展数据 | `extras.rs` | ics 日程、热榜聚合、Bing 壁纸（裁切 + Floyd–Steinberg 抖动 1-bit） | 核心字段 |
| 农历 | `lunar.rs` | 查表得农历/节气/月相、节假日兜底 | 联网（纯离线） |
| 配置 | `config.rs` | 字段级容错读取 + 默认值 | 写入（写由 server 做） |

---

## 3. 模块关系（src/ 依赖图）

分层清晰，自底向上：**基础层（config/lunar）→ 数据层（data/extras）→ 渲染层
（render）→ 接口层（server）→ 编排层（main）**。

```mermaid
graph BT
    CFGM["config.rs<br/>字段级容错配置"]
    LUNAR["lunar.rs<br/>农历/节气/月相查表"]
    EXTRA["extras.rs<br/>ics / 热榜 / 壁纸"]
    DATA["data.rs<br/>核心抓取与数据组装"]
    RENDER["render.rs<br/>模板注入 + 截图 + 灰度"]
    SERVER["server.rs<br/>HTTP 路由 + Shared"]
    MAIN["main.rs<br/>入口 / 线程 / 托盘 / 资源升级"]

    MAIN --> SERVER
    MAIN --> RENDER
    MAIN --> CFGM
    MAIN --> LUNAR
    SERVER --> RENDER
    SERVER --> CFGM
    SERVER --> LUNAR
    RENDER --> DATA
    RENDER --> CFGM
    DATA --> EXTRA
    DATA --> CFGM
    DATA --> LUNAR
    DATA --> RENDER
    EXTRA --> DATA
```

两处**局部互调**（均为单项函数借用，无初始化顺序问题，读代码时不要误以为是循环依赖）：

- `data ⇄ extras`：`data` 调 `extras::{fetch_schedule, fetch_hotlist, fetch_wallpaper}`；
  `extras` 只借用 `data::http_get`。
- `data → render`：仅借 `render::dimensions()`（壁纸按屏尺寸裁切）；`render` 调
  `data::build_data`。

另有一个"反向"依赖：`log_line` 定义在 `main.rs`，`data`/`render` 通过
`crate::log_line` 调用（stderr + `out/app.log` 双写）。

**外部依赖刻意少**：`tiny_http`（HTTP）、`reqwest`（抓取）、`image`（灰度/壁纸）、
`tray-icon` + `winit`（托盘）、`chrono`、`serde_json`、`feed-rs`（RSS 解析）、
`socket2`（SO_REUSEADDR）。TLS 按平台拆分：Windows 走 schannel（交叉编译免 C 工具链），
macOS/Linux 走 rustls。

---

## 4. 线程与并发模型

```mermaid
graph LR
    subgraph threads["执行流"]
        T1["主线程<br/>托盘事件循环 或 park"]
        T2["HTTP 线程<br/>tiny_http 串行 accept"]
        T3["渲染线程 ×1"]
        T4["推送线程 0..1<br/>PUSH_LOCK 保证同时最多一条"]
    end
    T1 -->|"tx.send(())"| Q(("mpsc<br/>channel"))
    T2 -->|"tx.send(())"| Q
    Q -->|"rx.recv_timeout(interval)"| T3
    T3 -.->|"渲染成功且开启推送时创建"| T4
```

| 同步点 | 机制 | 说明 |
|---|---|---|
| 渲染唤醒 | `mpsc::channel<()>` + `recv_timeout` | 定时到或收到信号都 break 出内层循环去渲染；通道断开则线程退出 |
| 共享状态 | `Arc<Shared>`，字段各自 `Mutex` | `last_render`（状态页展示）、`refresh`（Sender 装进 Mutex 以便多线程发送） |
| 推送单飞 | `OnceLock<Mutex<()>>` + `try_lock` | 同一时刻只有一条 SSH 重试链，后续渲染触发直接跳过 |
| 端口占用 | `socket2` 手动建 listener + `SO_REUSEADDR` | 避免重启被 TIME_WAIT 卡 30s~2min；托盘模式占用时弹系统对话框 |
| 渲染崩溃 | `catch_unwind(AssertUnwindSafe)` | 渲染线程内部 panic 只记日志，下个周期重试，服务不退 |

线程崩溃面收敛在渲染线程一处（截图/解析最不可控），HTTP 与主线程不执行抓取逻辑。

---

## 5. 核心流程

### 5.1 启动流程

```mermaid
flowchart TD
    S(["启动 exe"]) --> R1{"env KINDLE_DASHBOARD_RESTART<br/>已设?(托盘「重新启动」)"}
    R1 -->|"是"| W1["睡 1s 等旧实例退出"]
    R1 -->|"否"| M
    W1 --> M["解析子命令<br/>serve / once / 无参=托盘"]
    M -->|"未知参数"| E2["报错, 退出码 2"]
    M --> C1["base_dir() 定位数据目录:<br/>env KINDLE_DASHBOARD_HOME<br/>> macOS .app 内嵌运行时的<br/>~/Library/Application Support<br/>> exe 旁 resources/"]
    C1 --> C2["释放内置资源 + 版本升级<br/>详见 §8"]
    C2 --> C3["创建 out/ 目录"]
    C3 --> C4["config::load(字段级容错)<br/>lunar::Calendar::load(失败→空表,农历静默缺席)"]
    C4 --> D1{"mode"}
    D1 -->|"once"| D2["render_once 渲染一次<br/>成功退出 0 / 失败退出 1"]
    D1 -->|"serve / 托盘"| E1["server::start 启动 HTTP 线程<br/>端口被占: 托盘弹窗(fatal_notify)<br/>serve 直接退出码 1"]
    E1 --> E3["启动渲染线程:<br/>立即渲染一次 → 进入周期循环"]
    E3 --> F1{"mode"}
    F1 -->|"serve"| F2["主线程 park()<br/>等 Ctrl+C"]
    F1 -->|"托盘"| F3["winit 事件循环<br/>托盘菜单交互(macOS 为 Accessory, 无 Dock 图标)"]
```

要点：**配置与农历表在启动时只加载一次句柄，但配置内容每个渲染周期重读**——
Web 页改配置立即生效，无需重启；农历表缺失只降级不致命。

### 5.2 渲染管线（主流程）

```mermaid
flowchart TD
    T0(["触发: 周期到 / mpsc 信号唤醒 / once 模式"]) --> A1["config::load 重读配置"]
    A1 --> A2["data::build_data<br/>抓取数据并组装模板 JSON(§5.3)"]
    A2 --> A3{"cfg.template 在<br/>ALLOWED_TEMPLATES 白名单?"}
    A3 -->|"否"| A4["回退 template.html"]
    A3 -->|"是"| A5["读模板文本"]
    A4 --> A5
    A5 --> A6["字符串替换注入:<br/>/*__DATA__*/null ← 模板 JSON<br/>/*__ORIENTATION__*/portrait ← 朝向"]
    A6 --> A7["写 out/rendered.html<br/>转绝对路径 file:// URL"]
    A7 --> A8{"找到 Edge/Chrome?<br/>(BROWSER_CANDIDATES 探测)"}
    A8 -->|"否"| X1["报错返回"]
    A8 -->|"是"| A9["shoot() 无头截图(§5.4)"]
    A9 --> A10{"PNG > 10KB?"}
    A10 -->|"否"| X2["报错「截图失败」"]
    A10 -->|"是"| A11["image::to_luma8 灰度化<br/>原地回写 dashboard.png"]
    A11 --> A12["更新 Shared.last_render<br/>日志 rendered: N bytes"]
    A12 --> A13{"push_after_render<br/>且 kindle_host 已设?"}
    A13 -->|"是"| A14["起 SSH 推送线程(§5.6)"]
    A13 -->|"否"| A15(["本周期结束, 睡眠等下次触发"])
    A14 --> A15
```

截图尺寸随朝向：竖版 **1072×1448**，横版 **1448×1072**（PW4 面板像素）。

### 5.3 数据组装 `build_data`（模块开关与容错）

设计原则：**每个数据模块独立容错，单项失败只影响自己的字段**（记 `weather_error`
或留空），模板对 null/error 字段降级显示，整页永不空白。

```mermaid
flowchart TD
    B0(["build_data(base, cfg, cal)"]) --> B1["必出字段:<br/>date / weekday / time / year<br/>todo ← 读 todo.txt 按行"]
    B1 --> B2{"modules.weather?"}
    B2 -->|"开"| B3["geocode: city_cache.json<br/>缓存未命中才请求 Open-Meteo"]
    B3 --> B4["天气预报 5 天 + 日出日落<br/>必需字段缺失→整块失败"]
    B4 -->|"成功"| B5["data.weather = …<br/>空气质量另发一请求<br/>独立容错, 失败可缺席"]
    B4 -->|"失败"| B6["data.weather_error = 原因<br/>(超时/连接失败/HTTP状态…)"]
    B1 --> B7{"modules.rss?"}
    B7 -->|"开"| B8["逐源抓取 feed-rs 解析<br/>每源取 limit/N 条, 标题截 32 字<br/>单源失败显示「源暂时不可用」"]
    B1 --> B9{"modules.yiyan?"}
    B9 -->|"开"| B10["hitokoto 一言<br/>失败→字段缺席"]
    B1 --> B11{"modules.lunar?"}
    B11 -->|"开"| B12["查 calendar_table.json:<br/>lunar(农历) / jieqi(节气)<br/>moon(月相, 朔望月周期本地推算)"]
    B1 --> B13{"modules.countdowns?"}
    B13 -->|"开"| B14["倒数日 = 周末(欧几里得取余)<br/>+ 法定节日(农历表兜底, 取4条)<br/>+ 自定义项<br/>同名以法定为准, 按剩余天数排序"]
    B1 --> B15{"schedule_urls 非空?"}
    B15 -->|"是"| B16["extras::fetch_schedule<br/>只取今日事件, 全天排前"]
    B1 --> B17{"hotlist_sources 非空?"}
    B17 -->|"是"| B18["extras::fetch_hotlist<br/>github/weibo/bilibili 各取5条"]
    B1 --> B19{"wallpaper 开?"}
    B19 -->|"是"| B20["extras::fetch_wallpaper<br/>Bing 下载→cover裁切→FS抖动1-bit<br/>按天缓存 wallpaper.date<br/>失败仅记日志"]
```

模板 JSON（`__DATA__`）最终结构：

| 字段 | 类型 | 说明 |
|---|---|---|
| `date` `weekday` `time` `year` | string/int | 本地日期(`M月D日`不补零)/星期/`HH:MM`/年份 |
| `todo` | string[] | 待办非空行 |
| `weather` | object\|null | `city/temp/code/wc/wind/days[5]/sunrise/sunset`，可带 `aqi/aqi_desc/pm25` |
| `weather_error` | string? | 仅天气失败时存在 |
| `rss` | {src,title}[] | 每条含来源名 |
| `yiyan` | {text,from,who}? | 一言 |
| `lunar` / `jieqi` | string? | 农历文案 / 节气展示 |
| `moon` | {name,illum} | 月相名 + 照明率 |
| `countdowns` | {name,days,date,holiday?}[] | 已排序 |
| `schedule` | {title,all_day,time}[]? | 今日日程 |
| `hotlist` | {src,items[{title,hot}]}[]? | 热榜 |
| `wallpaper` | {file,credit}? | 相框模板用，图片在 `out/wallpaper.png` |

### 5.4 截图完成判定 `shoot()`

无头浏览器截图没有可靠的"完成回调"，采用的策略是**轮询 PNG 落盘尺寸稳定**：

```mermaid
flowchart TD
    S0(["shoot(browser, png, url, …)"]) --> S1["尝试 --headless=new"]
    S1 --> S2["删旧 PNG → 启动无头浏览器:<br/>--window-size=WxH --screenshot=png<br/>--virtual-time-budget=4000<br/>--default-background-color=FFFFFFFF<br/>独立 user-data-dir=out/edge_profile"]
    S2 --> S3["每 500ms 轮询"]
    S3 -->|"子进程已退出"| S5
    S3 -->|"PNG > 10KB 且连续两次<br/>采样尺寸不变 → 视为写完"| S5["kill + wait 强制收尾"]
    S3 -->|"90s 截止"| S5
    S5 --> S6{"PNG > 10KB?"}
    S6 -->|"是"| S7(["成功"])
    S6 -->|"否"| S8{"已用过回退<br/>--headless 老模式?"}
    S8 -->|"否"| S1
    S8 -->|"是"| S9(["失败 → render_once 报「截图失败」"])
```

### 5.5 Web 配置保存 → 上屏（端到端时序）

```mermaid
sequenceDiagram
    autonumber
    participant U as 浏览器 WebUI
    participant S as HTTP 线程
    participant F as config.json
    participant R as 渲染线程
    participant K as Kindle

    U->>S: GET /api/config
    S->>F: 读文件, 缺字段用内置默认补齐
    S-->>U: 配置 JSON
    U->>S: POST /api/config (编辑结果)
    S->>S: 校验 city非空 / port 1~65535<br/>interval≥1 / orientation 枚举
    S->>F: 与现有内容合并写回(保留未知字段)
    S->>R: mpsc refresh 信号
    S-->>U: {"ok":true}
    R->>F: 重读配置(下一周期被唤醒)
    Note over R: 抓数据 → 注入模板 → 截图 → 灰度
    K->>S: GET /dashboard.png (屏保轮询)
    S-->>K: 最新灰度 PNG
```

「保存即上屏」由 HTTP 线程直接发 refresh 信号实现，等待时间 = 一个渲染周期
（秒级），Kindle 侧最长再等一个屏保轮询间隔。

### 5.6 SSH 主动推送（可选加速）

Kindle 屏保是定时轮询拉图；若希望"渲染完成即上屏"，渲染线程成功后追加一步 SSH
触发 Kindle 端 `update.sh`（默认经 KOReader screensaver 的口令，端口 2222）：

```mermaid
flowchart TD
    P0(["渲染成功后(push_after_render=true)"]) --> P1{"config.kindle_host 已设?"}
    P1 -->|"否"| P2["日志: 跳过"]:::skip
    P1 -->|"是"| P3{"~/.ssh/kindle_id_rsa 存在?"}
    P3 -->|"否"| P2
    P3 -->|"是"| P4{"PUSH_LOCK 空闲?"}
    P4 -->|"占用"| P5["跳过: 已有一条重试链"]
    P4 -->|"空闲"| P6["ssh -p kindle_ssh_port<br/>-i 私钥 -o BatchMode<br/>-o StrictHostKeyChecking=accept-new<br/>-o PubkeyAcceptedAlgorithms=+ssh-rsa<br/>user@host 执行 kindle_update_cmd"]
    P6 --> P7{"成功?"}
    P7 -->|"是"| P8(["日志: 推送成功, 线程结束"])
    P7 -->|"否"| P9{"超过 push_retry_min 分钟?"}
    P9 -->|"否"| P10["睡 30s"] --> P6
    P9 -->|"是"| P11(["放弃: Kindle 睡眠不可达"])
    classDef skip fill:#eee,stroke:#999,stroke-dasharray: 3;
```

失败重试专为"Kindle 在睡眠"设计：渲染后立即推，设备醒来的窗口期内补上。

### 5.7 Kindle 拉图（被动通道）

`server.rs` 对 `GET /dashboard.png` 无任何特殊处理——它就是"静态文件只允许
`out/` 下 `png/html/json` 三类扩展名"这条白名单规则的普通命中。Kindle 端
（`kindle-fixes/`）的 online_screensaver 按自身节律向
`http://{lan_ip}:{port}/dashboard.png` 发起拉取；`lan_ip` 用
**UDP connect 探测**（朝 `kindle_host:2222` 或 `223.5.5.5:80` 连一次，读本地地址），
保证多网卡机器返回 Kindle 所在网段的 IP。

---

## 6. HTTP 接口与静态资源

```mermaid
graph TD
    REQ["tiny_http 请求"] --> API{"路径?"}
    API -->|"/api/status"| A1["lan_ip / port / last_render<br/>template / orientation"]
    API -->|"/api/config GET"| A2["config.json + 默认值补齐"]
    API -->|"/api/config POST"| A3["校验 → 合并保存 → 触发刷新"]
    API -->|"/api/todo GET/POST"| A4["读 / 整份替换, 保存触发刷新"]
    API -->|"/api/templates"| A5["白名单模板元数据<br/>file/name/orientations"]
    API -->|"/api/holidays"| A6["法定节日前6条<br/>查农历表, 无需联网"]
    API -->|"/api/refresh POST"| A7["触发立刻渲染"]
    API -->|"/ 或 /index.html"| A8["webui.html<br/>磁盘优先, no-store"]
    API -->|"/webui, /webui.html"| A9["302 → /"]
    API -->|"其他"| SEC{"安全检查"}
    SEC -->|"含 .. 或扩展名<br/>非 png/html/json"| D1["404"]
    SEC -->|"通过"| D2["out/ 下文件<br/>image/png 或 text/html"]
```

错误契约统一：`{"ok": false, "error": "…"}` + HTTP 400。完整字段说明见
[DEVELOPMENT.md](./DEVELOPMENT.md#http-api-契约)。WebUI（`resources/webui.html`）
是单文件原生 HTML+JS，与 API 的约定是"GET /api/config 返回的字段必然齐全"
（由服务端默认值补齐保证）。

---

## 7. 数据目录布局

`base_dir()` 解析优先级：`KINDLE_DASHBOARD_HOME` 环境变量 >（macOS `.app` 内嵌运行）
`~/Library/Application Support/KindleDashboard` > exe 旁 `resources/`（Windows 绿色
便携默认）。

| 文件 | 归属 | 说明 |
|---|---|---|
| `config.json` | ●用户数据 | 全部配置；缺字段自动回退默认 |
| `todo.txt` | ●用户数据 | 待办，`todo_file` 可改名 |
| `template_*.html` | 程序文件(可改) | 16 个模板，9 个在白名单内启用 |
| `calendar_table.json` | 程序文件 | `gen_tables.py` 预生成，2024-02 ~ 2101-01 |
| `webui.html` | 程序文件 | 配置页前端 |
| `resources.version` | 运行时 | 升级标记（RESOURCE_VERSION 快照） |
| `city_cache.json` | 运行时 | 城市地理编码缓存 |
| `out/dashboard.png` | 运行时 | 渲染产物（灰度），HTTP 供图对象 |
| `out/rendered.html` | 运行时 | 注入数据后的中间产物（截图输入） |
| `out/wallpaper.png` + `.date` | 运行时 | Bing 壁纸按天缓存 |
| `out/app.log` | 运行时 | 日志（stderr 同步双写） |
| `out/edge_profile/` | 运行时 | 无头浏览器独立 profile（**不在 HTTP 白名单内**） |

---

## 8. 资源嵌入与升级机制

所有默认资源经 `include_str!` 编译进二进制（单文件分发），首次运行释放到数据目录；
升级用 `RESOURCE_VERSION`（当前 "10"）与磁盘上的 `resources.version` 比对驱动：

```mermaid
flowchart TD
    U0(["每次启动都对全部内置资源逐项检查"]) --> U1{"resources.version<br/>== RESOURCE_VERSION ?"}
    U1 -->|"相等(常规启动)"| U2{"文件缺失?"}
    U1 -->|"不等(升级启动)"| U3{"是用户文件?<br/>config.json / todo.txt"}
    U3 -->|"是"| U2
    U3 -->|"否, 程序文件"| U4{"内容与内置不同?"}
    U4 -->|"是"| U5["覆盖为内置新版本<br/>日志: 升级刷新内置资源 X"]
    U4 -->|"否"| U2
    U5 --> U6["写回 resources.version"]
    U2 -->|"是"| U7["释放默认副本"]
    U2 -->|"否"| U8["不动(用户改动保留)"]
    U7 --> U9(["继续启动"])
    U8 --> U9
    U6 --> U9
```

两条铁律：**用户文件只建不盖**（改过的 config/todo 永远安全）；**程序文件升级即刷新**
（改过内置模板的用户需在升级后重新套用改动）。改了 `resources/` 里的内置文件必须
把 `main.rs` 的 `RESOURCE_VERSION` +1，否则已部署机器不会升级。

---

## 9. 关键设计决策（ADR 摘要）

| # | 决策 | 背景 / 动机 | 取舍与后果 |
|---|---|---|---|
| 1 | **无头浏览器截图当渲染引擎** | 想要 HTML/CSS 的全部排版能力而免自研排版库；模板即网页 | 依赖本机 Edge/Chrome；单帧约 1~3s；需 90s 超时兜底与完成判定（决策 5） |
| 2 | **模板 = 纯 HTML+JS，数据经 `/*__DATA__*/null` 字符串替换注入** | 零服务端模板引擎；模板文件可双击离线打开（占位 null）直接调试样式 | 模板消费的是一次性数据快照；循环/条件都在前端 JS 里写 |
| 3 | **数据模块独立容错** | 单一数据源故障不应让整页空白 | 模板必须处理 null/`weather_error`；天气的必需字段缺失仍整块失败，避免渲染残缺数据 |
| 4 | **配置字段级容错 + `warn_once`** | Web 页/手改都可能产生单字段手误 | 坏字段回退默认值并只告警一次；只有整份 JSON 不可解析才整体回退 |
| 5 | **截图完成 = PNG 落盘尺寸稳定（>10KB 且两次采样一致）** | headless 截图无完成回调 | 500ms 轮询，90s 强杀；`--headless=new` 失败自动退 `--headless` |
| 6 | **include_str! 单文件分发 + RESOURCE_VERSION 升级** | 绿色便携、双击即用；用户数据不能被升级覆盖 | 用户文件永不覆盖；改内置文件必须手动升版本号 |
| 7 | **HTTP 拉取为主，SSH 推送为辅** | 屏保轮询天然可靠；推送省电且即时 | 推送 30s 重试 + `push_retry_min` 放弃 + 单飞锁；未配 `kindle_host` 自动整体跳过 |
| 8 | **lan_ip 用 UDP connect 探测** | 多网卡机器固定公网探测地址可能选错网卡 | 优先朝 `kindle_host:2222` 连，取本地地址即 Kindle 同网段 IP |
| 9 | **socket2 手动建 listener + SO_REUSEADDR** | 重启被 TIME_WAIT 卡住数十秒到两分钟 | 与 Python 版 `allow_reuse_address` 行为对齐 |
| 10 | **农历/节气离线预生成表** | 节假日接口不稳定（被 Cloudflare 拦、年份覆盖不全） | `gen_tables.py`(lunar_python) 一次生成到 2101 年，节气精确到分钟；表损坏仅降级不致命 |
| 11 | **TLS 后端按平台拆分**（Windows schannel / 其余 rustls） | Windows 交叉编译（cargo-xwin）不需要构建 C 环 | 两套证书行为有细微差异，均接受系统信任库 |
| 12 | **逐项对齐 Python 版语义** | 本项目是已删除的 Python 版的完全移植 | 银行家舍入(`round_py`)、`%d月%d日`不补零、周末欧几里得取余等均有回归测试锁定 |

---

## 10. 约束与演进方向

当前约束（多为有意为之的单用户取舍）：

- `tiny_http` 单线程串行处理请求：单用户场景无压力，但并发拉图 + 保存配置会短暂排队。
- 抓取在渲染线程内**串行**执行（blocking client），一个刷新周期内数据源越多首帧越慢；
  好在周期默认 10 分钟，抓取耗时只影响当帧时间戳。
- `danger_accept_invalid_certs(true)`：为兼容证书不全的 RSS 源（沿袭 Python 版），
  局域网暴露面仅限 `out/` 白名单文件，风险可控但值得知晓。
- ics 解析为极简实现：不支持 RRULE，TZID 按本地墙钟、`Z` 后缀按 UTC+8 硬编码。
- `--virtual-time-budget=4000`：模板 JS 需在约 4s 虚拟时间内完成渲染，勿引入长轮询。
- 壁纸抖动、灰度均在主进程 CPU 完成，1072×1448 单帧耗时可忽略。

可行的演进方向（按性价比排序）：

1. **抓取并行化 + 失败沿用上次数据**：把 `build_data` 的各模块丢线程池并发，
   失败时读取上一帧数据缓存，消除网络抖动对当帧的影响。
2. **WebUI 状态推送**：`/api/status` 轮询换成 SSE，渲染完成实时刷新预览。
3. **渲染与抓取解耦**：数据缓存成 `data.json`，`once` 模式支持跳过抓取，加快模板调试。
4. **多屏 profile**：config 支持多组 (模板/朝向/端口)，一机供多块墨水屏。

---

## 11. 扩展指南

### 11.1 新增模板

1. `resources/` 放 `template_xx.html`，头部加自描述注释：
   `<!-- kindle-template: name=名称 orientations=portrait,landscape -->`；
2. `render.rs` 的 `ALLOWED_TEMPLATES` 白名单加入文件名（**两处缺一不可**，
   白名单外渲染回退 `template.html` 且 Web 页不显示）；
3. 数据从 `/*__DATA__*/null` 注入点的全局 `__DATA__` 读，处理 null 字段降级；
4. `main.rs` `RESOURCE_VERSION` +1（让已部署机器拿到模板）。

### 11.2 新增数据模块

1. `config.rs`：加字段与默认值（或复用 `modules` 开关，`module_on("xx")` 缺省为 true）；
2. 抓取函数放 `data.rs`（复用 `http_get`），失败返回 `Result` / `Option`，**不 panic**；
3. `build_data` 里按开关注入 `data["xx"]`；
4. 需要消费端默认值补齐的话同步改 `resources/config.json`（升 RESOURCE_VERSION）。

### 11.3 新增热榜源

`extras.rs` 加 `hot_xxx(c) -> Result<Vec<Value>, String>`（条目为 `{title, hot}`），
在 `fetch_hotlist` 的 `match` 加分支，`names::hotlist_name` 加中文名。三处齐活。

### 11.4 新增 API

`server.rs` 请求分发的 `match (method, path)` 加分支；遵守既有契约：JSON 返回、
出错 `{"ok":false,"error":…}` + 400、POST 用 `read_body` 限长（256KB）。

### 11.5 农历表再生成

```bash
python gen_tables.py    # 依赖 lunar_python，输出 resources/calendar_table.json
```

改完同样要升 `RESOURCE_VERSION`。表结构：`epoch`（农历 2024 正月初一的公历日期）+
`months`（每月天数与闰月标记）+ `terms`（公历年 → 节气名: MM-DDTHH:MM）。
