#!/bin/bash
# screenpipe 本地自部署启动脚本（个人/非商用）
# 启动自构建的桌面 app，数据 100% 本地、不登录账号、不出现付费墙。
# 用法: ./launch-local.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
APP="$ROOT/apps/screenpipe-app-tauri/src-tauri/target/debug-dev/bundle/macos/screenpipe - Development.app"

if [ ! -d "$APP" ]; then
  echo "错误：找不到自构建的 app：$APP" >&2
  echo "请先运行构建（见 README 或 build-local.sh）。" >&2
  exit 1
fi

# 隔离的数据目录：与正式版 (~/.screenpipe) 分开，避免读到已登录账号
# 从而跳过登录 → onboarding 不含 plan/付费墙步骤
export SCREENPIPE_DEV_USE_PROD_DATA="${SCREENPIPE_DEV_USE_PROD_DATA:-0}"

# 若之前登录过账号，引导用户选择：登录会触发付费墙，本地使用请保持隔离
if [ -f "$HOME/.screenpipe-dev" ] 2>/dev/null; then :; fi
if [ ! -d "$HOME/.screenpipe-dev" ]; then
  echo "首次使用：数据会保存在 ~/.screenpipe-dev（与正式版分开，本地私有）。"
fi

echo "启动 screenpipe（本地模式，未登录账号，无付费墙）..."
echo "  App:     $APP"
echo "  数据:    ~/.screenpipe-dev"
echo "  停止:    Ctrl+C（或杀掉 screenpipe-app 进程）"
echo "  API 端口: 3130"
echo ""
"$APP/Contents/MacOS/screenpipe-app" &
APP_PID=$!
echo "PID: $APP_PID"
trap 'echo "停止中..."; kill $APP_PID 2>/dev/null || true' INT TERM EXIT
wait $APP_PID