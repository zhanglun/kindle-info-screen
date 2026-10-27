##############################################################################
# Logs a message to a log file (or to console if argument is /dev/stdout)

logger () {
	MSG=$1

	# do nothing if logging is not enabled
	if [ "x1" != "x$LOGGING" ]; then
		return
	fi

	# if no logfile is specified, set a default
	if [ -z "$LOGFILE" ]; then
		$LOGFILE=stdout
	fi

	echo `date`: $MSG >> $LOGFILE
}


##############################################################################
# Retrieves the current time in seconds

currentTime () {
	date +%s
}


##############################################################################
# runs when in the readyToSuspend state;
# sets the rtc to wake up
# arguments: $1 - amount of seconds to wake up in
# 【修复 lipc-set-prop: invalid option 刷屏】原脚本在等待超时前后触发
# readyToSuspend 时,剩余秒数可能是 0 或负数,负数会被 lipc 当成
# 非法选项打到屏幕上。这里先做判断:非正数直接跳过,并静默 lipc 错误输出。
set_rtc_wakeup()
{
	if [ "$1" -gt 0 ] 2>/dev/null; then
		lipc-set-prop -i com.lab126.powerd rtcWakeup $1 2>/dev/null
		logger "rtcWakeup has been set to $1"
	else
		logger "rtcWakeup skipped, non-positive delay: $1"
	fi
}

##############################################################################
# sets an RTC alarm
# arguments: $1 - time in seconds from now

wait_for () {
	ENDWAIT=$(( $(currentTime) + $1 ))
	REMAININGWAITTIME=$(( $ENDWAIT - $(currentTime) ))
	logger "Starting to wait for timeout to expire: $1"

	# wait for timeout to expire
	while [ $REMAININGWAITTIME -gt 0 ]; do
		EVENT=$(lipc-wait-event -s $1 com.lab126.powerd readyToSuspend,wakeupFromSuspend,resuming 2>/dev/null)
		REMAININGWAITTIME=$(( $ENDWAIT - $(currentTime) ))
		logger "Received event: $EVENT"

		case "$EVENT" in
			readyToSuspend*)
				set_rtc_wakeup $REMAININGWAITTIME
			;;
			wakeupFromSuspend*|resuming*)
				logger "Finishing the wait"
				break
			;;
			*)
				logger "Ignored event: $EVENT"
			;;
		esac
	done

	logger "Wait finished"
}
