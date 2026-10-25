#!/bin/sh
# Name: Dashboard Update Now
# DontUseFBInk
# 立即从 PC 拉取一次 dashboard 图(手动测试/刷新用)

APP="/mnt/us/extensions/dashboard-updater"

if [ ! -f "$APP/bin/dashboard_updater.sh" ]; then
    exit 1
fi

exec sh "$APP/bin/dashboard_updater.sh" once
