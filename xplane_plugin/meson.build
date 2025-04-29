# init
project('PilotDataSyncPlugin', 'cpp', default_options: ['cpp_std=c++20'], meson_version: '>=1.7.0')

# imports
fs = import('fs')

# these defines are necessary as required by XPlane
compiler_args = [
  '-std=c++20',
  '-DXPLM200',
  '-DXPLM210',
  '-DXPLM300',
  '-DXPLM301',
  '-DXPLM303',
  '-DIBM=1',
]

# TODO review if this is still necessary
add_project_arguments('-Llib/SDK/Libraries/Win', language: 'cpp')

# includes
inc_dir = include_directories('include')

compiler = meson.get_compiler('cpp')

# dependencies
winsock = compiler.find_library(
  'ws2_32',
  has_headers: ['winsock2.h', 'ws2tcpip.h'],
)

# x-plane sdk handled by wrapDB instead of installing it manually
xplane_sdk_proj = subproject('x-plane-sdk')
xplm = xplane_sdk_proj.get_variable('xplm_dep')
xpwidgets = xplane_sdk_proj.get_variable('xpwidgets_dep')
xpcpp = xplane_sdk_proj.get_variable('xpcpp_dep')

# rust library from subproject
baton_subp = subproject('baton')
baton = baton_subp.get_variable('baton_dep')

# final output. KEEP IN MIND THAT THE FINAL OUTPUT STILL NEEDS TO BE MANUALLY RENAMED
lib = shared_library(
  'DataSync',
  'src/pilotdatasync-xp11.cpp',
  dependencies: [winsock, xplm, xpwidgets, xpcpp, baton],
  include_directories: [inc_dir],
  cpp_args: compiler_args,
  link_args: ['-lntdll', '-static-libstdc++', '-static-libgcc', '-static'],
  name_prefix: 'Pilot',
  name_suffix: 'xpl'
)

if host_machine.system() == 'windows'
    test_args = ['-DGTEST_ENABLE_CATCH_EXCEPTIONS_=0', '-std=c++20']
else
    test_args = []
endif

# gtest
# gtest_proj = subproject('gtest')
# gtest_dep = gtest_proj.get_variable('gtest_dep')
# test_exec = executable('testsuite',
#     'tests/test.cc',
#     dependencies: [gtest_dep],
#     cpp_args: test_args,
#     native: true 
# )
# test('gtest test', test_exec)
