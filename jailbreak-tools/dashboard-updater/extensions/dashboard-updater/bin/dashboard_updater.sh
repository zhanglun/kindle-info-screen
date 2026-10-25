#!/bin/sh
# Dashboard updater: 从 PC 的 dashboard.py serve 拉取最新 dashboard.png
# 存到 /mnt/us/screensavers, 由 custom-screensaver 在睡眠时绘制
#
# 用法: sh dashboard_updater.sh [once|start|stop|status|__daemon]
#   once      立即拉取一次(手动测试用)
#   start     启动后台守护: 唤醒时立即拉取 + 每 PERIOD 秒兜底轮询
#   stop      停止后台守护
#   status    查看状态和最近日志
#   __daemon  (内部)守护进程主体, 不要手动调用
#
# Kindle 挂起时用户进程被冻结, 所以拉取实际只在亮屏期间发生:
# 每次唤醒(outOfScreenSaver)后等几秒 WiFi 就绪即拉新图, 睡下显示的就是最新版。

BIN="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
APP="$(dirname "$BIN")"
CONF="$BIN/dashboard_url.conf"
LOG="$APP/updater.log"

SS_DIR="/mnt/us/screensavers"
TARGET="$SS_DIR/dashboard.png"
TMP_PREFIX="$SS_DIR/.dashboard"
PIDFILE="/tmp/dashboard_updater.pid"

PERIOD=600        # 兜底轮询周期(秒), 挂起时冻结、亮屏时推进
WAKE_DELAY=3      # 唤醒事件后等 WiFi 重连的秒数
FETCH_TIMEOUT=8   # 单次下载超时(秒)
RETRIES=3         # 每轮拉取重试次数(应对唤醒后 WiFi 尚未就绪)
MIN_SIZE=10240    # 低于此字节数视为无效响应, 不落盘

# 复用 custom-screensaver 的 FBInk 在屏幕上显示状态; 没有则只写日志
FBINK="/mnt/us/extensions/custom-screensaver/bin/fbink_hf"

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') $1" >> "$LOG"
}

show_status() {
    if [ -f "$FBINK" ]; then
        if "$FBINK" -p -m -M -S 5 -B WHITE -C BLACK -w "$1" >>"$LOG" 2>&1; then
            sleep 2
            DISPLAY=:0 xrefresh >>"$LOG" 2>&1
        fi
    fi
}

# conf 里第一个非注释行即 URL, 每轮拉取前重新读取, 改完无需重启守护
load_url() {
    URL="$(sed -e 's/#.*$//' -e '/^[[:space:]]*$/d' "$CONF" 2>/dev/null |
        head -n 1 | tr -d '\r\n ')"
    [ -n "$URL" ]
}

fetch() {
    if ! load_url; then
        log "ERROR: no URL configured in $CONF"
        return 1
    fi
    if [ ! -d "$SS_DIR" ]; then
        log "ERROR: $SS_DIR missing"
        return 1
    fi

    # 每个调用者用独立临时文件(含 PID), 事件拉取与轮询并发也不会写坏同一文件;
    # 点开头的文件不会被 screensaver 的 * 通配符枚举, 半成品不会上屏
    TMP="$TMP_PREFIX.$$.png.tmp"

    rc=1
    if command -v wget >/dev/null 2>&1; then
        wget -q -T "$FETCH_TIMEOUT" -O "$TMP" "$URL"
        rc=$?
    elif command -v curl >/dev/null 2>&1; then
        curl -fsS -m "$FETCH_TIMEOUT" -o "$TMP" "$URL"
        rc=$?
    else
        log "ERROR: no wget or curl on device"
        return 1
    fi

    SIZE=0
    [ -f "$TMP" ] && SIZE="$(wc -c < "$TMP")"

    if [ "$rc" -ne 0 ] || [ "$SIZE" -lt "$MIN_SIZE" ]; then
        rm -f "$TMP"
        log "fetch failed rc=$rc size=$SIZE url=$URL"
        return 1
    fi

    mv -f "$TMP" "$TARGET"
    log "updated: $TARGET ($SIZE bytes)"
    return 0
}

fetch_retry() {
    n=0
    while [ "$n" -lt "$RETRIES" ]; do
        if fetch; then
            return 0
        fi
        n=$((n + 1))
        [ "$n" -lt "$RETRIES" ] && sleep 5
    done
    return 1
}

daemon_is_running() {
    [ -f "$PIDFILE" ] || return 1
    PID="$(cat "$PIDFILE" 2>/dev/null)"
    [ -n "$PID" ] || return 1
    kill -0 "$PID" 2>/dev/null || return 1
    # 确认 PID 属于本脚本, 而非复用了该 PID 的无关进程
    if [ -r "/proc/$PID/cmdline" ]; then
        tr '\0' ' ' < "/proc/$PID/cmdline" 2>/dev/null |
            grep -F "dashboard_updater.sh" >/dev/null 2>&1 || return 1
    fi
    return 0
}

run_daemon() {
    # 清掉上次异常退出留下的半成品临时文件
    rm -f "$TMP_PREFIX".*.tmp

    echo "$$" > "$PIDFILE"
    log "daemon started (PID $$, period ${PERIOD}s, wake-pull on)"

    # 唤醒即拉取: 与 custom_ss_daemon 同款 lipc 事件机制
    (
        while true; do
            lipc-wait-event -m com.lab126.powerd outOfScreenSaver >/dev/null 2>&1
            sleep "$WAKE_DELAY"
            fetch_retry
        done
    ) &
    EVENT_PID=$!

    # 兜底轮询: 亮屏期间每 PERIOD 秒刷新一次
    (
        while true; do
            fetch_retry || true
            sleep "$PERIOD"
        done
    ) &
    LOOP_PID=$!

    trap '
        kill "$EVENT_PID" "$LOOP_PID" 2>/dev/null
        rm -f "$PIDFILE"
        log "daemon stopped"
        exit 0
    ' TERM INT

    wait
}

start_daemon() {
    if daemon_is_running; then
        log "start: already running (PID $(cat "$PIDFILE"))"
        show_status "Dashboard updater
already running"
        return 0
    fi

    rm -f "$PIDFILE"
    log "starting daemon"

    sh "$BIN/dashboard_updater.sh" __daemon >/dev/null 2>&1 &

    sleep 1

    if daemon_is_running; then
        show_status "Dashboard updater
turned on"
        return 0
    fi

    log "ERROR: daemon failed to start"
    show_status "Could not start
updater. See logs."
    return 1
}

stop_daemon() {
    if daemon_is_running; then
        PID="$(cat "$PIDFILE")"
        kill "$PID" 2>/dev/null
        n=0
        while kill -0 "$PID" 2>/dev/null && [ "$n" -lt 5 ]; do
            sleep 1
            n=$((n + 1))
        done
        kill -0 "$PID" 2>/dev/null && kill -9 "$PID" 2>/dev/null
        rm -f "$PIDFILE"
        log "stopped"
        show_status "Dashboard updater
turned off"
    else
        rm -f "$PIDFILE"
        log "stop: not running"
        show_status "Dashboard updater
not running"
    fi
}

once() {
    rm -f "$TMP_PREFIX".*.tmp
    if fetch_retry; then
        show_status "Dashboard updated"
    else
        show_status "Update FAILED
Check PC server,
WiFi and firewall."
    fi
}

status_cmd() {
    if daemon_is_running; then
        echo "running (PID $(cat "$PIDFILE"))"
    else
        echo "not running"
    fi
    if load_url; then
        echo "url: $URL"
    else
        echo "url: (not configured)"
    fi
    tail -n 8 "$LOG" 2>/dev/null || true
}

case "${1:-}" in
    once) once ;;
    start) start_daemon ;;
    stop) stop_daemon ;;
    status) status_cmd ;;
    __daemon) run_daemon ;;
    *)
        echo "Usage: sh $0 [once|start|stop|status]" >&2
        exit 1
        ;;
esac
