#!/usr/bin/env bash
# 油烟机清洗验收系统 —— 一条命令启动
set -euo pipefail
cd "$(dirname "$0")"

# Rust 工具链（rustup 默认安装位置）
if ! command -v cargo >/dev/null 2>&1; then
  if [ -f "$HOME/.cargo/env" ]; then
    . "$HOME/.cargo/env"
  fi
fi

# 构建前端
if ! command -v npm >/dev/null 2>&1; then
  echo "错误：未找到 npm，请先安装 Node.js" >&2
  exit 1
fi
if [ ! -d frontend/node_modules ]; then
  echo "==> 安装前端依赖…"
  (cd frontend && npm install)
fi
echo "==> 构建前端…"
(cd frontend && npm run build)

mkdir -p uploads

echo "==> 启动后端（首次运行需编译，请稍候）…"
exec cargo run
