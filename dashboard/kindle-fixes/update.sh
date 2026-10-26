#!/bin/sh
#
##############################################################################
#
# Fetch weather screensaver from a configurable URL.

# change to directory of this script
cd "$(dirname "$0")"

# load configuration
if [ -e "config.sh" ]; then
	source ./config.sh
else
	TMPFILE=/tmp/tmp.onlinescreensaver.png
fi

# load utils
if [ -e "utils.sh" ]; then
	source ./utils.sh
else
	echo "Could not find utils.sh in `pwd`"
	exit
fi

# do nothing if no URL is set
if [ -z "$IMAGE_URI" ]; then
	logger "No image URL has been set. Please edit config.sh."
	return
fi

# enable wireless if it is currently off
# 【修复刷屏】错误输出重定向 + 字符串比较,避免 lipc 偶发报错时把
# "LIPC:Error..." 和 "illegal number" 打到墨水屏上
WIRELESS_STATE=`lipc-get-prop com.lab126.cmd wirelessEnable 2>/dev/null`
if [ "x0" = "x$WIRELESS_STATE" ]; then
	logger "WiFi is off, turning it on now"
	lipc-set-prop com.lab126.cmd wirelessEnable 1
	DISABLE_WIFI=1
fi

# wait for network to be up
TIMER=${NETWORK_TIMEOUT}     # number of seconds to attempt a connection
CONNECTED=0                  # whether we are currently connected
while [ 0 -eq $CONNECTED ]; do
	# test whether we can ping outside
	# 【修复 ping: bad address 刷屏】stderr 也丢弃,DNS 失败时不再打到屏幕
	/bin/ping -c 1 "$TEST_DOMAIN" > /dev/null 2>&1 && CONNECTED=1

	# if we can't, checkout timeout or sleep for 1s
	if [ 0 -eq $CONNECTED ]; then
		TIMER=$(($TIMER-1))
		if [ 0 -eq $TIMER ]; then
			logger "No internet connection after ${NETWORK_TIMEOUT} seconds, aborting."
			break
		else
			sleep 1
		fi
	fi
done

if [ 1 -eq $CONNECTED ]; then
	if curl -kl $IMAGE_URI -o $TMPFILE; then
		# 【修复书库点脚本时的 mv 属主警告】跨分区移动到 FAT32 时 mv 会尝试
		# 保留属主并打 "can't preserve ownership" 警告,文件实际移动成功,静默即可
		mv $TMPFILE $SCREENSAVERFILE 2>/dev/null
		logger "Screen saver image file updated"
                # refresh screen
                lipc-get-prop com.lab126.powerd status 2>/dev/null | grep "Screen Saver" && (
                     logger "Updating image on screen"
                     # 【修复刷屏】eips 自身的信息输出("update mode=FULL..." /
                     # "swipe feature is not supported")会被 upstart 打上墨水屏,静默
                     eips -f -g $SCREENSAVERFILE > /dev/null 2>&1
                     batt=`powerd_test -s | awk -F: '/Battery Level: / {print $2}' | awk -F' |%' '{print $2}'`
# Create json for POST
generate_post_data()
{
  cat<<EOF
{
  "kindle_battery":"$batt"
}
EOF
}
                     # If WEBHOOKADR has been defined, send data
                     if [ -n "$WEBHOOKADR" ]; then
                       curl -X POST -k -d "$(generate_post_data)" -H 'Content-Type: application/json' $WEBHOOKADR
                     fi
                     # 【修复刷屏】原脚本在角落画 "Batt:xx%",但 eips 字体没有 %
                     # 字符,每次报 "character '%' not available";电池若要上屏,
                     # 以后走 webhook 让 Mac 渲染进图里,这里直接去掉
                )
	else
		logger "Error updating screensaver"
		if [ 1 -eq $DONOTRETRY ]; then
			touch $SCREENSAVERFILE
		fi
	fi
fi

# disable wireless if necessary
if [ 1 -eq $DISABLE_WIFI ]; then
	logger "Disabling WiFi"
	lipc-set-prop com.lab126.cmd wirelessEnable 0
fi
