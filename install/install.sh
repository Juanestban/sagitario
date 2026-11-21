#!/bin/sh
# Installs `sag` and `nova` from a release of Sagitario (docs/adr/0069-installers.md). Linux and
# macOS; on Windows, `install.ps1`.
#
#   curl -fsSL https://github.com/Juanestban/sagitario/releases/latest/download/install.sh | sh
#   curl -fsSL https://github.com/Juanestban/sagitario/releases/latest/download/install.sh | sh -s -- --version 0.8.0
set -eu

repository=https://github.com/Juanestban/sagitario

usage() {
  cat <<'EOF'
Usage: install.sh [--version X.Y.Z] [--dir DIR] [--no-modify-path] [--from URL]

Installs sag and nova from the latest release of Sagitario, or from the one --version names.

  --version X.Y.Z   The release to install
  --dir DIR         Install to DIR/bin (default: ~/.sagitario)
  --no-modify-path  Do not add DIR/bin to PATH in the shell profiles
  --from URL        Download from URL, a directory with the archives and SHA256SUMS, instead of
                    the GitHub Release
  --help            Show this message
EOF
}

error() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

usage_error() {
  printf 'error: %s\n\n' "$1" >&2
  usage >&2
  exit 2
}

version=
dir="$HOME/.sagitario"
modify_path=1
from=

while [ $# -gt 0 ]; do
  case "$1" in
    --version | --dir | --from)
      [ $# -ge 2 ] || usage_error "\`$1\` needs a value"
      case "$1" in
        --version) version=$2 ;;
        --dir) dir=$2 ;;
        --from) from=$2 ;;
      esac
      shift
      ;;
    --no-modify-path) modify_path=0 ;;
    --help | -h) usage; exit 0 ;;
    *) usage_error "unknown option \`$1\`" ;;
  esac
  shift
done

# The platform, named as the archives of a release are (ADR-0068, Q381).
case "$(uname -s)" in
  Linux) system=unknown-linux-gnu ;;
  Darwin) system=apple-darwin ;;
  *) error "this installer is for Linux and macOS; on Windows, use install.ps1 ($repository)" ;;
esac

case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) error "the releases are for x86-64 and AArch64, not \`$(uname -m)\`; build from source ($repository)" ;;
esac

# A shell under Rosetta on an Apple silicon Mac reports x86-64.
if [ "$system" = apple-darwin ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
  arch=aarch64
fi

if [ "$system" = unknown-linux-gnu ] && ldd --version 2>&1 | grep -qi musl; then
  error "the releases for Linux need glibc, which this system lacks; build from source ($repository)"
fi

triple="$arch-$system"

for tool in curl tar; do
  command -v "$tool" >/dev/null 2>&1 || error "the installer needs \`$tool\`"
done

if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d ' ' -f 1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
else
  error 'the installer needs `sha256sum` or `shasum`'
fi

if [ -n "$from" ]; then
  base=${from%/}
elif [ -n "$version" ]; then
  base="$repository/releases/download/v$version"
else
  base="$repository/releases/latest/download"
fi

download() {
  curl --proto '=https,file' -fsSL -o "$2" "$1" || error "cannot download $1"
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# The checksums of a release name its archives, and so the version of the latest one.
download "$base/SHA256SUMS" "$tmp/SHA256SUMS"

line=$(awk -v suffix="-$triple.tar.gz" '{
  name = $2
  sub(/^\*/, "", name)
  if (length(name) > length(suffix) && substr(name, length(name) - length(suffix) + 1) == suffix)
    print $1, name
}' "$tmp/SHA256SUMS")

[ -n "$line" ] || error "the release has no archive for $triple"
expected=${line%% *}
archive=${line#* }

if [ -n "$version" ] && [ "$archive" != "sagitario-$version-$triple.tar.gz" ]; then
  error "the release of $version has no archive sagitario-$version-$triple.tar.gz"
fi

printf 'downloading %s\n' "$archive"
download "$base/$archive" "$tmp/$archive"

[ "$(sha256 "$tmp/$archive")" = "$expected" ] || error "the checksum of $archive does not match SHA256SUMS"

tar -xzf "$tmp/$archive" -C "$tmp"
release="$tmp/${archive%.tar.gz}"
[ -f "$release/bin/sag" ] && [ -f "$release/bin/nova" ] || error "$archive does not hold bin/sag and bin/nova"

mkdir -p "$dir/bin"
dir=$(cd "$dir" && pwd)
bin="$dir/bin"

# A new file instead of overwriting: a running `sag` keeps its own.
for tool in sag nova; do
  cp "$release/bin/$tool" "$bin/$tool.new"
  chmod +x "$bin/$tool.new"
  mv -f "$bin/$tool.new" "$bin/$tool"
done

printf 'installed %s and %s to %s\n' "$("$bin/sag" --version)" "$("$bin/nova" --version)" "$bin"

# As `bootstrap/build.sh --install` does.
if [ "$modify_path" = 1 ]; then
  if [ "$dir" = "$HOME/.sagitario" ]; then
    entry='$HOME/.sagitario/bin'
  else
    entry=$bin
  fi

  profiles="$HOME/.profile"

  if [ "$system" = apple-darwin ]; then
    profiles="$profiles $HOME/.zshrc"
  fi

  for profile in $profiles; do
    if ! grep -qsF -e "$entry" -e "$bin" "$profile"; then
      printf '\n# Sagitario\nexport PATH="%s:$PATH"\n' "$entry" >> "$profile"
      printf 'added %s to PATH in %s\n' "$bin" "$profile"
    fi
  done

  case ":$PATH:" in
    *":$bin:"*) ;;
    *) printf 'open a new terminal (or log in again) to use sag and nova\n' ;;
  esac
fi

# `sag` compiles through a C compiler, which it looks for as here (ADR-0068, Q388).
compiler=
if [ -n "${SAG_CC:-}" ]; then
  candidates=$SAG_CC
elif [ "$system" = apple-darwin ] && ! xcode-select -p >/dev/null 2>&1; then
  # Without the command line tools, /usr/bin/clang only offers to install them.
  candidates=
else
  candidates='clang gcc'
fi

for candidate in $candidates; do
  if "$candidate" -dumpmachine </dev/null >/dev/null 2>&1; then
    compiler=$candidate
    break
  fi
done

if [ -n "$compiler" ]; then
  printf 'sag will compile with %s\n' "$compiler"
else
  if [ -n "${SAG_CC:-}" ]; then
    printf 'warning: cannot run the C compiler `%s` (from SAG_CC), which sag needs\n' "$SAG_CC" >&2
  else
    printf 'warning: no C compiler found; sag needs `clang` (or `gcc`) on PATH, or SAG_CC\n' >&2
  fi

  if [ "$system" = apple-darwin ]; then
    hint='xcode-select --install'
  elif command -v apt-get >/dev/null 2>&1; then
    hint='sudo apt install clang'
  elif command -v dnf >/dev/null 2>&1; then
    hint='sudo dnf install clang'
  elif command -v pacman >/dev/null 2>&1; then
    hint='sudo pacman -S clang'
  elif command -v zypper >/dev/null 2>&1; then
    hint='sudo zypper install clang'
  else
    hint='install clang with the package manager of the system'
  fi

  printf '  to install one: %s\n' "$hint" >&2
fi
