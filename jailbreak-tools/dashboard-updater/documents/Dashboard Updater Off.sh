#!/bin/sh
# Name: Dashboard Updater Off
# DontUseFBInk
# 停止后台更新守护(重启 Kindle 后也需要重新点 On)

APP="/mnt/us/extensions/dashboard-updater"

if [ ! -f "$APP/bin/dashboard_updater.sh" ]; then
    exit 1
fi

exec sh "$APP/bin/dashboard_updater.sh" stop
