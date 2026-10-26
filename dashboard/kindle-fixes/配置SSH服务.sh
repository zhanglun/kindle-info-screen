#!/bin/sh
# Name: 配置SSH服务
# Author: zhanglun
# 用 KOReader 自带的 patched dropbear 在 22 端口提供局域网 SSH。

BASE=/mnt/us/koreader
DB=$BASE/dropbear
HKDIR=$BASE/settings/SSH
HK=$HKDIR/dropbear_ed25519_host_key
PID=/tmp/dropbear_system.pid
LOG=/tmp/dropbear_system.log

say() { echo "$1"; }
listen22() {
  netstat -tln 2>/dev/null | grep -Eq '(^|:)22([[:space:]]|$)'
}

say "=== SSH 服务一键配置 v6 ==="

if [ ! -x "$DB" ]; then
  say "[错误] 未找到可执行文件: $DB"
  exit 1
fi

say "[1/5] 检查 SSH 主机密钥..."
mkdir -p "$HKDIR"
if [ ! -s "$HK" ]; then
  say "[错误] 缺少主机密钥: $HK"
  say "请先通过 USB 把项目中的 dropbear_ed25519_host_key 复制到该目录"
  exit 1
fi
chmod 600 "$HK" 2>/dev/null

say "[2/5] 部署 Mac 公钥..."
# 从 USB 分区已有的 authorized_keys 读取，避免 Kindle 脚本解释器的长字符串限制。
PUB=$(cat "$HKDIR/authorized_keys" 2>/dev/null)
if [ -z "$PUB" ]; then
  say "[错误] 未找到 Mac 公钥: $HKDIR/authorized_keys"
  exit 1
fi
for d in "$HKDIR" /home/root/.ssh /root/.ssh /var/root/.ssh /tmp/root/.ssh; do
  mkdir -p "$d" 2>/dev/null
  echo "$PUB" > "$d/authorized_keys"
  chmod 700 "$d" 2>/dev/null
  chmod 600 "$d/authorized_keys" 2>/dev/null
done

say "[3/5] 配置开机启动..."
mntroot rw >/dev/null 2>&1
cat > /etc/upstart/dropbear-system.conf << EOF
start on started filesystems_userstore
stop on stopping filesystems_userstore
script
    export HOME=/root
    export LD_LIBRARY_PATH=/mnt/us/koreader/libs
    cd /mnt/us/koreader
    iptables -I INPUT -p tcp --dport 22 -j ACCEPT 2>/dev/null
    [ -f /tmp/dropbear_system.pid ] && kill \$(cat /tmp/dropbear_system.pid) 2>/dev/null || true
    ./dropbear -E -p 22 -r settings/SSH/dropbear_ed25519_host_key -n -P /tmp/dropbear_system.pid >>/tmp/dropbear_system.log 2>&1
end script
EOF

say "[4/5] 启动 SSH 服务..."
killall dropbear 2>/dev/null || true
rm -f "$PID"
initctl stop dropbear-system >/dev/null 2>&1 || true
sleep 1
initctl start dropbear-system >/dev/null 2>&1 || true
sleep 3

# 某些固件上的 upstart 会报告 job failed，但 dropbear 本身可以直接启动；
# 这里直接回退，避免把 upstart 的兼容性问题误当成 SSH 本身失败。
if ! listen22; then
  say "upstart 未监听，尝试直接启动 dropbear..."
  cd "$BASE"
  export HOME=/root
  export LD_LIBRARY_PATH=/mnt/us/koreader/libs
  iptables -I INPUT -p tcp --dport 22 -j ACCEPT 2>/dev/null
  ./dropbear -E -p 22 -r settings/SSH/dropbear_ed25519_host_key -n -P "$PID" >>"$LOG" 2>&1 &
  sleep 3
fi

say "[5/5] 检查监听状态..."
if listen22; then
  say "OK! SSH 已在 22 端口运行"
  say "回电脑测试: ssh -i ~/.ssh/kindle_id_rsa root@<Kindle_IP>"
else
  say "!! SSH 启动失败，日志如下："
  tail -20 "$LOG" 2>/dev/null
  exit 1
fi
say "=== 完成 ==="
