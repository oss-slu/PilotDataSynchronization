project('meson-subp', 'cpp', meson_version: '1.7.0')

compiler = meson.get_compiler('cpp')

greet_subp = subproject('greet')
greet = greet_subp.get_variable('greet_dep')

meet_subp = subproject('meet')
meet = meet_subp.get_variable('meet_dep')

executable('out', sources: 'main.cpp', dependencies: [greet, meet])
