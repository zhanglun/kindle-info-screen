# 使用与启动说明

信息屏服务的唯一实现是 Rust 版（`dashboard-rs/`，原 Python 版已删除）。
一个程序，三种跑法，全部功能一致。

## 一、启动方式

在 `dashboard-rs/` 目录下：

| 跑法 | 命令 | 场景 |
|---|---|---|
| 托盘常驻（推荐） | `cargo run --release` 或直接双击 exe | 日常使用：系统托盘菜单「打开 Web 页 / 立刻刷新 / 重新启动 / 退出」 |
| 前台服务 | `cargo run --release -- serve` | 终端里跑，日志直接打印，`Ctrl+C` 退出；适合脚本/SSH/服务器场景 |
| 单次渲染 | `cargo run --release -- once` | 改了模板/配置后验证效果，渲染完自动退出 |

直接用编译好的 exe（免 Cargo）：

```bash
./target/release/kindle-dashboard.exe serve   # 前台服务
./target/release/kindle-dashboard.exe once    # 只渲染一次
./target/release/kindle-dashboard.exe         # 托盘常驻
```

启动成功的标志（终端或 `out/app.log`）：

```
服务已启动: http://0.0.0.0:8787/dashboard.png  每 10 分钟刷新
Kindle 拉取地址: http://<本机局域网IP>:8787/dashboard.png
配置管理页面:   http://127.0.0.1:8787/
```

注意：

- 默认端口 **8787**，同一台机器同时只能跑一个实例；端口被占会弹错误对话框（serve 模式则日志报错退出，可用 `config.json` 的 `port` 换端口）
- Windows 首次运行会弹防火墙授权，必须「允许访问」，否则 Kindle 拉不到图
- release 版无控制台黑窗；`serve` 模式从终端启动会自动附着终端打印日志
- Windows 渲染依赖本机安装 Microsoft Edge（常见安装路径已自动探测）

### 开机自启（可选）

- **Windows**：`Win+R` 输入 `shell:startup`，把 exe 的快捷方式放进去（托盘模式）；
  想隐藏窗口跑 serve 模式则用「任务计划程序」建开机任务
- **macOS**：系统设置 → 登录项，加入 `Kindle Dashboard.app`

## 二、数据目录

按优先级：环境变量 `KINDLE_DASHBOARD_HOME` > macOS `.app` 的
`~/Library/Application Support/KindleDashboard` > exe 旁的 `resources/`。

首次启动自动释放默认文件（已存在则不覆盖，用户改动得以保留）：

| 文件 | 说明 |
|---|---|
| `config.json` | 全部配置（字段见下节） |
| `todo.txt` | 待办清单，每行一条，改完等下个刷新周期自动上屏 |
| `template*.html` | 版面模板 |
| `calendar_table.json` | 农历+节气数据表（2024–2100） |
| `webui.html` | Web 配置页（缺失时用程序内置的） |
| `out/dashboard.png` | Kindle 拉取的图 |
| `out/app.log` | 运行日志（排查问题先看这里） |

## 三、配置参考（config.json）

日常配置全部可以在 Web 页完成，以下是手工编辑时的字段说明：

| 字段 | 类型/默认 | 说明 |
|---|---|---|
| `city` | 字符串，`杭州` | 天气城市（自动地理编码） |
| `template` | `template.html` | 模板文件名（可选值见 `/api/templates`） |
| `orientation` | `portrait` / `landscape` | 竖版 / 横版（分辨率随 `device` 型号互换宽高） |
| `device` | `pw4` | Kindle 型号，决定渲染分辨率（可选值见 `/api/devices`） |
| `interval_min` | `10` | 刷新间隔（分钟，≥1） |
| `port` | `8787` | HTTP 服务端口（1–65535） |
| `rss_sources` | `[{"name","url"}]` | RSS 订阅源 |
| `rss_count` | `6`（内置配置为 4） | RSS 显示条数 |
| `countdowns` | `[{"name","date"}]` | 自定义倒数日；法定节日自动推算，无需手填 |
| `todo_file` | `"todo.txt"` | 待办文件名 |
| `modules` | `{}` | 模块开关：`weather` / `rss` / `yiyan` / `lunar` / `countdowns`，缺省全开 |
| `schedule_urls` | `["https://…ics"]` | 日程 ics 订阅，非空即启用 |
| `hotlist_sources` | `["github","weibo","bilibili"]` | 热榜聚合，非空即启用 |
| `frame_mode` | `moon` / `wallpaper` | 相框夜画画面:月相(默认)或 Bing 每日一图。Bing 下载直连不走系统代理, 按天缓存 |
| `kindle_host` | 空 = 关闭推送 | Kindle 的 IP；设置后渲染完成即 SSH 通知拉图 |
| `kindle_user` | `root` | SSH 用户名 |
| `kindle_ssh_port` | `2222` | SSH 端口（KOReader Dropbear 默认 2222） |
| `kindle_update_cmd` | `sh /mnt/us/…/update.sh` | 推送时在 Kindle 上执行的命令 |
| `push_after_render` | `false` | 渲染后主动推送开关（需先配好 SSH 私钥） |
| `push_retry_min` | `10` | Kindle 睡眠不可达时的重试窗口（分钟） |

字段级容错：单个字段缺失/写错只回退该字段默认值并记一次日志，不影响其余配置。

## 四、Web 配置页

浏览器打开 `http://127.0.0.1:8787/`：在顶部「SERVICE / 版面」牌上点选模板、在「朝向」
行点竖版/横版，**即自动保存并渲染上屏**，右侧预览约十几秒后随之更新；城市/RSS/倒数日/
待办等表单改完点「刷新上屏」（或「仅保存」不渲染）。API 契约见
[dashboard-rs/DEVELOPMENT.md](dashboard-rs/DEVELOPMENT.md)。

## 五、Kindle 端对接

1. **拉图地址**：Kindle 端
   `/mnt/us/extensions/onlinescreensaver/bin/config.sh` 里的 `IMAGE_URI` 和
   `TEST_DOMAIN` 指向运行服务的电脑 IP（换电脑/换网络后必须同步修改，建议在路由器
   给两台设备绑定静态 IP）。详见 [kindle-fixes/README.md](kindle-fixes/README.md)。
2. **SSH 主动推送（可选，渲染完立即上屏）**：生成密钥对，把公钥放到 Kindle 的
   `/mnt/us/koreader/settings/SSH/authorized_keys`，私钥命名
   `kindle_id_rsa` 放在 `~/.ssh/` 下，然后 `config.json` 设 `kindle_host` 为
   Kindle IP、`push_after_render` 为 `true`。打通步骤见
   [kindle-fixes/README.md](kindle-fixes/README.md)。

## 六、常见问题

| 现象 | 排查 |
|---|---|
| 启动即报「端口监听失败」 | 8787 已被占：找占用进程结束它，或改 `config.json` 的 `port` |
| Kindle 拉不到图 | 先看防火墙是否放行；再在 Kindle 端 `config.sh` 确认 `IMAGE_URI` 指向本机当前 IP；浏览器先开 `http://本机IP:8787/dashboard.png` 自测 |
| 画面不更新 | `out/app.log` 看渲染日志；改模板/配置后可 Web 页点「立刻刷新」或跑一次 `once` |
| 「未找到 Edge/Chrome 浏览器」 | 安装 Microsoft Edge，或确认安装路径在常见位置 |
| 农历/节气不显示 | `calendar_table.json` 缺失或损坏，日志有提示；从 `dashboard-rs/resources/` 复制一份 |
| SSH 推送没发生 | 日志会写明原因：未设 `kindle_host` / 找不到 `~/.ssh/kindle_id_rsa` / 10 分钟内不可达 |
| Web 页打开是旧样式 | `webui.html` 是磁盘优先，升级内置页面后删掉数据目录里的旧文件即可 |
