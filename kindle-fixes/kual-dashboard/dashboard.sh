#!/bin/sh
# KUAL 统一信息屏入口。复杂逻辑复用现有官方扩展，避免复制两套实现。
BASE=/mnt/us/extensions
OSS=$BASE/onlinescreensaver
CSS=$BASE/custom-screensaver

case "$1" in
  update)
    # 不调用 eips 文字提示，避免覆盖/清空正在显示的自定义屏保。
    /bin/sh "$OSS/bin/update.sh"
    ;;
  enable)
    /bin/sh "$CSS/autostart_enable.sh"
    ;;
  disable)
    /bin/sh "$CSS/autostart_disable.sh"
    ;;
  autostart)
    # 先安装两个持久化 job，再启动一次；顺序避免更新脚本与自定义渲染器抢 framebuffer。
    /bin/sh "$CSS/autostart_enable.sh" || exit 1
    /bin/sh "$OSS/bin/enable.sh" || exit 1
    ;;
  *)
    echo "Usage: $0 {update|enable|disable|autostart}" >&2
    exit 1
    ;;
esac
