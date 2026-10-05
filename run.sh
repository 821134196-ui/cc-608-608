#!/usr/bin/env bash
# 家庭厨房油烟机清洗前后验收系统 —— 一条本地命令启动
#
# 用法：
#   ./run.sh            构建前端 + 启动后端（Axum 直接托管前端静态资源）
#   ./run.sh dev        前端 Vite 热更新(5173) + 后端(8080) 同时启动
#
# 首次运行会自动：安装 Rust 工具链（若缺失）、npm 安装、vite 构建、数据库迁移。
set -euo pipefail

cd "$(dirname "$0")"

PORT="${PORT:-8080}"

# ---------- 1. Rust 工具链 ----------
if ! command -v cargo >/dev/null 2>&1; then
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
  fi
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "[run.sh] 未检测到 Rust，正在通过 rustup 安装 stable 工具链…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

# ---------- 2. 前端依赖与构建 ----------
if [ ! -d frontend/node_modules ]; then
  echo "[run.sh] 安装前端依赖…"
  (cd frontend && npm install --no-audit --no-fund)
fi

MODE="${1:-serve}"
if [ "$MODE" = "dev" ]; then
  echo "[run.sh] 构建前端（dev 模式也先 build 一次以便 /api 联调参考）…"
  (cd frontend && npm run build)
  echo "[run.sh] 启动后端 http://127.0.0.1:${PORT} 与前端 Vite http://127.0.0.1:5173 …"
  (cd frontend && npm run dev) &
  VITE_PID=$!
  trap 'kill $VITE_PID 2>/dev/null || true' EXIT
  exec env PORT="$PORT" FRONTEND_DIST="$PWD/frontend/dist" cargo run
fi

echo "[run.sh] 构建前端生产包…"
(cd frontend && npm run build)

mkdir -p data/uploads
echo "[run.sh] 启动服务：浏览器打开 http://127.0.0.1:${PORT}"
echo "[run.sh] 数据库文件 data/hoodcare.db，照片目录 data/uploads/"
exec env PORT="$PORT" FRONTEND_DIST="$PWD/frontend/dist" cargo run
