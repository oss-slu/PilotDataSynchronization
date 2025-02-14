project('ipc-cpp-rs', 'cpp')

cargo = find_program('cargo')

run_command(cargo, 'build', '--release', check: true)

path = join_paths(meson.current_source_dir(), 'target/release/')
compiler = meson.get_compiler('cpp')
lib = compiler.find_library(
  'ipc_cpp_rs',
  has_headers: 'lib.rs.h',
  header_include_directories: include_directories('target/cxxbridge/ipc-cpp-rs/src'),
  dirs: [path],
  required: true
)

lib_rs = declare_dependency(dependencies: lib)

bib = static_library('bib', dependencies: lib_rs)
