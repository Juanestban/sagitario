#!/bin/sh
# Builds Sagitario from the seed with a C compiler alone (docs/adr/0067-bootstrap-from-the-seed.md):
# the seed is stage 0, which builds stage 1, which builds stage 2; stages 1 and 2 must generate the
# same C. Writes `build/bin/sag` and `build/bin/nova`. Linux and macOS; on Windows, `build.ps1`.
set -eu

usage() {
  cat <<'EOF'
Usage: bootstrap/build.sh [--debug] [--install] [--update-seed]

  --debug        Build stages 1 and 2 without optimizations; integer overflow panics
  --install      Copy sag and nova to ~/.sagitario/bin and add it to PATH
  --update-seed  Write the C of stage 2 and the runtime to bootstrap/
  --help         Show this message

The C compiler is SAG_CC, else cc.
EOF
}

mode=--release
install=0
update_seed=0

for arg in "$@"; do
  case "$arg" in
    --debug) mode= ;;
    --install) install=1 ;;
    --update-seed) update_seed=1 ;;
    --help | -h) usage; exit 0 ;;
    *) printf 'error: unknown option `%s`\n\n' "$arg" >&2; usage >&2; exit 2 ;;
  esac
done

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

# Git Bash and other POSIX layers on Windows, where executables end in `.exe`.
exe=
case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*) exe=.exe ;;
esac

SAG_CC=${SAG_CC:-cc}
export SAG_CC

step() {
  printf '==> %s\n' "$1"
}

# Fails if a check of `file` reports anything: the compiler, Nova and the harness have no
# warnings.
check_clean() {
  report=$(build/bin/sag$exe check "$1" 2>&1) || { printf '%s\n' "$report" >&2; exit 1; }

  if [ -n "$report" ]; then
    printf '%s\n' "$report" >&2
    printf 'error: `%s` has warnings\n' "$1" >&2
    exit 1
  fi
}

rm -rf build/stage0 build/stage1
mkdir -p build/stage0 build/stage1 build/bin

# What `sag` links: libm outside Windows, and on Windows the stack of Linux and macOS.
if [ -z "$exe" ]; then
  link=-lm
else
  case "$("$SAG_CC" -dumpmachine 2>/dev/null || true)" in
    *mingw* | *gnu*) link=-Wl,--stack,8388608 ;;
    *) link=-Wl,/STACK:8388608 ;;
  esac
fi

step "stage 0: the seed, with $SAG_CC"
# The flags `sag` uses for the generated C in release mode.
"$SAG_CC" -std=c11 -pedantic-errors -O2 -DSAG_RELEASE -I bootstrap -o build/stage0/sag$exe \
  bootstrap/sag.c bootstrap/sagitario_runtime.c $link

step "stage 1"
build/stage0/sag$exe build $mode compiler/src/main.sag --output build/stage1/sag$exe

step "stage 2"
build/stage1/sag$exe build $mode compiler/src/main.sag --output build/bin/sag$exe

step "fixed point: stages 1 and 2 generate the same C"
build/stage1/sag$exe check --dump=c compiler/src/main.sag > build/stage1/sag.c
build/bin/sag$exe check --dump=c compiler/src/main.sag > build/sag.c

if ! cmp -s build/stage1/sag.c build/sag.c; then
  printf 'error: stages 1 and 2 generate different C for the compiler (build/stage1/sag.c, build/sag.c)\n' >&2
  exit 1
fi

step "no warnings in the compiler, Nova and the harness"
check_clean compiler/src/main.sag
check_clean compiler/src/nova_main.sag
check_clean tests/harness/main.sag

step "nova"
build/bin/sag$exe build $mode compiler/src/nova_main.sag --output build/bin/nova$exe

if [ "$update_seed" = 1 ]; then
  step "update the seed"
  cp build/sag.c bootstrap/sag.c
  cp runtime/sagitario_runtime.c runtime/sagitario_runtime.h bootstrap/
fi

if [ "$install" = 1 ]; then
  if [ -n "$exe" ]; then
    printf 'error: on Windows, install with `bootstrap/build.ps1 --install`\n' >&2
    exit 1
  fi

  bin="$HOME/.sagitario/bin"

  step "install to $bin"
  mkdir -p "$bin"
  # A new file instead of overwriting: a running `sag` keeps its own.
  for tool in sag nova; do
    cp build/bin/$tool "$bin/$tool.new"
    mv -f "$bin/$tool.new" "$bin/$tool"
  done

  line='export PATH="$HOME/.sagitario/bin:$PATH"'
  profiles="$HOME/.profile"

  if [ "$(uname -s)" = Darwin ]; then
    profiles="$profiles $HOME/.zshrc"
  fi

  for profile in $profiles; do
    if ! grep -qs '\.sagitario/bin' "$profile"; then
      printf '\n# Sagitario\n%s\n' "$line" >> "$profile"
      printf 'added %s to PATH in %s\n' "$bin" "$profile"
    fi
  done

  case ":$PATH:" in
    *":$bin:"*) ;;
    *) printf 'open a new terminal (or log in again) to use sag and nova\n' ;;
  esac
fi

step "done: build/bin/sag$exe and build/bin/nova$exe"
