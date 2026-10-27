# Kindle 信息屏 · 桌面版(Rust)

信息屏服务的唯一实现(原 Python 版 `dashboard/dashboard.py` 已删除,本版为其完全移植,
数据抓取、模板、渲染流程与 Python 版一致,移植时已逐字段校验)。一个程序,三种跑法:

| 跑法 | 命令 | 场景 |
|---|---|---|
| 托盘常驻(推荐) | `cargo run --release` 或双击 exe | 日常使用,菜单「打开 Web 页 / 立刻刷新 / 重新启动 / 退出」 |
| 前台服务 | `cargo run --release -- serve` | 终端里跑,日志直接打印,Ctrl+C 退出 |
| 单次渲染 | `cargo run --release -- once` | 改了模板/配置后验证效果,渲染完退出 |

```bash
cargo build --release        # 本机构建;产物 target/release/kindle-dashboard.exe(绿色便携)
./make-app.sh                # macOS: 打包 "Kindle Dashboard.app"
./make-win.sh                # 交叉编译 Windows x64 zip(需 cargo install cargo-xwin)
cargo test                   # 农历对齐/舍入/配置容错回归测试
```

- Windows:双击 exe 即常驻托盘,首次运行在 exe 旁自动创建 `resources/` 数据目录;
  防火墙首次弹窗需允许;渲染依赖本机 Microsoft Edge(自动探测常见安装路径)
- macOS:`make-app.sh` 产出 `.app`(LSUIElement,无 Dock 图标,只驻留菜单栏)

## 功能

- 启动即:渲染一次 → 每 N 分钟刷新(默认 10)→ HTTP 供 Kindle 拉图(默认端口 8787)
- 数据模块可开关:天气/RSS/一言/农历节气/倒数日 + 日程(ics)、热榜聚合、Bing 每日壁纸
- SSH 主动推送:渲染完成即通知 Kindle 拉图,睡眠不可达自动重试(需配 `kindle_host`)
- Web 配置页 `http://127.0.0.1:8787/`:模板/横竖版/城市/RSS/倒数日/待办,保存即上屏

## 文档

- **使用与启动说明**:[../USAGE.md](../USAGE.md) —— 数据目录、config.json 全字段、Kindle 对接、常见问题
- **架构设计**:[./ARCHITECTURE.md](./ARCHITECTURE.md) —— 系统上下文、组件/线程/模块关系图、核心流程图、数据契约、设计决策与扩展方案(Mermaid)
- **开发说明**:[./DEVELOPMENT.md](./DEVELOPMENT.md) —— 代码结构、渲染管线、资源升级机制、API 契约、模板开发、打包
- Kindle 端修复与 SSH 打通:[../kindle-fixes/README.md](../kindle-fixes/README.md)
