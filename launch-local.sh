#!/bin/bash
# screenpipe 本地自部署启动脚本（个人/非商用）
# 启动最新的已打包桌面 bundle，数据 100% 本地、不登录账号、不出现付费墙。
# bundle 缺失或源码发生变化时，会先自动重新打包。
# 用法: ./launch-local.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
APP_ROOT="$ROOT/apps/screenpipe-app-tauri"
BUNDLE_ROOT="$APP_ROOT/src-tauri/target/debug-dev/bundle/macos"
SOURCE_KEY_FILE="$APP_ROOT/src-tauri/target/debug-dev/.screenpipe-bundle-source-key"

die() {
  echo "错误：$*" >&2
  exit 1
}

latest_bundle() {
  local bundle
  find "$BUNDLE_ROOT" -maxdepth 1 -type d -name 'screenpipe*.app' -print 2>/dev/null \
    | while IFS= read -r bundle; do
        stat -f '%m %N' "$bundle"
      done \
    | sort -nr \
    | sed -n '1s/^[0-9]* //p'
}

source_key() {
  # Include tracked and non-ignored working-tree files that can affect the
  # frontend or native build. The key lives outside the app bundle so it does
  # not invalidate the bundle's code signature.
  (
    cd "$ROOT"
    files="$(git ls-files -co --exclude-standard -- \
      apps/screenpipe-app-tauri crates Cargo.toml Cargo.lock package.json bun.lock \
      | LC_ALL=C sort)"
    {
      printf '%s\n' "$files"
      printf '%s\n' "$files" | git hash-object --stdin-paths
    } | LC_ALL=C shasum -a 256 | LC_ALL=C awk '{print $1}'
  )
}

mkdir -p "$BUNDLE_ROOT"

SOURCE_KEY="$(source_key)"
BUILT_SOURCE_KEY=""
if [ -f "$SOURCE_KEY_FILE" ]; then
  BUILT_SOURCE_KEY="$(<"$SOURCE_KEY_FILE")"
fi

APP="$(latest_bundle || true)"
if [ -z "$APP" ] || [ "$SOURCE_KEY" != "$BUILT_SOURCE_KEY" ]; then
  command -v bun >/dev/null 2>&1 \
    || die "找不到 bun，无法构建最新 bundle。请先安装 Bun。"

  if [ -z "$APP" ]; then
    echo "未找到已打包 bundle，开始构建..."
  else
    echo "已打包 bundle 不是当前源码版本，开始重新构建..."
  fi

  # build:tauri:bundle uses the repository-wide native build queue and produces
  # an unsigned local app bundle. A local bundle does not need a signing
  # identity; use the dedicated signed build path when stable TCC identity is
  # required.
  (
    cd "$APP_ROOT"
    bun run build:tauri:bundle
  )

  APP="$(latest_bundle || true)"
  [ -n "$APP" ] || die "bundle 构建完成，但没有找到 macOS .app。"
  printf '%s\n' "$SOURCE_KEY" > "$SOURCE_KEY_FILE"
else
  echo "使用最新已打包 bundle：$APP"
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
echo "  停止:    Ctrl+C"
echo "  API 端口: 3130"
echo ""
exec "$APP/Contents/MacOS/screenpipe-app"
