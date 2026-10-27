#!/bin/sh
# Run the community daemon as the Upstart-owned foreground process.
BASE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"

mkdir -p /tmp/custom-ss || exit 1
cp -f "$BASE/bin/fbink_hf" /tmp/custom-ss-fbink || exit 1
cp -f "$BASE/bin/screensaver_shield" /tmp/custom-ss-shield || exit 1
chmod +x /tmp/custom-ss-fbink /tmp/custom-ss-shield || exit 1

exec /bin/sh "$BASE/custom_ss_daemon.sh"
