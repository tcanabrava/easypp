#!/usr/bin/env bash
# Reproduces the Blockfall comparison in the paper. Usage: paper/eval/run.sh [easypp-binary]
# Works in a fresh temporary folder; prints results as tab-separated lines.
set -uo pipefail

EVAL="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$EVAL/../.." && pwd)"
EASYPP="${1:-$REPO/target/release/easypp}"
GAME="$REPO/examples/blockfall"
WORK="$(mktemp -d /tmp/easypp-eval.XXXXXX)"
RUNS=3
export CC=clang CXX=clang++

now() { date +%s.%N; }
median() { sort -n | awk '{a[NR]=$1} END {print a[int((NR+1)/2)]}'; }
quiet() { "$@" >/dev/null 2>&1; }

# Build commands per tool: setup+build, test, no-op rebuild.
build_cmd() {
  case $1 in
    easypp) echo "$EASYPP build && $EASYPP test" ;;
    cmake)  echo "cmake -S . -B build -G Ninja && cmake --build build && ctest --test-dir build" ;;
    meson)  echo "meson setup build && meson test -C build" ;;
    make)   echo "make -j$(nproc) all && make test" ;;
  esac
}
noop_cmd() {
  case $1 in
    easypp) echo "$EASYPP build" ;;
    cmake)  echo "cmake --build build" ;;
    meson)  echo "meson compile -C build" ;;
    make)   echo "make -j$(nproc) all" ;;
  esac
}

fresh_copy() { # tool -> project folder with sources + that tool's build files
  local dir="$WORK/$1"
  rm -rf "$dir" && mkdir -p "$dir"
  cp -r "$GAME/src" "$dir/"
  [ "$1" != easypp ] && cp "$EVAL/$1/"* "$dir/"
  echo "$dir"
}

echo "# environment"
echo -e "clang\t$(clang --version | head -1)"
echo -e "cmake\t$(cmake --version | head -1)"
echo -e "meson\t$(meson --version)"
echo -e "ninja\t$(ninja --version)"
echo -e "make\t$(make --version | head -1)"
echo -e "cpus\t$(nproc)"

echo; echo "# configuration size (non-blank, non-comment lines / words)"
for f in cmake/CMakeLists.txt meson/meson.build make/Makefile; do
  body=$(grep -vE '^\s*(#|$)' "$EVAL/$f")
  echo -e "$f\t$(echo "$body" | wc -l)\t$(echo "$body" | wc -w)"
done
echo -e "easypp\t0\t0"

echo; echo "# distinct build-language commands/functions used"
echo -e "cmake\t$(grep -oE '^\s*[a-z_]+\(' "$EVAL/cmake/CMakeLists.txt" | tr -d ' (' | sort -u | tr '\n' ' ')"
echo -e "meson\t$(grep -oE '[a-z_]+\(' "$EVAL/meson/meson.build" | tr -d '(' | sort -u | tr '\n' ' ')"
echo -e "meson-kwargs\t$(grep -oE '[a-z_]+:' "$EVAL/meson/meson.build" | sort -u | tr '\n' ' ')"

echo; echo "# clean build + test, then no-op rebuild (seconds, median of $RUNS)"
for tool in easypp cmake meson make; do
  clean=(); noop=(); ok=yes
  for _ in $(seq $RUNS); do
    dir=$(fresh_copy $tool); cd "$dir"
    t0=$(now); quiet bash -c "$(build_cmd $tool)" || ok=no; t1=$(now)
    quiet bash -c "$(noop_cmd $tool)"; t2=$(now); quiet bash -c "$(noop_cmd $tool)"; t3=$(now)
    clean+=("$(echo "$t1 - $t0" | bc)"); noop+=("$(echo "$t3 - $t2" | bc)")
  done
  echo -e "$tool\tok=$ok\tclean=$(printf '%s\n' "${clean[@]}" | median)\tnoop=$(printf '%s\n' "${noop[@]}" | median)\tcmd=$(build_cmd $tool | sed "s|$EASYPP|easypp|g")"
done

echo; echo "# scenario: extract scoring into a new library, without touching build files"
add_score_library() {
  mkdir -p src/libs/score
  printf '#pragma once\n\nnamespace blockfall {\nint line_score(int cleared, int level);\n}\n' > src/libs/score/score.hpp
  printf '#include "score.hpp"\n\nnamespace blockfall {\nint line_score(int cleared, int level)\n{\n    constexpr int table[] = {0, 100, 300, 500, 800};\n    return table[cleared] * level;\n}\n}\n' > src/libs/score/score.cpp
  sed -i 's|#include "game.hpp"|#include "game.hpp"\n#include "score/score.hpp"|' src/libs/game/game.cpp
  sed -i '/constexpr std::array<int, 5> line_scores/d' src/libs/game/game.cpp
  sed -i 's|score_ += line_scores\[cleared\] \* level_;|score_ += line_score(cleared, level_);|' src/libs/game/game.cpp
}
for tool in easypp cmake meson make; do
  dir=$(fresh_copy $tool); cd "$dir"
  quiet bash -c "$(build_cmd $tool)"
  add_score_library
  out=$(bash -c "$(build_cmd $tool)" 2>&1); status=$?
  first=$(echo "$out" | grep -m1 -E 'undefined reference|error:|No rule' | sed "s|$WORK/||g" | cut -c1-140)
  echo -e "$tool\texit=$status\t${first:-builds and tests pass}"
done

echo; echo "# the edit each tool needs for the new library (lines added/changed)"
fix_cmake() { sed -i 's|^add_library(game STATIC|add_library(score STATIC src/libs/score/score.cpp)\ntarget_include_directories(score PUBLIC src/libs)\n\nadd_library(game STATIC|; s|target_link_libraries(game PUBLIC bag board)|target_link_libraries(game PUBLIC bag board score)|' CMakeLists.txt; }
fix_meson() { sed -i "s|^game = static_library|score = static_library('score', 'src/libs/score/score.cpp')\nscore_dep = declare_dependency(link_with: score, include_directories: inc)\n\ngame = static_library|; s|dependencies: \[bag_dep, board_dep\])|dependencies: [bag_dep, board_dep, score_dep])|g" meson.build; }
fix_make() { sed -i 's|^GAME_LIBS := $(call lib,game)|GAME_LIBS := $(call lib,game) $(call lib,score)|; s|^$(call lib,game):   |$(call lib,score):  $(call obj,src/libs/score/score.cpp)\n$(call lib,game):   |' Makefile; }
for tool in cmake meson make; do
  dir="$WORK/$tool"; cd "$dir"
  cp "$(ls | grep -E '^(CMakeLists.txt|meson.build|Makefile)$')" /tmp/before.$$
  fix_$tool
  conf=$(ls | grep -E '^(CMakeLists.txt|meson.build|Makefile)$')
  added=$(diff /tmp/before.$$ "$conf" | grep -cE '^>\s*\S')
  removed=$(diff /tmp/before.$$ "$conf" | grep -cE '^<\s*\S')
  quiet bash -c "$(build_cmd $tool)" && result="builds and tests pass" || result="still failing"
  echo -e "$tool\t+$added\t-$removed\t$result"
done
echo -e "easypp\t+0\t-0\tno build file"
rm -f /tmp/before.$$

echo; echo "# defaults: a minimal project with an uninitialised read and a VLA"
D="$WORK/defaults"
mkdir -p "$D"/{cmake,meson,make,easypp/src/bin}
cp "$EVAL/defaults/main.cpp" "$D/cmake/"; cp "$EVAL/defaults/CMakeLists.txt" "$D/cmake/"
cp "$EVAL/defaults/main.cpp" "$D/meson/"; cp "$EVAL/defaults/meson.build" "$D/meson/"
cp "$EVAL/defaults/main.cpp" "$D/make/"
cp "$EVAL/defaults/main.cpp" "$D/easypp/src/bin/"
report() { # tool, build output, binary
  local diag; diag=$(echo "$2" | grep -E '\.cpp:[0-9]+:[0-9]+: (warning|error):' | sed -E 's/.*(warning|error): .*\[(.*)\]$/\1 \2/' | sort -u | tr '\n' ';')
  local run="(not built)"; [ -x "$3" ] && run=$("$3")
  echo -e "$1\tdiagnostics=${diag:-none}\t$run"
}
cd "$D/cmake" && out=$(cmake -S . -B build -G Ninja 2>&1 && cmake --build build 2>&1); report cmake "$out" build/demo
cd "$D/meson" && out=$(meson setup build 2>&1 && meson compile -C build 2>&1); report meson "$out" build/demo
cd "$D/make" && out=$(make main 2>&1); report "make (built-in rule)" "$out" ./main
cd "$D/easypp" && out=$("$EASYPP" build 2>&1); report easypp "$out" build/debug/bin/main

[[ $WORK == /tmp/easypp-eval.* ]] && rm -rf -- "$WORK"
