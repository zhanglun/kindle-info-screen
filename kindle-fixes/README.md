# Kindle 端修复包(2026-10)

修复两个刷屏报错(已部署到设备),并预留 Mac 端 SSH 主动推送。三个文件直接覆盖到
`/mnt/us/extensions/onlinescreensaver/bin/` 即可(USB 连接时从 Finder 拖,
或终端 `cp`),覆盖前原文件无需备份(改动点都在注释里标了【修复】)。

## 修了什么

### 1. `ping: bad address www.baidu.com` 刷屏
- 原因:`update.sh` 的联网检测 ping `www.baidu.com`,stderr 没有丢弃;
  Kindle 的 DNS 偶尔抽风时,`bad address` 错误每秒打一条到墨水屏。
- 修复:
  - `config.sh`:`TEST_DOMAIN` 从 `www.baidu.com` 改为 `192.168.100.191`(Mac 的
    局域网 IP)。图本来就是从这台电脑拉的,ping 通它就等于网络可用,彻底不依赖 DNS。
  - `update.sh`:ping 加 `2>&1`,失败静默。
- 附带:WiFi 状态检测(`lipc-get-prop`)加了错误重定向和字符串比较,
  避免 lipc 偶发报错时把 `LIPC:Error` / `illegal number` 打到屏幕。

### 2. `lipc-set-prop: invalid option ==1` 刷屏
- 原因:`utils.sh` 的 `set_rtc_wakeup()` 在等带唤醒事件时,如果
  `readyToSuspend` 在等待截止后才到,剩余秒数为 0 或负数,
  `lipc-set-prop ... rtcWakeup -1` 把 `-1` 当成命令行选项报错。
- 修复:`set_rtc_wakeup()` 先判断秒数 > 0 才调用 lipc,且静默 lipc 错误输出。

## 注意
- `config.sh` 里写死了当前网络环境:
  - Mac IP `192.168.100.191`(en1 网卡)
  - 图片地址 `http://192.168.100.191:8787/dashboard.png`
- 换网络/路由器后若 IP 变了,要同步改 `config.sh` 的
  `IMAGE_URI` 和 `TEST_DOMAIN`(建议在路由器上给两台设备绑定静态 IP)。

## 配套:KOReader SSH 与主动推送
KOReader 已安装到 `/mnt/us/koreader`，并自带 `SSH.koplugin` 与 Dropbear。
不要运行旧的「配置SSH服务」脚本，也不要手动写 Upstart 或监听 22 端口；SSH 由
KOReader 自己管理，默认端口是 **2222**。

1. Mac 通过 USB 把 `启动KOReader.sh` 复制到 Kindle `documents/`，推出并拔线。
2. 在 Kindle 书库点开「启动KOReader」。
3. KOReader 菜单打开 `SSH server`，端口保持 `2222`；建议打开
   `Login with key only`，不要打开 `Login without password`。
4. 把 Mac 公钥放到 KOReader 提示的路径：
   `/mnt/us/koreader/settings/SSH/authorized_keys`。
5. 用 KOReader 弹窗显示的 Kindle IP 测试：

```bash
ssh -p 2222 -i ~/.ssh/kindle_id_rsa \\
  -o IdentitiesOnly=yes -o PubkeyAcceptedAlgorithms=+ssh-rsa \\
  root@<kindle-ip> 'echo SSH_OK; id'
```

确认手动连接成功后，数据目录 `config.json` 中的 `kindle_host` 改为弹窗 IP，
保留 `kindle_ssh_port: 2222`，最后再将 `push_after_render` 改为 `true` 并重启服务。
Kindle 睡眠时 SSH 不可达，程序会按 `push_retry_min` 重试。
