#!/bin/bash
# 在 macOS 上交叉编译 Windows x64 exe 并打包 zip。
# 路线: cargo-xwin(MSVC 目标) + rust-lld + Windows SDK, TLS 走系统 schannel, 无需安装 mingw。
# 前置(仅需一次):
#   cargo install cargo-xwin
#   rustup target add x86_64-pc-windows-msvc
# 如在 Windows 上原生构建, 直接 cargo build --release 即可, 无需本脚本。
set -euo pipefail
cd "$(dirname "$0")"

TARGET=x86_64-pc-windows-msvc
rustup target add $TARGET >/dev/null

export CC_x86_64_pc_windows_msvc=clang
export CFLAGS_x86_64_pc_windows_msvc="--target=x86_64-pc-windows-msvc"
cargo-xwin build --target $TARGET --release

DIST=dist-windows
rm -rf "$DIST" && mkdir -p "$DIST"
cp "target/$TARGET/release/kindle-dashboard.exe" "$DIST/"
cat > "$DIST/README.txt" <<'EOF'
Kindle 信息屏 · Windows 版(绿色便携,无需安装)

1. 解压后双击 kindle-dashboard.exe
   - 首次运行会在 exe 旁自动创建 resources/ 数据目录(配置/模板/农历表)
   - 常驻系统托盘(右下角小报纸图标,右键菜单:在浏览器打开 Web 页 / 立刻刷新 / 退出)
2. Kindle 拉取地址: http://<本机局域网IP>:8787/dashboard.png
   (点托盘"在浏览器打开 Web 页"即可看到具体地址)
3. 修改内容:
   - 城市/刷新间隔/端口/RSS/倒数日/版面: resources/config.json
   - 待办清单: resources/todo.txt
   - 版面模板: resources/template*.html(config 的 template 字段选择)
4. 首次启动 Windows 防火墙弹窗请点"允许"(局域网供图需要)
5. 运行日志: resources/out/app.log

需要系统已安装 Edge 或 Chrome(截图引擎)。
EOF

ZIP="kindle-dashboard-windows-x64.zip"
rm -f "$ZIP"
(cd "$DIST" && zip -q -r "../$ZIP" .)
echo "打包完成: $ZIP ($(du -h "$ZIP" | cut -f1))"
echo "说明: macOS 交叉编译产物, 逻辑与 macOS 版一致; 建议在 Windows 上首次运行时确认托盘与防火墙放行"
