project(
  'baton',
  ['cpp'],
  meson_version: '>=1.7.0',
)

baton_tgt = custom_target(
  'baton',
  command: [
    'python3',
    '@CURRENT_SOURCE_DIR@/script.py',
    '@OUTDIR@',
    '@CURRENT_SOURCE_DIR@'
  ],
  output: ['libbaton.a', 'lib.rs.cc', 'lib.rs.h'],
  build_always_stale: true,
)

baton_dep = declare_dependency(
  sources: [baton_tgt],
  include_directories: include_directories('.')
)
