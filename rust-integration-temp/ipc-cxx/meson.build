project('ipc-demo', 'cpp', meson_version: '1.7.0')

compiler = meson.get_compiler('cpp')

# lib_files = files('./libipc_cpp_rs.a')
# lib_path = lib_files[0].full_path().replace('./libipc_cpp_rs.a', '')
# message(lib_path)

subp = subproject('ipc-cpp-rs')
message(subp.found())

#subp_dep = subp.get_variable('lib_rs')
subp_dep = subp.get_variable('bib')

# subp_header = subp.get_variable('lib.rs.h')

# dep_rs = compiler.find_library(
#   'ipc_cpp_rs',
#   # dirs: lib_path,
#   has_headers: 'lib.rs.h',
#   header_include_directories: include_directories('.')
# )

executable(
  'ipc-demo', 
  sources: 'main.cpp',
  #dependencies: subp_dep,
  link_with: subp_dep
)
