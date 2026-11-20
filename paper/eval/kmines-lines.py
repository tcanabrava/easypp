#!/usr/bin/env python3
"""Count pinned upstream KMines build files. Pass a Git repository containing the commit.

Reproduce without modifying examples/kmines:
  git init /tmp/kmines-count
  git -C /tmp/kmines-count fetch --depth 1 https://invent.kde.org/games/kmines.git dbb447ca171469a55a5efbadbcbb480331dc7a38
  python3 paper/eval/kmines-lines.py /tmp/kmines-count
"""
import subprocess
import sys

commit = 'dbb447ca171469a55a5efbadbcbb480331dc7a38'
repo = sys.argv[1]
def git(*args):
    return subprocess.check_output(['git', '-C', repo, *args], text=True)

print(f'# KMines upstream {commit}')
print('# Effective lines exclude blank lines and full-line # comments; inline comments remain.')
print('file\tphysical\teffective')
physical = effective = 0
for path in git('ls-tree', '-r', '--name-only', commit).splitlines():
    if not (path.endswith('CMakeLists.txt') or path.endswith('.cmake')):
        continue
    lines = git('show', f'{commit}:{path}').splitlines()
    code = sum(bool(line.strip()) and not line.lstrip().startswith('#') for line in lines)
    print(f'{path}\t{len(lines)}\t{code}')
    physical += len(lines)
    effective += code
print(f'TOTAL\t{physical}\t{effective}')
print('easypp project build files\t0\t0')
print(f'difference (easypp - upstream)\t{-physical}\t{-effective}')
print('# Excludes presets/CI/packaging; upstream CMake also covers installation, translations, documentation.')
print('# This is a build/launch adaptation, not installation feature parity.')

# Snapshot the implementation cost separately from the project build-file count.
# Preserve the pre-existing debugger changes when reconstructing main.rs's baseline.
import difflib
from pathlib import Path
root = Path(__file__).resolve().parents[2]
baseline = 'ee23bcbee7dfd37b2a46917599ac1d07abe29feb'
print('\n# Tool diff: physical added/deleted lines, including comments, blanks and unit tests.')
print('# Baseline:', baseline)
print('# main.rs excludes the pre-existing debugger preference/documentation changes.')
print('file\tadded\tdeleted\tnet')
added = deleted = 0
for name in ['builder', 'export', 'main', 'project', 'qt', 'system', 'kde']:
    path = f'src/{name}.rs'
    after = (root / path).read_text()
    if name == 'kde':
        before = ''
    elif name == 'main':
        before = after.replace('mod kde;\n', '')
        start = before.index('/// data/ mirrors')
        end = before.index('fn run_bin(', start)
        before = before[:start] + before[end:]
        before = before.replace('    runtime_data(&mut command, project);\n', '')
        before = before.replace('    let mut command = Command::new(&exe);\n    let status = command\n', '    let status = Command::new(&exe)\n')
    else:
        before = subprocess.check_output(['git', 'show', f'{baseline}:{path}'], cwd=root, text=True)
    diff = list(difflib.unified_diff(before.splitlines(), after.splitlines(), n=0))
    a = sum(line.startswith('+') and not line.startswith('+++') for line in diff)
    d = sum(line.startswith('-') and not line.startswith('---') for line in diff)
    print(f'{path}\t{a}\t{d}\t{a-d}')
    added += a
    deleted += d
print(f'TOTAL TOOL\t{added}\t{deleted}\t{added-deleted}')
