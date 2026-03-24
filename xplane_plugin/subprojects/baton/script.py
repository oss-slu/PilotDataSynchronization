import os
import subprocess
import sys
import shutil

outdir = sys.argv[1]
curr_src_dir = sys.argv[2]
target = sys.argv[3] if len(sys.argv) > 3 else ''

command = [
    'cargo',
    'build',
    '--release',
    '--target-dir',
    outdir,
    '--manifest-path',
    f'{curr_src_dir}/Cargo.toml'
]

if target:
    command.extend(['--target', target])

env = dict(os.environ)
if target:
    env['BATON_TARGET'] = target

subprocess.run(command, check=True, env=env)

artifact_root = f'{outdir}/{target}' if target else outdir

shutil.copyfile(
    f'{artifact_root}/release/libbaton.a',
    f'{outdir}/libbaton.a'
)

shutil.copyfile(
    f'{artifact_root}/cxxbridge/baton/src/lib.rs.cc',
    f'{outdir}/lib.rs.cc'
)

shutil.copyfile(
    f'{artifact_root}/cxxbridge/baton/src/lib.rs.h',
    f'{outdir}/lib.rs.h'
)
