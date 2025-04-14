import subprocess
import sys
import shutil

outdir = sys.argv[1]
curr_src_dir = sys.argv[2]

print(f"outdir: {outdir}, curr_src_dir: {curr_src_dir}")

subprocess.run([
    'cargo', 
    'build',
    '--target',
    'x86_64-pc-windows-gnu',
    '--release',
    '--target-dir',
    outdir,
    '--manifest-path',
    f'{curr_src_dir}/Cargo.toml' 
])

shutil.copyfile(
    f'{outdir}/x86_64-pc-windows-gnu/release/libbaton.a',
    f'{outdir}/libbaton.a'
)

shutil.copyfile(
    f'{outdir}/x86_64-pc-windows-gnu/cxxbridge/baton/src/lib.rs.cc',
    f'{outdir}/lib.rs.cc'
)

shutil.copyfile(
    f'{outdir}/x86_64-pc-windows-gnu/cxxbridge/baton/src/lib.rs.h',
    f'{outdir}/lib.rs.h'
)
