#!/bin/sh
# The official Grove installer
# Supports Linux, macOS, all major architectures.
# 
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/ZiiMs/Grove/main/install.sh | bash
#   
# Options:
#   --bin-dir DIR    Override installation directory (default: ~/.local/bin)
#   --version VER    Install specific version (default: latest)
#   --no-deps        Skip dependency installation (tmux)
#   --help           Show this help message

set -e

REPO="ZiiMs/Grove"
BINARY_NAME="grove"

main() {
    if [ "${KSH_VERSION-}" = 'Version JM 93t+ 2010-03-05' ]; then
        err 'the installer does not work with this ksh93 version; please try bash'
    fi

    set -u
    parse_args "$@"

   echo ""
    echo " ██████╗ ██████╗  ██████╗ ██╗   ██╗███████╗"
    echo " ██╔════╝ ██╔══██╗██╔═══██╗██║   ██║██╔════╝"
    echo " ██║  ███╗██████╔╝██║   ██║██║   ██║█████╗  "
    echo " ██║   ██║██╔══██╗██║   ██║╚██╗ ██╔╝██╔══╝  "
    echo " ╚██████╔╝██║  ██║╚██████╔╝ ╚████╔╝ ███████╗"
    echo "  ╚═════╝ ╚═╝  ╚═╝ ╚═════╝   ╚═══╝  ╚══════╝"
    echo ""

    local _arch
    _arch="${ARCH:-$(ensure get_architecture)}"
    assert_nz "${_arch}" "arch"
    echo "Detected architecture: ${_arch}"

    if [ "${NO_DEPS}" != "1" ]; then
        check_dependencies
    fi

    local _bin_name
    case "${_arch}" in
    *windows*) _bin_name="${BINARY_NAME}.exe" ;;
    *) _bin_name="${BINARY_NAME}" ;;
    esac

    local _tmp_dir
    _tmp_dir="$(mktemp -d)" || err "mktemp: could not create temporary directory"
    trap 'rm -rf "${_tmp_dir:-}"' EXIT
    cd "${_tmp_dir}" || err "cd: failed to enter directory: ${_tmp_dir}"

    local _package
    _package="$(ensure download_flock "${_arch}")"
    assert_nz "${_package}" "package"
    echo "Downloaded package: ${_package}"

    case "${_package}" in
    *.tar.gz)
        need_cmd tar
        ensure tar -xzf "${_package}"
        ;;
    *.zip)
        need_cmd unzip
        ensure unzip -oq "${_package}"
        ;;
    *)
        err "unsupported package format: ${_package}"
        ;;
    esac

    ensure mkdir -p -- "${BIN_DIR}"
    ensure cp -- "${_bin_name}" "${BIN_DIR}/${_bin_name}"
    ensure chmod +x "${BIN_DIR}/${_bin_name}"
    echo "Installed ${BINARY_NAME} to ${BIN_DIR}"

    echo ""
    echo "${BINARY_NAME} is installed!"
    
    if ! echo ":${PATH}:" | grep -Fq ":${BIN_DIR}:"; then
        echo ""
        echo "Note: ${BIN_DIR} is not on your \$PATH."
        setup_path
    fi

    echo ""
    echo "Run '${BINARY_NAME}' to get started."
}

parse_args() {
    BIN_DIR_DEFAULT="${HOME}/.local/bin"
    VERSION_DEFAULT="latest"
    NO_DEPS_DEFAULT="0"

    BIN_DIR="${BIN_DIR_DEFAULT}"
    VERSION="${VERSION_DEFAULT}"
    NO_DEPS="${NO_DEPS_DEFAULT}"

    while [ "$#" -gt 0 ]; do
        case "$1" in
        --bin-dir) BIN_DIR="$2" && shift 2 ;;
        --bin-dir=*) BIN_DIR="${1#*=}" && shift 1 ;;
        --version) VERSION="$2" && shift 2 ;;
        --version=*) VERSION="${1#*=}" && shift 1 ;;
        --no-deps) NO_DEPS="1" && shift 1 ;;
        -h | --help) usage && exit 0 ;;
        *) err "Unknown option: $1" ;;
        esac
    done
}

usage() {
    local _text_heading _text_reset
    _text_heading="$(tput bold 2>/dev/null || true)"
    _text_reset="$(tput sgr0 2>/dev/null || true)"

    cat <<EOF
${_text_heading}Grove installer${_text_reset}
https://github.com/${REPO}

Fetches and installs Grove. If Grove is already installed, it will be updated to the latest version.

${_text_heading}Usage:${_text_reset}
  install.sh [OPTIONS]

${_text_heading}Options:${_text_reset}
      --bin-dir DIR    Override the installation directory [default: ${BIN_DIR_DEFAULT}]
      --version VER    Install specific version [default: latest]
      --no-deps        Skip dependency installation
  -h, --help           Print this help
EOF
}

check_dependencies() {
    echo "Checking dependencies..."
    
    if ! check_cmd tmux; then
        echo "tmux is not installed. Installing tmux..."
        install_tmux
    else
        echo "tmux is already installed."
    fi
    
    echo "All dependencies satisfied."
    echo ""
}

install_tmux() {
    local _ostype
    _ostype="$(uname -s)"
    
    case "${_ostype}" in
    Darwin)
        if check_cmd brew; then
            ensure brew install tmux
        else
            err "Homebrew is not installed. Please install Homebrew first: https://brew.sh"
        fi
        ;;
    Linux)
        if check_cmd apt-get; then
            ensure sudo apt-get update
            ensure sudo apt-get install -y tmux
        elif check_cmd dnf; then
            ensure sudo dnf install -y tmux
        elif check_cmd yum; then
            ensure sudo yum install -y tmux
        elif check_cmd pacman; then
            ensure sudo pacman -S --noconfirm tmux
        elif check_cmd apk; then
            ensure sudo apk add tmux
        else
            err "Could not determine package manager. Please install tmux manually."
        fi
        ;;
    *)
        err "Unsupported OS for automatic tmux installation. Please install tmux manually."
        ;;
    esac
}

setup_path() {
    local _shell
    _shell="$(basename "${SHELL:-bash}")"
    
    case "${_shell}" in
        bash)
            if [ -f "$HOME/.bashrc" ]; then
                echo ""
                echo "Add the following to your ~/.bashrc:"
                echo "  export PATH=\"\$PATH:${BIN_DIR}\""
                echo ""
                echo "Then run: source ~/.bashrc"
            fi
            ;;
        zsh)
            if [ -f "$HOME/.zshrc" ]; then
                echo ""
                echo "Add the following to your ~/.zshrc:"
                echo "  export PATH=\"\$PATH:${BIN_DIR}\""
                echo ""
                echo "Then run: source ~/.zshrc"
            fi
            ;;
        fish)
            echo ""
            echo "Add the following to your ~/.config/fish/config.fish:"
            echo "  set -gx PATH \$PATH ${BIN_DIR}"
            echo ""
            echo "Then run: source ~/.config/fish/config.fish"
            ;;
        *)
            echo ""
            echo "Add ${BIN_DIR} to your PATH to use Grove."
            ;;
    esac
}

get_asset_pattern() {
    local _arch="$1"
    
    case "${_arch}" in
    x86_64-unknown-linux-*) echo "Linux_x86_64" ;;
    aarch64-unknown-linux-*) echo "Linux_arm64" ;;
    x86_64-apple-darwin) echo "Darwin_x86_64" ;;
    aarch64-apple-darwin) echo "Darwin_arm64" ;;
    *) echo "${_arch}" ;;
    esac
}

download_flock() {
    local _arch="$1"

    if check_cmd curl; then
        _dld=curl
    elif check_cmd wget; then
        _dld=wget
    else
        need_cmd 'curl or wget'
    fi
    need_cmd grep
    need_cmd cut

    local _version
    if [ "${VERSION}" = "latest" ]; then
        local _releases_url="https://api.github.com/repos/${REPO}/releases/latest"
        local _releases
        case "${_dld}" in
        curl) _releases="$(curl -sL "${_releases_url}")" ||
            err "curl: failed to download ${_releases_url}" ;;
        wget) _releases="$(wget -qO- "${_releases_url}")" ||
            err "wget: failed to download ${_releases_url}" ;;
        esac
        
        if echo "${_releases}" | grep -q 'API rate limit exceeded'; then
            err "GitHub API rate limit exceeded. Please try again later or use a different installation method."
        fi
        
        _version="$(echo "${_releases}" | grep -m1 '"tag_name":' | cut -d'"' -f4)"
        if [ -z "${_version}" ]; then
            err "Failed to determine latest version. Please specify a version with --version"
        fi
    else
        _version="v${VERSION}"
        if ! echo "${_version}" | grep -q '^v'; then
            _version="v${VERSION}"
        fi
    fi
    
    echo "Installing version: ${_version}" >&2

    local _package_url
    local _releases_list_url="https://api.github.com/repos/${REPO}/releases"
    local _releases_list
    
    case "${_dld}" in
    curl) _releases_list="$(curl -sL "${_releases_list_url}")" ||
        err "curl: failed to download ${_releases_list_url}" ;;
    wget) _releases_list="$(wget -qO- "${_releases_list_url}")" ||
        err "wget: failed to download ${_releases_list_url}" ;;
    esac
    
    local _asset_pattern
    _asset_pattern="$(get_asset_pattern "${_arch}")"
    
    _package_url="$(echo "${_releases_list}" | grep "browser_download_url" | cut -d'"' -f4 | grep -- "${_version}" | grep -- "${_asset_pattern}" | grep '\.tar\.gz$' | head -1)"
    
    if [ -z "${_package_url}" ]; then
        err "No release found for architecture (${_asset_pattern}) and version (${_version}).

Available architectures for version ${_version}:
$(echo "${_releases_list}" | grep "browser_download_url" | cut -d'"' -f4 | grep -- "${_version}" | grep '\.tar\.gz$' | xargs -n1 basename 2>/dev/null || echo "None found")

Please check https://github.com/${REPO}/releases for available downloads."
    fi

    local _ext
    case "${_package_url}" in
    *.tar.gz) _ext="tar.gz" ;;
    *.zip) _ext="zip" ;;
    *) err "unsupported package format: ${_package_url}" ;;
    esac

    local _package="${BINARY_NAME}.${_ext}"
    echo "Downloading from: ${_package_url}" >&2
    
    case "${_dld}" in
    curl) curl -sLo "${_package}" "${_package_url}" || err "curl: failed to download ${_package_url}" ;;
    wget) wget -qO "${_package}" "${_package_url}" || err "wget: failed to download ${_package_url}" ;;
    esac

    echo "${_package}"
}

get_architecture() {
    local _ostype _cputype _bitness _arch _clibtype
    _ostype="$(uname -s)"
    _cputype="$(uname -m)"
    _clibtype="musl"

    if [ "${_ostype}" = Linux ]; then
        if [ "$(uname -o 2>/dev/null || true)" = Android ]; then
            _ostype=Android
        fi
    fi

    if [ "${_ostype}" = Darwin ] && [ "${_cputype}" = i386 ]; then
        if sysctl hw.optional.x86_64 2>/dev/null | grep -q ': 1'; then
            _cputype=x86_64
        fi
    fi

    if [ "${_ostype}" = SunOS ]; then
        if [ "$(/usr/bin/uname -o 2>/dev/null || true)" = illumos ]; then
            _ostype=illumos
        fi
        if [ "${_cputype}" = i86pc ]; then
            _cputype="$(isainfo -n)"
        fi
    fi

    case "${_ostype}" in
    Android)
        _ostype=linux-android
        ;;
    Linux)
        check_proc
        _ostype=unknown-linux-${_clibtype}
        _bitness=$(get_bitness)
        ;;
    FreeBSD)
        _ostype=unknown-freebsd
        ;;
    NetBSD)
        _ostype=unknown-netbsd
        ;;
    DragonFly)
        _ostype=unknown-dragonfly
        ;;
    Darwin)
        _ostype=apple-darwin
        ;;
    illumos)
        _ostype=unknown-illumos
        ;;
    MINGW* | MSYS* | CYGWIN* | Windows_NT)
        _ostype=pc-windows-msvc
        ;;
    *)
        err "unrecognized OS type: ${_ostype}"
        ;;
    esac

    case "${_cputype}" in
    i386 | i486 | i686 | i786 | x86)
        _cputype=i686
        ;;
    xscale | arm)
        _cputype=arm
        if [ "${_ostype}" = "linux-android" ]; then
            _ostype=linux-androideabi
        fi
        ;;
    armv6l)
        _cputype=arm
        if [ "${_ostype}" = "linux-android" ]; then
            _ostype=linux-androideabi
        else
            _ostype="${_ostype}eabihf"
        fi
        ;;
    armv7l | armv8l)
        _cputype=armv7
        if [ "${_ostype}" = "linux-android" ]; then
            _ostype=linux-androideabi
        else
            _ostype="${_ostype}eabihf"
        fi
        ;;
    aarch64 | arm64)
        _cputype=aarch64
        ;;
    x86_64 | x86-64 | x64 | amd64)
        _cputype=x86_64
        ;;
    mips)
        _cputype=$(get_endianness mips '' el)
        ;;
    mips64)
        if [ "${_bitness}" -eq 64 ]; then
            _ostype="${_ostype}abi64"
            _cputype=$(get_endianness mips64 '' el)
        fi
        ;;
    ppc)
        _cputype=powerpc
        ;;
    ppc64)
        _cputype=powerpc64
        ;;
    ppc64le)
        _cputype=powerpc64le
        ;;
    s390x)
        _cputype=s390x
        ;;
    riscv64)
        _cputype=riscv64gc
        ;;
    *)
        err "unknown CPU type: ${_cputype}"
        ;;
    esac

    if [ "${_ostype}" = unknown-linux-musl ] && [ "${_bitness}" -eq 32 ]; then
        case ${_cputype} in
        x86_64)
            if is_host_amd64_elf; then
                err "x32 userland is unsupported"
            else
                _cputype=i686
            fi
            ;;
        mips64)
            _cputype=$(get_endianness mips '' el)
            ;;
        powerpc64)
            _cputype=powerpc
            ;;
        aarch64)
            _cputype=armv7
            if [ "${_ostype}" = "linux-android" ]; then
                _ostype=linux-androideabi
            else
                _ostype="${_ostype}eabihf"
            fi
            ;;
        riscv64gc)
            err "riscv64 with 32-bit userland unsupported"
            ;;
        esac
    fi

    if [ "${_ostype}" = "unknown-linux-musleabihf" ] && [ "${_cputype}" = armv7 ]; then
        if ensure grep '^Features' /proc/cpuinfo 2>/dev/null | grep -q -v neon; then
            _cputype=arm
        fi
    fi

    _arch="${_cputype}-${_ostype}"
    echo "${_arch}"
}

get_bitness() {
    need_cmd head
    local _current_exe_head
    _current_exe_head=$(head -c 5 /proc/self/exe)
    if [ "${_current_exe_head}" = "$(printf '\177ELF\001')" ]; then
        echo 32
    elif [ "${_current_exe_head}" = "$(printf '\177ELF\002')" ]; then
        echo 64
    else
        err "unknown platform bitness"
    fi
}

get_endianness() {
    local cputype="$1"
    local suffix_eb="$2"
    local suffix_el="$3"
    need_cmd head
    need_cmd tail
    local _current_exe_endianness
    _current_exe_endianness="$(head -c 6 /proc/self/exe | tail -c 1)"
    if [ "${_current_exe_endianness}" = "$(printf '\001')" ]; then
        echo "${cputype}${suffix_el}"
    elif [ "${_current_exe_endianness}" = "$(printf '\002')" ]; then
        echo "${cputype}${suffix_eb}"
    else
        err "unknown platform endianness"
    fi
}

is_host_amd64_elf() {
    need_cmd head
    need_cmd tail
    local _current_exe_machine
    _current_exe_machine=$(head -c 19 /proc/self/exe | tail -c 1)
    [ "${_current_exe_machine}" = "$(printf '\076')" ]
}

check_proc() {
    if [ "${_ostype}" != "unknown-linux-musl" ]; then
        return
    fi
    if ! test -L /proc/self/exe; then
        err "unable to find /proc/self/exe. Is /proc mounted? Installation cannot proceed without /proc."
    fi
}

need_cmd() {
    if ! check_cmd "$1"; then
        err "need '$1' (command not found)"
    fi
}

check_cmd() {
    command -v -- "$1" >/dev/null 2>&1
}

ensure() {
    if ! "$@"; then err "command failed: $*"; fi
}

assert_nz() {
    if [ -z "$1" ]; then err "found empty string: $2"; fi
}

err() {
    printf '\033[31mError:\033[0m %s\n' "$1" >&2
    exit 1
}

main "$@" || exit 1
