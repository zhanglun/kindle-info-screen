#!/bin/sh
# Name: Dashboard Updater On
# DontUseFBInk
# 启动后台更新守护: 唤醒时自动拉新图 + 每 10 分钟兜底轮询

APP="/mnt/us/extensions/dashboard-updater"

if [ ! -f "$APP/bin/dashboard_updater.sh" ]; then
    exit 1
fi

exec sh "$APP/bin/dashboard_updater.sh" start
