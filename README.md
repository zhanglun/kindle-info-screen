# Kindle 信息屏项目(Kindle Info Screen)

把 Kindle Paperwhite 10 变成一块墨水屏信息牌:定时显示日期、天气、倒数日、待办、RSS 资讯。

## 目录结构

```
kindle-info-screen/
├── dashboard/          ★ 信息屏渲染服务(常驻运行)
│   ├── dashboard.py    主程序:抓数据 → 渲染 HTML → Edge 截图 → HTTP 服务
│   ├── config.json     配置:城市、RSS源、倒数日、刷新间隔、端口
│   ├── template.html   版面模板(想改版式就改它)
│   ├── todo.txt        待办清单(每行一条,改完等下个刷新周期自动上屏)
│   └── out/            输出目录(dashboard.png 就是 Kindle 拉取的图)
└── jailbreak-tools/    越狱与安装工具(一次性工具,越狱完成后仅留档)
    ├── WinterBreak/    WinterBreak 越狱包(旧方法,仅支持 ≤5.18.0)
    ├── PEKI/           KUAL 启动器(装到 Kindle documents 文件夹)
    ├── kualmrpi/       MRInstaller(KUAL 扩展,装到 Kindle 根目录)
    ├── oss/            onlinescreensaver(信息屏核心扩展,装到 Kindle 根目录)
    ├── fill_kindle.ps1 存储填充脚本(防 OTA,越狱前用)
    ├── eject3.ps1      Kindle 安全弹出脚本
    ├── Update_hotfix_universal.bin  Hotfix 2.5.0(越狱后固化为防OTA)
    └── *.html/json     官方指南存档(kindlemodding.org)
```

## 常用操作

```bash
# 启动信息屏服务(渲染一次 + 每10分钟刷新 + 提供下载)
cd C:\Users\zhang\source\repos\zhanglun\kindle-info-screen\dashboard
python dashboard.py serve

# 只渲染一次(改了模板/配置后验证效果)
python dashboard.py once
```

- Kindle 取图地址:`http://192.168.1.4:8787/dashboard.png`(局域网)
- 改城市/RSS/倒数日:编辑 `dashboard/config.json`
- 改待办:编辑 `dashboard/todo.txt`
- 改版面:编辑 `dashboard/template.html` 后 `python dashboard.py once` 预览

## 设备当前状态(2026-10-25)

- 固件 5.18.1.1.1(WinterBreak 越狱曾成功,后被 OTA 抹除)
- 待执行:Sanctuary 越狱(官方支持 PW4 + 5.18.1.1.1,浏览器网页触发)
- 越狱成功后:装 Hotfix 防OTA → 拷入 PEKI/KUAL + onlinescreensaver → 配置拉图
