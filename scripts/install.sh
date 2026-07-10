#!/bin/sh
# Installer script for lembas CLI.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/lembas-project/lembas-cli/main/scripts/install.sh | sh
#
# Or with options:
#   ./install.sh --version v0.3.1 --install-dir /usr/local/bin

set -eu

# Wrap entire script to ensure complete download before execution
__wrap__() {

BINARY_NAME="lembas"
REPO="lembas-project/lembas-cli"

# Defaults
DEFAULT_INSTALL_DIR="$HOME/.local/bin"
DEFAULT_VERSION="latest"

usage() {
    local _display_dir
    _display_dir="$(echo "$DEFAULT_INSTALL_DIR" | sed "s|^$HOME|~|")"

    cat <<EOF
Usage: install.sh [OPTIONS]

Install the lembas CLI tool.

Options:
  -d, --install-dir DIR    Install directory (default: ${_display_dir})
  -v, --version VERSION    Version to install (default: ${DEFAULT_VERSION})
      --no-verify-checksum Skip checksum verification
  -f, --force              Overwrite existing installation without prompting
  -h, --help               Show this help message

Environment variables:
  LEMBAS_INSTALL_DIR       Same as --install-dir
  LEMBAS_VERSION           Same as --version
  LEMBAS_VERIFY_CHECKSUM   Set to "false" to skip checksum verification
  LEMBAS_FORCE_INSTALL     Set to non-empty to overwrite without prompting

Examples:
  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/scripts/install.sh | sh
  ./install.sh --version v0.3.1 --install-dir /usr/local/bin
EOF
}

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            -h|--help)
                usage
                exit 0
                ;;
            -d|--install-dir)
                [ $# -ge 2 ] || err "Missing argument for $1"
                LEMBAS_INSTALL_DIR="$2"
                shift 2
                ;;
            -v|--version)
                [ $# -ge 2 ] || err "Missing argument for $1"
                LEMBAS_VERSION="$2"
                shift 2
                ;;
            --no-verify-checksum)
                LEMBAS_VERIFY_CHECKSUM="false"
                shift
                ;;
            -f|--force)
                LEMBAS_FORCE_INSTALL="1"
                shift
                ;;
            -*)
                err "Unknown option: %s\nRun 'install.sh --help' for usage." "$1"
                ;;
            *)
                err "Unexpected argument: %s\nRun 'install.sh --help' for usage." "$1"
                ;;
        esac
    done
}

main() {
    parse_args "$@"

    ensure_cmd uname
    ensure_cmd chmod
    ensure_cmd mkdir

    local _os _arch _target
    _os="$(detect_os)"
    _arch="$(detect_arch)"
    _target="$(map_target "$_os" "$_arch")"

    local _version="${LEMBAS_VERSION:-$DEFAULT_VERSION}"
    local _install_dir="${LEMBAS_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"
    local _exe_suffix=""
    if [ "$_os" = "windows" ]; then
        _exe_suffix=".exe"
    fi
    local _asset_name="lembas-${_target}${_exe_suffix}"

    # Resolve version
    if [ "$_version" = "latest" ]; then
        _version="$(get_latest_version)"
    fi

    local _url="https://github.com/${REPO}/releases/download/${_version}/${_asset_name}"
    local _checksum_url="${_url}.sha256"

    info "Installing lembas %s for %s %s" "$_version" "$_os" "$_arch"

    local _dest="${_install_dir}/${BINARY_NAME}${_exe_suffix}"
    check_existing_install "$_dest"

    info "Downloading %s" "$_url"

    local _tmp
    _tmp="$(mktemp "${TMPDIR:-/tmp}/.lembas_install.XXXXXXXX")"
    trap 'rm -f "$_tmp"' EXIT

    download "$_url" "$_tmp"

    if [ ! -s "$_tmp" ]; then
        err "Downloaded file is empty. Check the URL or try again."
    fi

    verify_checksum "$_checksum_url" "$_tmp"

    install_binary "$_tmp" "$_install_dir" "$_exe_suffix"

    add_to_path "$_install_dir"

    printf "\nDone! Run 'lembas --help' to get started.\n"
}

detect_os() {
    local _os
    _os="$(uname -s)"
    case "$_os" in
        Linux)                    echo "linux" ;;
        Darwin)                   echo "darwin" ;;
        MINGW*|MSYS*|CYGWIN*)     echo "windows" ;;
        *)                        err "Unsupported operating system: %s" "$_os" ;;
    esac
}

detect_arch() {
    local _arch
    _arch="$(uname -m)"
    case "$_arch" in
        x86_64|amd64)   echo "x86_64" ;;
        aarch64|arm64)  echo "arm64" ;;
        *)              err "Unsupported architecture: %s" "$_arch" ;;
    esac
}

map_target() {
    local _os="$1" _arch="$2"
    case "${_os}-${_arch}" in
        linux-x86_64)    echo "linux-x86_64" ;;
        linux-arm64)     echo "linux-aarch64" ;;
        darwin-x86_64)   echo "darwin-x86_64" ;;
        darwin-arm64)    echo "darwin-arm64" ;;
        windows-x86_64)  echo "windows-x86_64" ;;
        *)               err "No prebuilt binary for %s %s" "$_os" "$_arch" ;;
    esac
}

get_latest_version() {
    local _url="https://api.github.com/repos/${REPO}/releases/latest"
    local _version

    if check_cmd curl; then
        _version="$(curl -fsSL "$_url" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')"
    elif check_cmd wget; then
        _version="$(wget -qO- "$_url" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')"
    else
        err "Need curl or wget to fetch latest version"
    fi

    if [ -z "$_version" ]; then
        err "Could not determine latest version"
    fi

    echo "$_version"
}

download() {
    local _url="$1" _dest="$2"

    if check_cmd curl; then
        curl -fSL --progress-bar "$_url" --output "$_dest" || {
            err "Download failed: %s" "$_url"
        }
    elif check_cmd wget; then
        wget --show-progress --output-document="$_dest" "$_url" || {
            err "Download failed: %s" "$_url"
        }
    else
        err "Need curl or wget to download files"
    fi
}

verify_checksum() {
    local _checksum_url="$1" _file="$2"
    local _verify="${LEMBAS_VERIFY_CHECKSUM:-true}"

    case "$_verify" in
        false|0)
            warn "Checksum verification disabled"
            return 0
            ;;
    esac

    info "Verifying checksum"

    local _expected _actual _tmp_sha
    _tmp_sha="$(mktemp "${TMPDIR:-/tmp}/.lembas_sha.XXXXXXXX")"
    if ! download "$_checksum_url" "$_tmp_sha" 2>/dev/null; then
        warn "Checksum file not available, skipping verification"
        rm -f "$_tmp_sha"
        return 0
    fi

    _expected="$(awk '{print $1}' "$_tmp_sha")"
    rm -f "$_tmp_sha"

    if check_cmd sha256sum; then
        _actual="$(sha256sum "$_file" | awk '{print $1}')"
    elif check_cmd shasum; then
        _actual="$(shasum -a 256 "$_file" | awk '{print $1}')"
    else
        warn "No sha256sum or shasum found, skipping verification"
        return 0
    fi

    if [ "$_expected" != "$_actual" ]; then
        err "Checksum mismatch!\n  expected: %s\n  actual:   %s" "$_expected" "$_actual"
    fi

    info "Checksum OK"
}

check_existing_install() {
    local _dest="$1"

    if [ -f "$_dest" ] && [ -z "${LEMBAS_FORCE_INSTALL:-}" ]; then
        if [ -t 0 ]; then
            printf "  %s already exists. Overwrite? [y/N] " "$_dest"
            read -r _reply
            case "$_reply" in
                [Yy]|[Yy][Ee][Ss]) ;;
                *) err "Installation cancelled." ;;
            esac
        else
            err "%s already exists. Use --force or LEMBAS_FORCE_INSTALL=1 to overwrite." "$_dest"
        fi
    fi
}

install_binary() {
    local _src="$1" _install_dir="$2" _exe_suffix="${3:-}"
    local _dest="${_install_dir}/${BINARY_NAME}${_exe_suffix}"

    chmod +x "$_src"
    mkdir -p "$_install_dir"
    mv -f "$_src" "$_dest"
    trap - EXIT

    info "Installed lembas to %s" "$_dest"
}

add_to_path() {
    local _dir="$1" _line

    # Already in PATH
    if echo "$PATH" | tr ':' '\n' | grep -qx "$_dir" 2>/dev/null; then
        return 0
    fi

    _line="export PATH=\"${_dir}:\$PATH\""

    case "$(basename "${SHELL:-}")" in
        bash)
            append_line_if_missing "$HOME/.bashrc" "$_line"
            ;;
        zsh)
            append_line_if_missing "$HOME/.zshrc" "$_line"
            ;;
        fish)
            _line="set -gx PATH \"${_dir}\" \$PATH"
            append_line_if_missing "$HOME/.config/fish/config.fish" "$_line"
            ;;
        *)
            warn "%s is not in your PATH." "$_dir"
            warn "Add it with:  %s" "$_line"
            return 0
            ;;
    esac
}

append_line_if_missing() {
    local _file="$1" _line="$2"

    if [ -f "$_file" ] && grep -Fxq "$_line" "$_file" 2>/dev/null; then
        return 0
    fi

    [ -f "$_file" ] || touch "$_file"

    printf '\n%s\n' "$_line" >> "$_file"
    info "Added %s to %s" "$_dir" "$_file"
}

check_cmd() {
    command -v "$1" >/dev/null 2>&1
}

ensure_cmd() {
    if ! check_cmd "$1"; then
        err "Required command not found: %s" "$1"
    fi
}

info() {
    local _fmt="$1"; shift
    # shellcheck disable=SC2059
    printf "> $_fmt\n" "$@"
}

warn() {
    local _fmt="$1"; shift
    # shellcheck disable=SC2059
    printf "! $_fmt\n" "$@" >&2
}

err() {
    local _fmt="$1"; shift
    # shellcheck disable=SC2059
    printf "x $_fmt\n" "$@" >&2
    exit 1
}

main "$@"
} && __wrap__ "$@"
