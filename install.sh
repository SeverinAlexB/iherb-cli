#!/bin/sh
set -eu

REPO="SeverinAlexB/iherb-cli"
BINARY="iherb-cli"
INSTALL_DIR="/usr/local/bin"

main() {
    os=$(detect_os)
    arch=$(detect_arch)
    asset=$(asset_name "$os" "$arch")

    echo "Detected platform: ${os}/${arch}"
    echo "Downloading ${BINARY} (latest release)..."

    download_url=$(get_download_url "$asset")
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT

    if ! curl -fsSL "$download_url" -o "$tmp/$asset"; then
        echo "Error: Failed to download $asset" >&2
        exit 1
    fi

    echo "Extracting..."
    case "$asset" in
        *.tar.gz) tar xzf "$tmp/$asset" -C "$tmp" ;;
        *.zip)    unzip -q "$tmp/$asset" -d "$tmp" ;;
    esac

    if [ -w "$INSTALL_DIR" ]; then
        mv "$tmp/$BINARY" "$INSTALL_DIR/$BINARY"
    else
        echo "Installing to ${INSTALL_DIR} (requires sudo)..."
        sudo mv "$tmp/$BINARY" "$INSTALL_DIR/$BINARY"
    fi
    chmod +x "$INSTALL_DIR/$BINARY"

    echo "Installed ${BINARY} to ${INSTALL_DIR}/${BINARY}"
    echo "Run '${BINARY} --help' to get started."
}

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
        *) echo "Error: Unsupported OS: $(uname -s)" >&2; exit 1 ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "amd64" ;;
        arm64|aarch64) echo "arm64" ;;
        *) echo "Error: Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
    esac
}

asset_name() {
    os="$1"
    arch="$2"
    case "$os" in
        windows) echo "${BINARY}-${os}-${arch}.zip" ;;
        *)       echo "${BINARY}-${os}-${arch}.tar.gz" ;;
    esac
}

get_download_url() {
    asset="$1"
    url="https://github.com/${REPO}/releases/latest/download/${asset}"
    # Verify the asset exists (follows redirect, fails on 404)
    if ! curl -fsSL -o /dev/null --head "$url" 2>/dev/null; then
        echo "Error: No release asset found for your platform (${asset})" >&2
        echo "Check available releases: https://github.com/${REPO}/releases" >&2
        exit 1
    fi
    echo "$url"
}

main
