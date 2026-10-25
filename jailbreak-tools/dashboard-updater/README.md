# Dashboard 无线更新器（Kindle 端）

让 Kindle 通过 WiFi 自己从 PC 拉取最新的 `dashboard.png`，装好之后更新图片不再需要数据线。

```
PC (dashboard.py serve)                Kindle (越狱)
┌──────────────────────────┐   HTTP    ┌─────────────────────────────┐
│ 每 interval_min 分钟渲染   │ ◄─────── │ dashboard_updater.sh 守护    │
│ 并在 :8787 提供 PNG 下载   │  拉取     │ → /mnt/us/screensavers/…png │
└──────────────────────────┘           └──────────────┬──────────────┘
                                                      │ 睡眠事件
                                       custom-screensaver 用 FBInk 绘制
```

更新时机：**每次唤醒屏幕后几秒自动拉新图**（监听 `outOfScreenSaver` 事件，与
custom-screensaver 同款机制），外加亮屏期间每 10 分钟兜底轮询。Kindle 睡眠断网
时进程被冻结，不耗电；所以想让图变新，唤醒一下 Kindle 再让它睡下即可。

## 一次性安装（最后一次插线）

1. **PC**：运行 `python dashboard.py serve`，记下控制台打印的
   `Kindle 拉取地址: http://<IP>:8787/dashboard.png`。
2. **编辑** `extensions/dashboard-updater/bin/dashboard_url.conf`，
   把里面的 `192.168.1.100` 换成第 1 步拿到的 IP。
3. **USB 连接 Kindle**，把本目录里的内容拷过去（合并，不要替换）：
   - `extensions/dashboard-updater/` → Kindle 的 `extensions/`
   - `documents/` 下三个 `.sh` → Kindle 的 `documents/`
4. 安全弹出 USB。
5. 在 Kindle 主界面找到并点开 **Dashboard Updater On**（像开书一样打开），
   屏幕应显示 `Dashboard updater turned on`。
6. 点开 **Dashboard Update Now** 测试：屏幕显示 `Dashboard updated` 即成功；
   显示 `Update FAILED` 见下方排查。按睡眠键，屏幕应显示最新一版 dashboard。

## 日常使用

- PC 上跑着 `serve` 即可，Kindle 侧全自动，无需插线。
- 前提检查：`/mnt/us/screensavers/` 里**只保留一张 `dashboard.png`**——
  custom-screensaver 会轮换显示该目录下所有图片，混入别的图会轮流上屏。
- PC 建议在路由器里绑定静态 IP（DHCP 保留），否则换 IP 后要重新改 `dashboard_url.conf`
  （改完即生效，守护进程每轮拉取前都会重读该文件，不用重启）。
- Kindle 重启后守护不会自启，重新点开一次 **Dashboard Updater On**。
  （状态都存在 /tmp，重启即清空；如需开机自启可再加 upstart 任务。）
- 停止/卸载：点开 **Dashboard Updater Off**，或直接重启 Kindle。

## 排查

| 现象 | 检查 |
|---|---|
| `Update FAILED` | PC 的 `serve` 是否在跑；PC 和 Kindle 是否同一 WiFi；Windows 防火墙是否放行 Python/8787 端口（第一次启动 serve 时系统弹窗要点"允许"）；conf 里的 IP 是否正确 |
| 拉取成功但睡眠后不显示 | screensavers 目录是否有多张图在轮换；custom-screensaver 是否处于开启状态 |
| 看日志 | PC 上执行 `sh dashboard_updater.sh status` 没意义（这是 Kindle 侧脚本），日志在 Kindle 的 `extensions/dashboard-updater/updater.log`，可点开 `Dashboard Update Now` 后直接看屏幕提示，或下次插线时查看 |

PC 端唯一的改动：`serve` 启动时现在会多打印 Kindle 拉取地址（`lan_ip()`）。
