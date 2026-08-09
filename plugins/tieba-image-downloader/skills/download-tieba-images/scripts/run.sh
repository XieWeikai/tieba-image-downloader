#!/bin/sh
set -eu

PLUGIN_VERSION="0.4.0"
REPOSITORY="XieWeikai/tieba-image-downloader"
BINARY_NAME="tieba-image-downloader"

find_binary() {
    if [ -n "${TIEBA_IMAGE_DOWNLOADER_BIN:-}" ] && [ -x "${TIEBA_IMAGE_DOWNLOADER_BIN}" ]; then
        printf '%s\n' "${TIEBA_IMAGE_DOWNLOADER_BIN}"
        return 0
    fi
    if command -v "${BINARY_NAME}" >/dev/null 2>&1; then
        command -v "${BINARY_NAME}"
        return 0
    fi
    return 1
}

install_release() {
    os="$(uname -s)"
    arch="$(uname -m)"
    if [ "${os}" != "Darwin" ]; then
        printf '%s\n' "This release supports macOS only (detected ${os})." >&2
        return 1
    fi
    case "${arch}" in
        arm64|aarch64) suffix="macos-arm64" ;;
        x86_64|amd64) suffix="macos-x86_64" ;;
        *) printf '%s\n' "Unsupported macOS architecture: ${arch}" >&2; return 1 ;;
    esac

    cache_base="${XDG_CACHE_HOME:-${HOME}/Library/Caches}"
    install_dir="${cache_base}/tieba-image-downloader-plugin/${PLUGIN_VERSION}"
    installed_binary="${install_dir}/${BINARY_NAME}"
    if [ -x "${installed_binary}" ]; then
        printf '%s\n' "${installed_binary}"
        return 0
    fi

    command -v curl >/dev/null 2>&1 || { printf '%s\n' "curl is required to install ${BINARY_NAME}." >&2; return 1; }
    command -v shasum >/dev/null 2>&1 || { printf '%s\n' "shasum is required to verify ${BINARY_NAME}." >&2; return 1; }

    archive="${BINARY_NAME}-v${PLUGIN_VERSION}-${suffix}.tar.gz"
    base_url="https://github.com/${REPOSITORY}/releases/download/v${PLUGIN_VERSION}"
    temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/tieba-image-downloader.XXXXXX")"
    trap 'rm -rf "${temp_dir}"' EXIT HUP INT TERM

    printf '%s\n' "Installing verified ${BINARY_NAME} v${PLUGIN_VERSION}..." >&2
    curl --fail --location --silent --show-error "${base_url}/${archive}" --output "${temp_dir}/${archive}"
    curl --fail --location --silent --show-error "${base_url}/SHA256SUMS" --output "${temp_dir}/SHA256SUMS"

    expected="$(awk -v name="${archive}" '$2 == "dist/" name || $2 == name { print $1; exit }' "${temp_dir}/SHA256SUMS")"
    actual="$(shasum -a 256 "${temp_dir}/${archive}" | awk '{print $1}')"
    if [ -z "${expected}" ] || [ "${expected}" != "${actual}" ]; then
        printf '%s\n' "Release checksum verification failed for ${archive}." >&2
        return 1
    fi

    mkdir -p "${install_dir}"
    tar -xzf "${temp_dir}/${archive}" -C "${install_dir}"
    chmod 755 "${installed_binary}"
    printf '%s\n' "${installed_binary}"
}

binary="$(find_binary || install_release)"
exec "${binary}" "$@"
