# Kindle 信息屏项目(Kindle Info Screen)

把 Kindle Paperwhite 10 变成一块墨水屏信息牌:定时渲染日期、天气、农历节气、倒数日、待办、RSS、一言;可开关 日程(ics)、热榜聚合、Bing 每日壁纸(相框模式)。六套版面(含横竖版),Web 页全程配置。

## 目录结构

```
kindle-info-screen/
├── dashboard/          ★ 信息屏渲染服务(Python 版,常驻运行)
│   ├── dashboard.py    主程序:抓数据 → 渲染 HTML → Edge 截图 → HTTP 服务
│   ├── config.json     配置:城市、RSS源、倒数日、刷新间隔、端口、Kindle地址
│   ├── template*.html  版面模板(template.html 为当前使用的版式)
│   ├── todo.txt        待办清单(每行一条,改完等下个刷新周期自动上屏)
│   ├── kindle-fixes/   已部署到 Kindle 的修复脚本留档(刷屏报错修复)
│   └── out/            输出目录(dashboard.png 就是 Kindle 拉取的图)
├── dashboard-rs/       ★ 桌面版(Rust 移植,托盘常驻,免 Python 环境)
│   ├── src/            抓数据/农历查表/渲染/HTTP 服务/托盘菜单
│   ├── resources/      内置资源:配置、模板、农历节气表(2024–2100)
│   ├── gen_tables.py   用 lunar_python 预生成农历/节气数据表
│   └── make-app.sh     打包 macOS .app(详见 dashboard-rs/README.md)
└── jailbreak-tools/    越狱与安装工具(一次性工具,越狱完成后仅留档)
    ├── WinterBreak/    WinterBreak 越狱包(旧方法,仅支持 ≤5.18.0)
    ├── PEKI/           KUAL 启动器(装到 Kindle documents 文件夹)
    ├── kualmrpi/       MRInstaller(KUAL 扩展,装到 Kindle 根目录)
    ├── oss/            onlinescreensaver(信息屏核心扩展,装到 Kindle 根目录)
    ├── Update_hotfix_universal.bin  Hotfix 2.5.0(越狱后固化为防OTA)
    └── *.html/json     官方指南存档(kindlemodding.org)
```

## 常用操作

```bash
# 方式一:桌面版(Rust,推荐)——托盘常驻,菜单可"打开 Web 页/立刻刷新/退出"
cd dashboard-rs
./make-app.sh && open "Kindle Dashboard.app"   # macOS 打包并启动
./make-win.sh                                   # 交叉编译 Windows x64 zip(需 cargo install cargo-xwin)
cargo run --release                            # 或开发模式直接跑

# 方式二:Python 版
cd dashboard
python dashboard.py serve    # 渲染一次 + 每10分钟刷新 + 提供下载
python dashboard.py once     # 只渲染一次(改了模板/配置后验证效果)
```

- 桌面版数据目录:`~/Library/Application Support/KindleDashboard`(macOS .app)或 exe 旁 `resources/`
- 桌面版配置管理:浏览器打开 `http://127.0.0.1:8787/` —— 切模板/横竖版/城市/RSS/倒数日/待办全在网页上完成,保存即上屏(Python 版同样支持该页面)
- Kindle 取图地址:`http://192.168.100.191:8787/dashboard.png`(局域网,以 Mac 当前 IP 为准,桌面版浏览器页面会显示)
- 改城市/RSS/倒数日:编辑对应数据目录的 `config.json`
- 改待办:编辑 `todo.txt`
- 改版面:编辑 `template*.html` 后等下个刷新周期(或点托盘"立刻刷新")
- 注意:两版默认端口都是 8787,同时只能跑一个

## 设备当前状态(2026-10-26)

- 固件 5.18.1.1.1,Sanctuary 越狱生效,已刷 Hotfix 防 OTA
- 已装 KUAL、MRInstaller、onlinescreensaver、custom-screensaver
- 信息屏已跑通:Kindle 定时从 Mac 拉图(白天每 10 分钟一次),无需数据线
- 设备端跑脚本用「书库脚本」机制:documents/ 里的 .sh 会显示为图书馆里的一本书,点开即运行;KPM 命令在主界面搜索栏输入(如 `;kpm install koreader`,英文半角分号)
- SSH 主动推送(渲染完立即通知 Kindle)的 Mac 端代码已就绪,打通步骤见 dashboard/kindle-fixes/README.md
