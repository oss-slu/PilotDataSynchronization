project(
  'baton',
  ['cpp'],
  meson_version: '>=1.7.0',
)

rust_target = ''

if host_machine.system() == 'windows' and host_machine.cpu_family() == 'x86_64'
  rust_target = 'x86_64-pc-windows-gnu'
elif host_machine.system() == 'darwin' and host_machine.cpu_family() == 'aarch64'
  rust_target = 'aarch64-apple-darwin'
elif host_machine.system() == 'darwin' and host_machine.cpu_family() == 'x86_64'
  rust_target = 'x86_64-apple-darwin'
elif host_machine.system() == 'linux' and host_machine.cpu_family() == 'x86_64'
  rust_target = 'x86_64-unknown-linux-gnu'
endif

baton_tgt = custom_target(
  'baton',
  command: [
    'python3',
    '@CURRENT_SOURCE_DIR@/script.py',
    '@OUTDIR@',
    '@CURRENT_SOURCE_DIR@',
    rust_target,
  ],
  output: ['libbaton.a', 'lib.rs.cc', 'lib.rs.h'],
  build_always_stale: true,
)

baton_dep = declare_dependency(
  sources: [baton_tgt],
  include_directories: include_directories('.')
)
