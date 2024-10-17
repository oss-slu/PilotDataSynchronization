cmake_minimum_required(VERSION 3.10)
project(XPlanePlugin)

# Add this line to define the IBM macro globally for the entire project
add_definitions(-DIBM=1)
add_definitions(-DXPLM410=1)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# X-Plane SDK path (you'll need to adjust this)
set(XPLANE_SDK_PATH "lib/SDK")

# Specify the path to the X-Plane SDK libraries
set(XPLANE_LIBS_DIR "${XPLANE_SDK_PATH}/lib")
set(XPLANE_LIBS "${XPLANE_LIBS_DIR}/XPLM_410.dylib") # Adjust if necessary

# Plugin source
add_library(xplane_plugin SHARED
    src/hello-world.cpp
)


target_include_directories(xplane_plugin PRIVATE
    ${XPLANE_SDK_PATH}/CHeaders/XPLM
    ${XPLANE_SDK_PATH}/CHeaders/Widgets
    include
)

# Link against the X-Plane SDK libraries
target_link_libraries(xplane_plugin PRIVATE ${XPLANE_LIBS})


# Enable testing
enable_testing()

# Google Test
include(FetchContent)
FetchContent_Declare(
    googletest
    URL https://github.com/google/googletest/archive/03597a01ee50ed33e9dfd640b249b4be3799d395.zip
)
FetchContent_MakeAvailable(googletest)

# Test executable
add_executable(plugin_tests
    tests/plugin_tests.cpp 
)

target_link_libraries(plugin_tests PRIVATE
    gtest_main
    xplane_plugin
)

include(GoogleTest)
gtest_discover_tests(plugin_tests)