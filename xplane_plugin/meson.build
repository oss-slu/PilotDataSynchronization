# init
project('PilotDataSyncPlugin', 'cpp', default_options: ['cpp_std=c++20'], meson_version: '1.7.0')

# imports
fs = import('fs')
 
# download sdk if not present
# TODO

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
inc_dir = include_directories('include', 'lib/SDK/CHeaders/Widgets/', 'lib/SDK/CHeaders/XPLM/')

compiler = meson.get_compiler('cpp')

# slightly hacky method of retrieving the required absolute path to the precomp
# libs until I find a better way
lib_files = files('lib/SDK/Libraries/Win/XPLM_64.lib')
lib_path = lib_files[0].full_path().replace('XPLM_64.lib', '')
message(lib_path)
  
lib_dir = include_directories('lib/SDK/Libraries/Win/')

# dependencies
winsock = compiler.find_library(
  'ws2_32',
  has_headers: ['winsock2.h', 'ws2tcpip.h'],
)

# retrieve the xplm precompiled library and wreap it as a dependency
xplm = compiler.find_library(
  'XPLM_64',
  dirs: lib_path,
  has_headers: [
    'XPLMPlugin.h',
    'XPLMDefs.h',
    'XPLMDataAccess.h',
    'XPLMUtilities.h',
    'XPLMDisplay.h'
  ],
  header_include_directories: inc_dir,
  required: true
)
xpwidgets = compiler.find_library(
  'XPWidgets_64',
  dirs: lib_path,
  required: true
)

# rust library from subproject
baton_subp = subproject('baton')
baton = baton_subp.get_variable('baton_dep')

# final output. KEEP IN MIND THAT THE FINAL OUTPUT STILL NEEDS TO BE MANUALLY RENAMED
lib = shared_library(
  'DataSync',
  'src/pilotdatasync-xp11.cpp',
  'src/threading-tools.cpp',
  dependencies: [winsock, xplm, xpwidgets, baton],
  # baton dep temporarily removed while cross-compilation bug is handled
  # dependencies: [winsock, xplm, xpwidgets],
  include_directories: [inc_dir, lib_dir],
  cpp_args: compiler_args,
  # link_args: ['-lntdll', '-static-libstdc++', '-static-libgcc', '-static'],
  name_prefix: 'Pilot',
  name_suffix: 'xpl'
)

if host_machine.system() == 'windows'
    test_args = ['-DGTEST_ENABLE_CATCH_EXCEPTIONS_=0']
else
    test_args = []
endif


# gtest
gtest_proj = subproject('gtest')
gtest_dep = gtest_proj.get_variable('gtest_dep')
test_exec = executable('testsuite',
    'tests/test.cc',
    dependencies: [gtest_dep],
    cpp_args: test_args,
    native: true 
)
test('gtest test', test_exec)
