# init
project('PilotDataSyncPlugin', 'cpp', default_options: ['cpp_std=c++20'])

# imports
fs = import('fs')
 
# download sdk if not present
# TODO

compiler_args = [
  '-DXPLM200',
  '-DXPLM210',
  '-DXPLM300',
  '-DXPLM301',
  '-DXPLM303',
  '-DIBM=1',
  '-D_WINSOCK_DEPRECATED_NO_WARNINGS',
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

# include directory specifically for the cross compiler
xdir = []
if fs.is_dir('/usr/x86_64-w64-mingw32/include')
  xdir.append(include_directories('/usr/x86_64-w64-mingw32/include'))
endif
if fs.is_dir('/opt/homebrew/bin/x86_64-w64-mingw32-g++/include')
  xdir.append(include_directories('/opt/homebrew/x86_64-w64-mingw32-g++/include'))
endif

# final output. KEEP IN MIND THAT THE FINAL OUTPUT STILL NEEDS TO BE MANUALLY RENAMED
shared_library(
  'PilotDataSync.xpl',
  'src/pilotdatasync-xp11.cpp',
  'src/threading-tools.cpp',
  dependencies: [winsock, xplm, xpwidgets],
  include_directories: [inc_dir, lib_dir],
  cpp_args: compiler_args,
  link_args: ['-static-libstdc++', '-static-libgcc', '-static'],
)
