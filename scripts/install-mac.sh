#!/usr/bin/env bash
# scripts/install-mac.sh
# Build, bundle, and install Paper Shell.app locally on macOS.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

# Ensure we are on macOS
if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Error: scripts/install-mac.sh is only supported on macOS." >&2
  exit 1
fi

APP_NAME="Paper Shell.app"
DEST_DIR="/Applications"
BUILD_MODE="release"
ACTION_OPEN="auto" # "auto" = restart if previously running; "always" = open; "never" = do not open
SKIP_BUILD=0
FORCE_KILL=0
TARGET_TRIPLE=""

usage() {
  cat <<'EOF'
Usage: ./scripts/install-mac.sh [OPTIONS]

Build, bundle, and replace Paper Shell on this Mac.

Options:
  --release         Build and bundle with --release (default, optimized)
  --debug           Build and bundle in debug mode (faster compilation)
  --target TRIPLE   Specify cargo target triple (e.g. aarch64-apple-darwin)
  --user            Install to ~/Applications instead of /Applications
  --dest DIR        Install to custom directory (default: /Applications)
  --open, -o        Always open Paper Shell after installing
  --no-open         Never open Paper Shell after installing
  --restart, -r     Gracefully restart Paper Shell (or launch if not running)
  --skip-build      Skip building/bundling and install existing bundle
  --force, -f       Force-quit Paper Shell if it does not exit gracefully
  --help, -h        Show this help message

Examples:
  ./scripts/install-mac.sh              # Fast release bundle & replace in /Applications
  ./scripts/install-mac.sh --debug -r   # Debug build, replace, and restart app
  ./scripts/install-mac.sh --user       # Install to ~/Applications/Paper Shell.app
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release)
      BUILD_MODE="release"
      shift
      ;;
    --debug)
      BUILD_MODE="debug"
      shift
      ;;
    --target)
      if [[ $# -lt 2 ]]; then
        echo "Error: --target requires a target triple argument." >&2
        exit 1
      fi
      TARGET_TRIPLE="$2"
      shift 2
      ;;
    --user)
      DEST_DIR="${HOME}/Applications"
      shift
      ;;
    --dest)
      if [[ $# -lt 2 ]]; then
        echo "Error: --dest requires a directory argument." >&2
        exit 1
      fi
      DEST_DIR="$2"
      shift 2
      ;;
    --open|-o)
      ACTION_OPEN="always"
      shift
      ;;
    --no-open)
      ACTION_OPEN="never"
      shift
      ;;
    --restart|-r)
      ACTION_OPEN="always"
      shift
      ;;
    --skip-build)
      SKIP_BUILD=1
      shift
      ;;
    --force|-f)
      FORCE_KILL=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Error: Unknown option: $1" >&2
      echo "Run './scripts/install-mac.sh --help' for usage." >&2
      exit 1
      ;;
  esac
done

# Check prerequisites
if ! command -v cargo &>/dev/null; then
  echo "Error: 'cargo' is not installed or not in PATH." >&2
  exit 1
fi

if ! command -v cargo-bundle &>/dev/null; then
  echo "==> 'cargo-bundle' not found. Installing cargo-bundle..."
  cargo install cargo-bundle
fi

# Locate bundle output path
if [[ -n "${TARGET_TRIPLE}" ]]; then
  BUNDLE_DIR="target/${TARGET_TRIPLE}/${BUILD_MODE}/bundle/osx"
else
  BUNDLE_DIR="target/${BUILD_MODE}/bundle/osx"
fi
SOURCE_APP="${BUNDLE_DIR}/${APP_NAME}"
TARGET_APP="${DEST_DIR}/${APP_NAME}"

# Build & bundle if requested
if [[ "${SKIP_BUILD}" -eq 0 ]]; then
  echo "==> Bundling Paper Shell (${BUILD_MODE})..."
  BUNDLE_CMD=(cargo bundle)
  if [[ "${BUILD_MODE}" == "release" ]]; then
    BUNDLE_CMD+=(--release)
  fi
  if [[ -n "${TARGET_TRIPLE}" ]]; then
    BUNDLE_CMD+=(--target "${TARGET_TRIPLE}")
  fi
  BUNDLE_CMD+=(--format osx)

  "${BUNDLE_CMD[@]}"
fi

if [[ ! -d "${SOURCE_APP}" ]]; then
  echo "Error: Expected bundle not found at: ${SOURCE_APP}" >&2
  exit 1
fi

# Ad-hoc sign the newly bundled app so macOS Gatekeeper / ARM execution accepts it cleanly
echo "==> Signing bundle (ad-hoc)..."
codesign --force --deep --sign - "${SOURCE_APP}" >/dev/null 2>&1 || true

# Check if Paper Shell is currently running
WAS_RUNNING=0
RUNNING_PIDS=$(pgrep -f "${APP_NAME}/Contents/MacOS/paper-shell" || true)
if [[ -n "${RUNNING_PIDS}" ]]; then
  WAS_RUNNING=1
  echo "==> Paper Shell is currently running (PID: ${RUNNING_PIDS}). Quitting gracefully..."
  osascript -e 'tell application id "paper.shell.app" to quit' >/dev/null 2>&1 || true

  # Wait up to 5 seconds for it to exit
  for _ in {1..10}; do
    if ! pgrep -f "${APP_NAME}/Contents/MacOS/paper-shell" >/dev/null 2>&1; then
      break
    fi
    sleep 0.5
  done

  if pgrep -f "${APP_NAME}/Contents/MacOS/paper-shell" >/dev/null 2>&1; then
    if [[ "${FORCE_KILL}" -eq 1 ]]; then
      echo "==> Force quitting Paper Shell..."
      pkill -9 -f "${APP_NAME}/Contents/MacOS/paper-shell" || true
      sleep 0.5
    else
      echo "Warning: Paper Shell did not exit within timeout. It will be replaced on disk." >&2
    fi
  fi
fi

# Ensure destination directory exists
mkdir -p "${DEST_DIR}"

# Check write permissions for destination directory
USE_SUDO=""
if [[ ! -w "${DEST_DIR}" ]]; then
  if command -v sudo &>/dev/null; then
    echo "Notice: ${DEST_DIR} requires administrator privileges to write. Requesting sudo..."
    sudo -v
    USE_SUDO="sudo"
  else
    echo "Error: Cannot write to ${DEST_DIR} and sudo is unavailable. Try --user to install to ~/Applications." >&2
    exit 1
  fi
fi

# Perform safe replacement via temporary staging
echo "==> Installing to ${TARGET_APP}..."
STAGE_PARENT="${DEST_DIR}"
if [[ -n "${USE_SUDO}" ]]; then
  STAGE_DIR=$(${USE_SUDO} mktemp -d "${DEST_DIR}/.paper-shell-stage.XXXXXX")
else
  STAGE_DIR=$(mktemp -d "${DEST_DIR}/.paper-shell-stage.XXXXXX" 2>/dev/null || mktemp -d "/tmp/.paper-shell-stage.XXXXXX")
fi

cleanup_stage() {
  if [[ -d "${STAGE_DIR}" ]]; then
    ${USE_SUDO} rm -rf "${STAGE_DIR}" >/dev/null 2>&1 || true
  fi
}
trap cleanup_stage EXIT

# Copy to staging
if [[ -n "${USE_SUDO}" ]]; then
  ${USE_SUDO} ditto "${SOURCE_APP}" "${STAGE_DIR}/${APP_NAME}"
  ${USE_SUDO} xattr -r -d com.apple.quarantine "${STAGE_DIR}/${APP_NAME}" >/dev/null 2>&1 || true
else
  ditto "${SOURCE_APP}" "${STAGE_DIR}/${APP_NAME}"
  xattr -r -d com.apple.quarantine "${STAGE_DIR}/${APP_NAME}" >/dev/null 2>&1 || true
fi

# Atomic replacement
if [[ -e "${TARGET_APP}" ]]; then
  BACKUP_APP="${STAGE_DIR}/Paper Shell.previous.app"
  ${USE_SUDO} mv "${TARGET_APP}" "${BACKUP_APP}"
  if ! ${USE_SUDO} mv "${STAGE_DIR}/${APP_NAME}" "${TARGET_APP}"; then
    echo "Error: Failed to move new bundle into place. Restoring previous installation..." >&2
    ${USE_SUDO} mv "${BACKUP_APP}" "${TARGET_APP}"
    exit 1
  fi
else
  if ! ${USE_SUDO} mv "${STAGE_DIR}/${APP_NAME}" "${TARGET_APP}"; then
    echo "Error: Failed to move new bundle to ${TARGET_APP}." >&2
    exit 1
  fi
fi

# Notify LaunchServices
${USE_SUDO} touch "${TARGET_APP}"

echo "==> Successfully installed ${APP_NAME} to ${TARGET_APP}"

# Decide whether to launch
SHOULD_LAUNCH=0
if [[ "${ACTION_OPEN}" == "always" ]]; then
  SHOULD_LAUNCH=1
elif [[ "${ACTION_OPEN}" == "auto" && "${WAS_RUNNING}" -eq 1 ]]; then
  SHOULD_LAUNCH=1
fi

if [[ "${SHOULD_LAUNCH}" -eq 1 ]]; then
  echo "==> Launching Paper Shell..."
  open -a "${TARGET_APP}"
fi
