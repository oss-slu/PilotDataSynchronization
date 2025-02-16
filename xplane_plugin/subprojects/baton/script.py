import subprocess
import sys

outdir = sys.argv[1]
curr_src_dir = sys.argv[2]

print(f"outdir: {outdir}, curr_src_dir: {curr_src_dir}")

# target/cxxbridge/baton/src/lib.rs.h

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

subprocess.run([
    'cp',
    f'{outdir}/release/libbaton.a',
    outdir
])

subprocess.run([
    'cp',
    f'{outdir}/cxxbridge/baton/src/lib.rs.h',
    outdir
])
