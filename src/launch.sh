#!/bin/sh
set -eu

# This script finds the installation prefix beside itself.
prefix=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)

# To run installed commands directly from your shell, prepend ~/easypp/bin
# to PATH in your shell startup file. For desktop applications to see the
# installed icons, launchers and data, prepend ~/easypp/share to XDG_DATA_DIRS
# in your login environment and sign in again. Keep your existing entries.
#   export PATH="$HOME/easypp/bin${PATH:+:$PATH}"
#   export XDG_DATA_DIRS="$HOME/easypp/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
export PATH="$prefix/bin${PATH:+:$PATH}"
export XDG_DATA_DIRS="$prefix/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"

# easypp executables find project libraries in ../lib relative to the binary.
# If another program loads these libraries, prepend ~/easypp/lib to its
# LD_LIBRARY_PATH. XDG_DATA_HOME and XDG_CONFIG_HOME still control where
# applications save your personal data and settings.
#   export LD_LIBRARY_PATH="$HOME/easypp/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

if [ "$#" -eq 0 ]; then
    set -- "$prefix"/bin/*
    if [ "$#" -eq 1 ] && [ -f "$1" ] && [ -x "$1" ]; then
        exec "$1"
    fi
    printf 'Usage: %s PROGRAM [ARGUMENT ...]\n' "$0" >&2
    for program; do
        if [ -f "$program" ] && [ -x "$program" ]; then
            printf '  %s\n' "${program##*/}" >&2
        fi
    done
    exit 2
fi

program=$1
shift
case "$program" in
    ''|.|..|*/*) printf 'Pass a program name from the installation bin directory.\n' >&2; exit 2 ;;
esac
exec "$prefix/bin/$program" "$@"
