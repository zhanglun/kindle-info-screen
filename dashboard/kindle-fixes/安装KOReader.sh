#!/bin/sh
# Name: 安装KOReader
# Author: zhanglun
echo "=== 通过 KPM 安装 KOReader(自带SSH程序) ==="
KPM=/var/local/kmc/kindlehf/bin/kpm
if [ ! -x "$KPM" ]; then
  echo "[错误] 未找到 kpm: $KPM"
  echo "实际内容:"
  ls /var/local/kmc 2>/dev/null
  echo "把上面几行告诉电脑侧"
  exit 1
fi
echo "[1/2] 刷新软件源(需联网,稍等)..."
"$KPM" update
echo ""
echo "[2/2] 安装 KOReader(包较大,耐心等几分钟)..."
"$KPM" install koreader
echo ""
if [ -x /mnt/us/koreader/dropbear ]; then
  echo "OK! KOReader 已装好,SSH 程序就位"
  echo "下一步: 图书馆点《配置SSH服务》"
else
  echo "!! 未找到 /mnt/us/koreader/dropbear"
  echo "把上方报错内容告诉电脑侧"
fi
echo "=== 完成 ==="
