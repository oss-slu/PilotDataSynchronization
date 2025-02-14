project('greet', 'cpp')

lib = library('greet', objects: ['greet.o'])

greet_dep = declare_dependency(
  sources: ['greet.hpp'],
  link_with: [lib],
  include_directories: ['.']
)
