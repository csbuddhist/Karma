#!/bin/bash
# macOS version of TryYourBest.bat: pull, commit all changes, push.
# Double-click in Finder or run ./TryYourBest.command in Terminal.

# Finder starts .command files in the home directory; switch to the repository.
cd "$(dirname "$0")" || exit 1

git pull
git add -A
git commit -m "add new content"
git push

read -n 1 -s -r -p "按任意键继续..."
echo
