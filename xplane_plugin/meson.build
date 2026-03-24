project(
  'PilotDataSyncPlugin',
  'cpp',
  default_options: ['cpp_std=c++20'],
  meson_version: '>=1.7.0',
)

inc_dir = include_directories('include')
compiler = meson.get_compiler('cpp')
host_system = host_machine.system()

compiler_args = [
  '-std=c++20',
  '-DXPLM200',
  '-DXPLM210',
  '-DXPLM300',
  '-DXPLM301',
  '-DXPLM303',
]

plugin_src = 'src/pilotdatasync-xp11.cpp'
deps = []
link_args = []

if host_system == 'windows'
  winsock = compiler.find_library(
    'ws2_32',
    has_headers: ['winsock2.h', 'ws2tcpip.h'],
  )
  opengl_dep = compiler.find_library('opengl32')
  deps += [winsock, opengl_dep]
  link_args += ['-lntdll', '-static-libstdc++', '-static-libgcc', '-static']
elif host_system == 'darwin'
  plugin_src = 'src/pilotdatasync-xp11.macos.cpp'
  opengl_dep = dependency('appleframeworks', modules: ['OpenGL'])
  deps += [opengl_dep]
endif

xplane_sdk_proj = subproject('x-plane-sdk')
xplm = xplane_sdk_proj.get_variable('xplm_dep')
xpwidgets = xplane_sdk_proj.get_variable('xpwidgets_dep')
xpcpp = xplane_sdk_proj.get_variable('xpcpp_dep')

baton_subp = subproject('baton')
baton = baton_subp.get_variable('baton_dep')

deps += [xplm, xpwidgets, xpcpp, baton]

shared_library(
  'DataSync',
  plugin_src,
  dependencies: deps,
  include_directories: [inc_dir],
  cpp_args: compiler_args,
  link_args: link_args,
  name_prefix: 'Pilot',
  name_suffix: 'xpl',
)

if host_system == 'windows'
  test_args = ['-DGTEST_ENABLE_CATCH_EXCEPTIONS_=0', '-std=c++20']
else
  test_args = []
endif

gtest_proj = subproject('gtest')
gtest_dep = gtest_proj.get_variable('gtest_dep')
gtest_main_dep = gtest_proj.get_variable('gtest_main_dep')

inc = include_directories('include')
test_exe = executable(
  'test_threading_tools',
  'tests/test_threading_tools.cpp',
  include_directories: inc,
  dependencies: [gtest_dep, gtest_main_dep],
  cpp_args: test_args,
)
test('test_threading_tools', test_exe)
