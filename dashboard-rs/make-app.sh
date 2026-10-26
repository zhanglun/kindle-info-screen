#!/bin/bash
# 打包 macOS .app: cargo release 构建 + 组装 bundle。
# 双击运行后常驻菜单栏(无 Dock 图标), 数据目录 ~/Library/Application Support/KindleDashboard。
set -euo pipefail
cd "$(dirname "$0")"

cargo build --release

APP="Kindle Dashboard.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/kindle-dashboard "$APP/Contents/MacOS/"

cat > "$APP/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>kindle-dashboard</string>
  <key>CFBundleIdentifier</key><string>local.kindle-dashboard</string>
  <key>CFBundleName</key><string>Kindle Dashboard</string>
  <key>CFBundleDisplayName</key><string>Kindle 信息屏</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSUIElement</key><true/>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSLocalNetworkUsageDescription</key>
  <string>向局域网内的 Kindle 提供信息屏图片</string>
</dict>
</plist>
EOF

echo "打包完成: $APP"
echo "  双击运行 · 数据目录: ~/Library/Application Support/KindleDashboard"
