project('hello', 'cpp')

compiler = meson.get_compiler('cpp')

# lib_rs = declare_dependency(
#   sources: 'libinner_rust.a' 
# )

executable(
  'hello',
  sources: ['main.cpp'],
#  dependencies: [lib_rs],
  dependencies: compiler.find_library('inner_rust', dirs: [meson.current_source_dir()]),
  include_directories: include_directories('.')
)
