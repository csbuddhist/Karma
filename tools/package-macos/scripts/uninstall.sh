#!/bin/bash
# Uninstalls Karma after KarmaService authorizes it.
# 经 KarmaService 授权后卸载 Karma。用法 / Usage: sudo /Library/Karma/uninstall.sh [--purge-data]
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "请使用 sudo 运行 / Run with sudo: sudo /Library/Karma/uninstall.sh" >&2
  exit 1
fi

purge=false
case "${1:-}" in
  "") ;;
  --purge-data) purge=true ;;
  *)
    echo "用法 / Usage: sudo /Library/Karma/uninstall.sh [--purge-data]" >&2
    exit 2
    ;;
esac

install_root="/Library/Karma"
control="$install_root/KarmaControl"

# The super password, or the administrator password once a requested
# uninstall waiting period has elapsed (the administrator password when no
# super password is configured).
read -r -s -p "超级密码（或等待期满后的管理员密码）/ Super password (or the administrator password after the waiting period): " password
echo
if ! printf '%s\n' "$password" | "$control" shutdown; then
  unset password
  echo "授权失败或服务无法连接，卸载已取消。/ Authorization failed or the service is unreachable; uninstall cancelled." >&2
  exit 3
fi
unset password

# The authorized service exits by itself and launchd does not restart a
# successful exit; remove both jobs from launchd.
/bin/launchctl bootout system/com.karma.service 2>/dev/null || true
for uid in $(/bin/ps -axo uid=,comm= | /usr/bin/awk '$2 ~ /karma-agent-macos$/ { print $1 }' | /usr/bin/sort -u) \
  "$(/usr/bin/stat -f %u /dev/console)"; do
  if [[ "$uid" -ge 501 ]]; then
    /bin/launchctl bootout "gui/$uid/com.karma.agent" 2>/dev/null || true
  fi
done

/bin/rm -f /Library/LaunchDaemons/com.karma.service.plist /Library/LaunchAgents/com.karma.agent.plist
/bin/rm -rf "/Applications/Karma Family Protection.app" "$install_root" /var/run/karma

if [[ "$purge" == true ]]; then
  /bin/rm -rf "/Library/Application Support/Karma" /Library/Logs/Karma
  echo "Karma 及其策略、审计记录和加密证据已删除。/ Karma and its policy, audit log, and encrypted evidence were removed."
else
  echo "Karma 已卸载；/Library/Application Support/Karma 中的策略和加密证据已保留（--purge-data 可删除）。"
  echo "Karma was uninstalled; policy and encrypted evidence in /Library/Application Support/Karma were kept (remove them with --purge-data)."
fi
