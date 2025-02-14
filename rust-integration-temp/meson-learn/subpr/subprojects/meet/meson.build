project(
  'meet',
  ['cpp'],
  meson_version: '1.7.0',
)

meet_tgt = custom_target(
  'meet_tgt',
  command: [
    'python3',
    '@CURRENT_SOURCE_DIR@/script.py',
    '@OUTDIR@',
    '@CURRENT_SOURCE_DIR@'
  ],
  output: 'libmeet.dylib'
)

meet_dep = declare_dependency(
  sources: [meet_tgt, 'meet.hpp'],
  include_directories: include_directories('.')
)
