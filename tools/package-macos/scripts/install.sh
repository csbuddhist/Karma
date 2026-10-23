#!/bin/bash
# Installs the unsigned Karma macOS test bundle.
# 安装未签名的 Karma macOS 测试包。用法 / Usage: sudo ./install.sh
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "请使用 sudo 运行 / Run with sudo: sudo ./install.sh" >&2
  exit 1
fi

bundle="$(cd "$(dirname "$0")" && pwd)"
install_root="/Library/Karma"
console_app="Karma Family Protection.app"
daemon_plist="/Library/LaunchDaemons/com.karma.service.plist"
agent_plist="/Library/LaunchAgents/com.karma.agent.plist"

if [[ -e "$daemon_plist" || -e "$install_root" ]]; then
  echo "Karma 已安装；请先运行 sudo $install_root/uninstall.sh 再重新安装。" >&2
  echo "Karma is already installed; run sudo $install_root/uninstall.sh first." >&2
  exit 1
fi

echo "校验测试包 / Verifying the bundle…"
(cd "$bundle" && /usr/bin/shasum -a 256 -c SHA256SUMS >/dev/null)

for item in KarmaService KarmaControl KarmaAgent.app models uninstall.sh "$console_app" \
  launchd/com.karma.service.plist launchd/com.karma.agent.plist; do
  if [[ ! -e "$bundle/$item" ]]; then
    echo "测试包缺少 / The bundle is missing: $item" >&2
    exit 1
  fi
done

/bin/mkdir -p "$install_root"
for item in KarmaService KarmaControl KarmaAgent.app models uninstall.sh; do
  /usr/bin/ditto "$bundle/$item" "$install_root/$item"
done
/usr/bin/ditto "$bundle/$console_app" "/Applications/$console_app"

# Only root may replace the programs launchd starts as root and the agent
# executable KarmaService trusts.
/usr/sbin/chown -R root:wheel "$install_root" "/Applications/$console_app"
/bin/chmod -R go-w "$install_root" "/Applications/$console_app"
/bin/chmod 0755 "$install_root" "$install_root/KarmaService" "$install_root/KarmaControl" \
  "$install_root/uninstall.sh"
# An unsigned test build downloaded from the internet is quarantined, and
# Gatekeeper would refuse to let launchd start it.
/usr/bin/xattr -dr com.apple.quarantine "$install_root" "/Applications/$console_app" 2>/dev/null || true

/usr/bin/install -o root -g wheel -m 0644 "$bundle/launchd/com.karma.service.plist" "$daemon_plist"
/usr/bin/install -o root -g wheel -m 0644 "$bundle/launchd/com.karma.agent.plist" "$agent_plist"
/bin/mkdir -p /Library/Logs/Karma
/bin/chmod 0755 /Library/Logs/Karma

/bin/launchctl bootstrap system "$daemon_plist"
console_uid="$(/usr/bin/stat -f %u /dev/console)"
if [[ "$console_uid" -ge 501 ]]; then
  /bin/launchctl bootstrap "gui/$console_uid" "$agent_plist" || true
fi

cat <<'MESSAGE'
Karma 已安装。
1. 打开“应用程序”中的 Karma Family Protection，设置至少 10 个字符的管理员密码。
2. 在“系统设置 → 隐私与安全性 → 屏幕与系统录音”中允许 KarmaAgent。
3. 卸载：sudo /Library/Karma/uninstall.sh

Karma is installed.
1. Open Karma Family Protection from Applications and set an administrator password of at least 10 characters.
2. Allow KarmaAgent in System Settings → Privacy & Security → Screen & System Audio Recording.
3. To uninstall: sudo /Library/Karma/uninstall.sh
MESSAGE
