import subprocess
import sys

outdir = sys.argv[1]
curr_src_dir = sys.argv[2]

print(f"outdir: {outdir}, curr_src_dir: {curr_src_dir}")

subprocess.run([
    'cargo', 
    'build', 
    '--release',
    '--target-dir',
    outdir,
    '--manifest-path',
    f'{curr_src_dir}/Cargo.toml' 
])

subprocess.run([
    'cp',
    f'{outdir}/release/libmeet.dylib',
    outdir
])