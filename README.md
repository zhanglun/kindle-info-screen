# Kindle 信息屏项目(Kindle Info Screen)

把 Kindle Paperwhite 10 变成一块墨水屏信息牌:定时渲染日期、天气、农历节气、倒数日、待办、RSS、一言;可开关 日程(ics)、热榜聚合、Bing 每日壁纸(相框模式)。九套版面分属三个设计流派(报纸/状态优先/相框艺术,均含横竖版),Web 页全程配置。

## 文档索引

| 文档 | 内容 |
|---|---|
| [USAGE.md](USAGE.md) | **启动与使用说明**:三种启动方式、开机自启、config.json 全字段、Kindle 对接、常见问题 |
| [dashboard-rs/DEVELOPMENT.md](dashboard-rs/DEVELOPMENT.md) | **开发说明**:代码结构、启动流程、渲染管线、资源升级机制、API 契约、模板开发、测试与打包 |
| [dashboard-rs/README.md](dashboard-rs/README.md) | 桌面版简介与快速上手 |
| [PRODUCT.md](PRODUCT.md) | 产品定义与能力版图 |
| [DESIGN.md](DESIGN.md) | Web 配置页设计系统(车站翻牌视觉) |
| [kindle-fixes/README.md](kindle-fixes/README.md) | Kindle 端修复脚本留档与 SSH 推送打通步骤 |

## 目录结构

```
kindle-info-screen/
├── dashboard-rs/       ★ 信息屏渲染服务(Rust,唯一实现;原 Python 版已删除)
│   ├── src/            抓数据/农历查表/渲染/HTTP 服务/托盘菜单
│   ├── resources/      内置资源:配置、模板、农历节气表(2024–2100)
│   ├── gen_tables.py   用 lunar_python 预生成农历/节气数据表(工具脚本,非服务)
│   └── make-app.sh     打包 macOS .app / make-win.sh 交叉编译 Windows(详见 dashboard-rs/README.md)
├── kindle-fixes/       已部署到 Kindle 的修复脚本留档(SSH 推送、KOReader、刷屏修复)
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
cd dashboard-rs
cargo run --release                    # 托盘常驻(推荐日常),菜单可"打开 Web 页/立刻刷新/退出"
cargo run --release -- serve           # 终端前台起服务,Ctrl+C 退出(日志直接打印,不进托盘)
cargo run --release -- once            # 只渲染一次(改了模板/配置后验证效果)
./make-app.sh                          # 打包 macOS "Kindle Dashboard.app"
./make-win.sh                          # 交叉编译 Windows x64 zip(需 cargo install cargo-xwin)
```

完整的启动说明、开机自启、config.json 字段参考、Kindle 对接与常见问题见 [USAGE.md](USAGE.md);开发相关见 [dashboard-rs/DEVELOPMENT.md](dashboard-rs/DEVELOPMENT.md)。

- 数据目录:exe 旁 `resources/`(Windows/Linux);macOS .app 为 `~/Library/Application Support/KindleDashboard`;可用环境变量 `KINDLE_DASHBOARD_HOME` 另指定
- 配置管理:浏览器打开 `http://127.0.0.1:8787/` —— 切模板/横竖版/城市/RSS/倒数日/待办全在网页上完成,保存即上屏
- Kindle 取图地址:`http://<本机局域网IP>:8787/dashboard.png`(启动日志与 Web 页会显示实际地址)
- 改城市/RSS/倒数日:Web 页,或直接编辑数据目录的 `config.json`
- 改待办:编辑数据目录的 `todo.txt`,或 Web 页
- 改版面:编辑 `template*.html` 后等下个刷新周期(或点托盘"立刻刷新")
- 注意:默认端口 8787,同一台机器同时只能跑一个实例

## 设备当前状态(2026-10-26)

- 固件 5.18.1.1.1,Sanctuary 越狱生效,已刷 Hotfix 防 OTA
- 已装 KUAL、MRInstaller、onlinescreensaver、custom-screensaver
- 信息屏已跑通:Kindle 定时从主机拉图(白天每 10 分钟一次),无需数据线
- 设备端跑脚本用「书库脚本」机制:documents/ 里的 .sh 会显示为图书馆里的一本书,点开即运行;KPM 命令在主界面搜索栏输入(如 `;kpm install koreader`,英文半角分号)
- SSH 主动推送(渲染完立即通知 Kindle)已内置:config.json 设 `kindle_host` 即生效,打通步骤见 kindle-fixes/README.md
