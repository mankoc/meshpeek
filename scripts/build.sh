#!/usr/bin/env bash
# Build, test and package MeshPeek on Linux.
#
# Runs the checks (format, lint, tests), builds the release executable and writes to dist/:
#   MeshPeek-<version>-linux-x64.tar.gz   portable: unpack and run ./meshpeek
#   meshpeek_<version>-1_amd64.deb         Debian/Ubuntu package (only if cargo-deb is installed)
#
# Usage: scripts/build.sh [--check] [--skip-tests] [--no-deb] [--install-tools]
#   --check          only run the checks; build nothing (used by CI)
#   --skip-tests     skip the checks, go straight to the release build
#   --no-deb         do not build the .deb
#   --install-tools  install the build dependencies (apt), cargo-deb and cargo-about first
set -euo pipefail

cd "$(dirname "$0")/.."
check=0 skip_tests=0 no_deb=0 install_tools=0
for arg in "$@"; do
    case "$arg" in
        --check) check=1 ;;
        --skip-tests) skip_tests=1 ;;
        --no-deb) no_deb=1 ;;
        --install-tools) install_tools=1 ;;
        -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done
step() { printf '\n\033[36m==> %s\033[0m\n' "$1"; }

if [ "$install_tools" = 1 ]; then
    step "Installing build dependencies"
    sudo apt-get update
    sudo apt-get install -y build-essential pkg-config libx11-dev libxkbcommon-dev libwayland-dev libgtk-3-dev \
        mesa-vulkan-drivers git-lfs
    command -v cargo-deb >/dev/null || cargo install cargo-deb --locked
    command -v cargo-about >/dev/null || cargo install cargo-about --locked --features cli
fi

if [ "$skip_tests" = 0 ]; then
    step "Checking formatting"
    cargo fmt --all --check
    step "Linting (clippy)"
    cargo clippy --all-targets -- -D warnings
    step "Running tests"
    cargo test --all
fi
if [ "$check" = 1 ]; then echo; echo "Checks passed."; exit 0; fi

step "Building release executable"
cargo build --release -p meshpeek-app

step "Collecting third-party licenses"
command -v cargo-about >/dev/null || { echo "cargo-about not found: run with --install-tools once" >&2; exit 1; }
cargo about generate about.hbs -o target/release/THIRD-PARTY-LICENSES.html
version=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n1)
mkdir -p dist

step "Packaging portable tar.gz"
name="MeshPeek-$version-linux-x64"
stage="dist/$name"
rm -rf "$stage" && mkdir -p "$stage"
cp target/release/meshpeek target/release/THIRD-PARTY-LICENSES.html LICENSE README.md LYS_FILE_FORMAT.md \
    crates/meshpeek-app/assets/icon.png \
    crates/meshpeek-app/assets/meshpeek.desktop "$stage/"
tar -C dist -czf "dist/$name.tar.gz" "$name"
rm -rf "$stage"
echo "    dist/$name.tar.gz"

if [ "$no_deb" = 0 ]; then
    step "Building .deb package"
    if command -v cargo-deb >/dev/null; then
        cargo deb -p meshpeek-app --no-build --output dist/
    else
        echo "cargo-deb not found: skipping the .deb. Run with --install-tools once to install it." >&2
    fi
fi

echo; echo "Done. Packages are in dist/"
