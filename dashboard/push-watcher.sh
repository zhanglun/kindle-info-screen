#!/bin/sh
# Kindle 推送守护:监听 dashboard.png 变化,变了就 SSH 通知 Kindle 立即拉图。
# 与渲染程序解耦(Rust 版/Python 版通用),Kindle 睡眠时自动重试,最多 10 分钟。
PNG="$HOME/Library/Application Support/KindleDashboard/out/dashboard.png"
KEY="$HOME/.ssh/kindle_id_rsa"
HOST="root@192.168.97.155"
CMD="sh /mnt/us/extensions/onlinescreensaver/bin/update.sh"
LOCK=/tmp/kindle-pushing.lock
LAST=0
echo "$(date '+%F %H:%M:%S') 推送守护启动,监听: $PNG"
while true; do
  M=$(stat -f %m "$PNG" 2>/dev/null || echo 0)
  if [ "$M" != "$LAST" ] && [ "$M" != "0" ] && [ "$LAST" != "0" ] && [ ! -e "$LOCK" ]; then
    (
      touch "$LOCK"
      N=0
      while [ $N -lt 20 ]; do
        if ssh -p 2222 -i "$KEY" -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=5 \
             -o StrictHostKeyChecking=accept-new -o PubkeyAcceptedAlgorithms=+ssh-rsa \
             "$HOST" "$CMD" >/dev/null 2>&1; then
          echo "$(date '+%F %H:%M:%S') push 成功"
          break
        fi
        N=$((N+1)); sleep 30
      done
      [ $N -ge 20 ] && echo "$(date '+%F %H:%M:%S') push 放弃(Kindle 10分钟不可达,醒来后按书库『立即更新信息屏』)"
      rm -f "$LOCK"
    ) &
  fi
  LAST=$M
  sleep 15
done
