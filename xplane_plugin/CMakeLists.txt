cmake_minimum_required(VERSION 3.10)
project(PilotDataSync)

# Path to the X-Plane SDK folder (adjust to your repo layout)
set(XPLANE_SDK_DIR "${CMAKE_SOURCE_DIR}/subprojects/x-plane-sdk-4.0.1/SDK")

# Source files (adjust to actual source file names)
set(SOURCES
    src/pilotdatasync-xp11.cpp
)

add_library(PilotDataSync SHARED ${SOURCES})

# Produce .xpl on all platforms
set_target_properties(PilotDataSync PROPERTIES SUFFIX ".xpl")

# Include SDK headers
target_include_directories(PilotDataSync PRIVATE "${XPLANE_SDK_DIR}/CHeaders")

# Platform-specific link settings (adjust library names/paths as needed)
if(WIN32)
    target_link_directories(PilotDataSync PRIVATE "${XPLANE_SDK_DIR}/Libraries/Win64")
    target_link_libraries(PilotDataSync PRIVATE XPLM)   # ensure XPLM.lib present
elseif(APPLE)
    target_link_directories(PilotDataSync PRIVATE "${XPLANE_SDK_DIR}/Libraries/Mac")
    target_link_libraries(PilotDataSync PRIVATE XPLM)
else()
    target_link_directories(PilotDataSync PRIVATE "${XPLANE_SDK_DIR}/Libraries/Linux")
    target_link_libraries(PilotDataSync PRIVATE XPLM)
endif()

# Optional: copy result to your X-Plane plugins folder after build.
# Configure XPLANE_INSTALL_DIR when invoking cmake or edit below.
if(NOT DEFINED XPLANE_INSTALL_DIR)
  set(XPLANE_INSTALL_DIR "C:/Program Files/X-Plane 11/Resources/plugins" CACHE PATH "X-Plane install plugins folder")
endif()
add_custom_command(TARGET PilotDataSync POST_BUILD
  COMMAND ${CMAKE_COMMAND} -E copy $<TARGET_FILE:PilotDataSync> "${XPLANE_INSTALL_DIR}/PilotDataSync.xpl"
)