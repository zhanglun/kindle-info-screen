# Kindle 信息屏 · 桌面版(Rust)

Python 版 `dashboard/dashboard.py` 的 Rust 移植:一个托盘常驻小程序,启动即开始服务,
不再依赖 Python 环境。数据抓取、模板、渲染流程与 Python 版完全一致(农历/节气数据
由 `gen_tables.py` 用同一库 `lunar_python` 预生成,已逐字段校验一致)。

## 功能

- 启动后自动:渲染一次 Dashboard → 每 N 分钟刷新(默认 10)→ HTTP 供 Kindle 拉图
- **数据模块可开关**:天气/RSS/一言/农历节气/倒数日,另有 日程(ics 订阅)、热榜聚合(微博/B站/GitHub)、每日壁纸(Bing 抖动灰度)
- **倒数日**:法定节日(元旦/春节/清明/端午/中秋/劳动节/国庆)由内置农历节气表自动推算,无需手填;自定义倒数日在 Web 页添加并设日期
- 托盘菜单:
  - **在浏览器打开 Web 页** — 预览当前画面,页面上有 Kindle 拉取地址
  - **立刻刷新 Dashboard** — 不等下个周期,立即重渲染
  - **退出**
- 无 Dock 图标、无主窗口,只驻留菜单栏(macOS `.app` 为 LSUIElement)

## 构建与运行

```bash
cd dashboard-rs
cargo run --release          # 开发运行(数据目录: target/release/resources/)
./make-app.sh                # 打包 "Kindle Dashboard.app"(macOS)
./make-win.sh                # 交叉编译并打包 Windows x64 zip(需 cargo install cargo-xwin)
```

- **macOS**:`make-app.sh` 产出 `Kindle Dashboard.app`,双击运行,无 Dock 图标
- **Windows**:
  - macOS 上交叉编译:`./make-win.sh` 产出 `kindle-dashboard-windows-x64.zip`
    (前置 `cargo install cargo-xwin` + `rustup target add x86_64-pc-windows-msvc`;
     TLS 走 Windows 系统 schannel, 已静态链接 CRT, 目标机器无需装任何运行库)
  - 或在 Windows 上原生构建:`cargo build --release`,产物是绿色便携 exe
  - 双击 exe 即常驻系统托盘;首次运行在 exe 旁自动创建 `resources/` 数据目录
  - 启动失败(如端口被占)会弹出错误对话框;首次运行 Windows 防火墙弹窗需允许

## 数据目录

按优先级:环境变量 `KINDLE_DASHBOARD_HOME` > `.app` 方式运行时的
`~/Library/Application Support/KindleDashboard` > 可执行文件旁的 `resources/`。

首次运行自动释放默认文件(已存在则不覆盖,改动得以保留):

| 文件 | 说明 |
|---|---|
| `config.json` | 城市、刷新间隔、端口、RSS、倒数日、**template(选择模板,Python 版没有)** |
| `todo.txt` | 待办清单,改完等下个刷新周期自动上屏 |
| `template*.html` | 六套版面:信息日报 / 报纸头版 / 大字报极简 / 时刻表 / 相框(Bing 壁纸) / 专注大字钟 |
| `calendar_table.json` | 农历+节气数据表(2024–2100) |
| `out/dashboard.png` | Kindle 拉取的图;`out/app.log` 为运行日志 |

## Web 配置管理

浏览器打开 `http://127.0.0.1:8787/`(托盘"在浏览器打开 Web 页"即达)即可完成全部日常操作:
切换模板、横竖版、城市/间隔/端口、RSS 源、倒数日、待办编辑,保存即触发重渲染,
预览区实时显示当前画面。页面与 API 契约由 Python / Rust 两版共享(`webui.html` 同一份文件)。

API 契约(两版一致,便于脚本调用):

| 接口 | 说明 |
|---|---|
| `GET /api/status` | lan_ip / port / last_render / template / orientation |
| `GET /api/config` | 当前配置(缺字段用默认值补齐) |
| `POST /api/config` | 保存配置(校验+合并), 自动触发刷新 |
| `GET /api/templates` | 模板列表(名称/文件/支持的朝向, 来自模板头部 `kindle-template:` 注释) |
| `GET /api/todo` · `POST /api/todo` | 待办读写(整份替换) |
| `GET /api/holidays` | 法定节日(内置农历/节气表推算, 无需联网) |
| `POST /api/refresh` | 立刻刷新 |

新增模板:把 `template_xx.html` 放进数据目录,头部加一行
`<!-- kindle-template: name=名称 orientations=portrait,landscape -->`,Web 页自动出现。

## 与 Python 版的差异

- 换模板/横竖版:浏览器打开服务首页即可操作,或改 `config.json` 的 `"template"` / `"orientation"`
- 大字报极简与时刻表两套模板支持横版(1448×1072),其余模板仅竖版
- 升级程序会自动刷新内置模板/农历表/Web 页;`config.json` 与 `todo.txt` 是用户数据永不覆盖
- 农历/节气查表(由 `gen_tables.py` 生成),节气精确到分钟、与 lunar_python 时刻级判定一致
  (已用 800 个随机时刻验证),覆盖公历 2024-02 ~ 2101-01
- 配置字段级容错:单个字段写错只回退该字段默认值并记日志,不影响其余配置
- 数值舍入与 Python round() 一致(银行家舍入),日期格式不补零,均与 py 逐项对齐
- 天气/一言/RSS 抓取逻辑逐项对齐,已做 DATA JSON 奇偶校验(仅"一言"随机句与时钟不同)

## 重新生成农历表

```bash
../.venv/bin/python gen_tables.py   # 依赖 lunar_python,生成 resources/calendar_table.json
```
