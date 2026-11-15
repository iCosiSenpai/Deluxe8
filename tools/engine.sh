#!/usr/bin/env bash
# Deluxe8 engine build tool for Linux (x86-64): the counterpart of tools/engine.ps1.
#
#   tools/engine.sh toolchain       check the system compiler, CMake, Ninja and glslang (packages: docs/BUILDING.md)
#   tools/engine.sh fetch           clone/fetch the pinned Eden revision (engine/upstream.json) into .cache/engine/eden
#   tools/engine.sh patch [--force] reset to the pinned revision, apply engine/patches/*.patch, link engine/src/deluxe8
#   tools/engine.sh configure       CMake (Ninja), no Qt: eden-cli + eden-room with -DDELUXE8=ON
#   tools/engine.sh build           compile
#   tools/engine.sh stage           copy deluxe8-engine / deluxe8-room into out/engine/linux-x64
#   tools/engine.sh all             everything above, in order
#   tools/engine.sh export-patches  (dev) regenerate engine/patches from the commits on the 'deluxe8' branch
#   tools/engine.sh clean           delete the build directory
#
# Environment: CONFIG=Release|RelWithDebInfo|Debug (default Release), CC/CXX to pick the compiler (Clang 19+ or GCC 16+).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE="$ROOT/.cache"
SRC_DIR="$CACHE/engine/eden"
BUILD_DIR="$ROOT/build/engine-linux-x64"
STAGE_DIR="$ROOT/out/engine/linux-x64"
PATCH_DIR="$ROOT/engine/patches"
OUR_SRC="$ROOT/engine/src/deluxe8"
CONFIG="${CONFIG:-Release}"

step() { printf '\033[36m==> %s\033[0m\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

json_field() { # json_field <key>: a top-level string field of engine/upstream.json
    sed -n "s/^[[:space:]]*\"$1\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" "$ROOT/engine/upstream.json" | head -n1
}
UPSTREAM_URL="$(json_field url)"
UPSTREAM_COMMIT="$(json_field commit)"
[ -n "$UPSTREAM_URL" ] && [ -n "$UPSTREAM_COMMIT" ] || die "engine/upstream.json: url/commit not found"

egit() { git -C "$SRC_DIR" "$@"; }

toolchain() {
    step "Toolchain"
    for tool in git cmake ninja pkg-config glslangValidator; do
        command -v "$tool" >/dev/null || die "$tool is missing (see docs/BUILDING.md for the packages)"
    done
    local cc="${CXX:-$(command -v clang++ || command -v g++ || true)}"
    [ -n "$cc" ] || die "no C++ compiler (Clang 19+ or GCC 16+)"
    echo "   compiler: $("$cc" --version | head -n1)"
    echo "   cmake:    $(cmake --version | head -n1)"
    echo "   ninja:    $(ninja --version)"
}

fetch() {
    step "Fetch eden @ ${UPSTREAM_COMMIT:0:12}"
    if [ ! -d "$SRC_DIR/.git" ]; then
        mkdir -p "$SRC_DIR"
        egit init -q
        egit remote add origin "$UPSTREAM_URL"
    fi
    if [ "$(egit cat-file -t "$UPSTREAM_COMMIT" 2>/dev/null || true)" != "commit" ]; then
        egit fetch --depth 1 origin "$UPSTREAM_COMMIT"
    fi
}

patch_tree() {
    step "Apply Deluxe8 patch queue"
    local force="${1:-}"
    if [ -n "$(egit status --porcelain --untracked-files=no)" ] && [ "$force" != "--force" ]; then
        die "the engine tree has local changes: commit + export-patches first, or pass --force"
    fi
    egit -c advice.detachedHead=false checkout -q -f -B deluxe8 "$UPSTREAM_COMMIT"
    local patches=("$PATCH_DIR"/*.patch)
    if [ -e "${patches[0]}" ]; then
        egit -c user.name=Deluxe8 -c user.email=build@deluxe8.invalid am --3way --keep-cr "${patches[@]}"
        echo "   ${#patches[@]} patch(es) applied"
    fi
    # our own sources live in this repository: link them into the tree
    rm -rf "$SRC_DIR/src/deluxe8"
    ln -s "$OUR_SRC" "$SRC_DIR/src/deluxe8"
    grep -qx '/src/deluxe8' "$SRC_DIR/.git/info/exclude" 2>/dev/null || echo '/src/deluxe8' >> "$SRC_DIR/.git/info/exclude"
}

configure() {
    step "Configure ($CONFIG)"
    cmake -S "$SRC_DIR" -B "$BUILD_DIR" -G Ninja \
        -DCMAKE_BUILD_TYPE="$CONFIG" \
        -DCPM_SOURCE_CACHE="$CACHE/cpm" \
        -DENABLE_QT=OFF -DYUZU_CMD=ON -DYUZU_ROOM=ON -DYUZU_ROOM_STANDALONE=ON \
        -DYUZU_TESTS=OFF -DBUILD_TESTING=OFF -DENABLE_WERROR=OFF \
        -DENABLE_UPDATE_CHECKER=OFF -DENABLE_LSFG=ON -DENABLE_RESHADE=ON \
        -DDELUXE8=ON
}

build() {
    step "Build"
    cmake --build "$BUILD_DIR" --config "$CONFIG"
}

stage() {
    step "Stage -> $STAGE_DIR"
    local bin="$BUILD_DIR/bin"
    mkdir -p "$STAGE_DIR"
    install -m 755 "$bin/eden-cli" "$STAGE_DIR/deluxe8-engine"
    [ -f "$bin/eden-room" ] && install -m 755 "$bin/eden-room" "$STAGE_DIR/deluxe8-room"
    install -m 644 "$SRC_DIR/LICENSE.txt" "$STAGE_DIR/LICENSE-eden.txt"
    ls -l "$STAGE_DIR"
}

export_patches() {
    step "Export patches from branch deluxe8"
    mkdir -p "$PATCH_DIR"
    rm -f "$PATCH_DIR"/*.patch
    egit format-patch --quiet --no-signature --zero-commit -N -o "$PATCH_DIR" "$UPSTREAM_COMMIT..deluxe8"
    ls "$PATCH_DIR"
}

case "${1:-all}" in
    toolchain) toolchain ;;
    fetch) fetch ;;
    patch) patch_tree "${2:-}" ;;
    configure) configure ;;
    build) build ;;
    stage) stage ;;
    export-patches) export_patches ;;
    clean) rm -rf "$BUILD_DIR" ;;
    all) toolchain; fetch; patch_tree "${2:-}"; configure; build; stage ;;
    *) die "unknown command '$1' (toolchain|fetch|patch|configure|build|stage|all|export-patches|clean)" ;;
esac
