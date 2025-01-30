cmake_minimum_required(VERSION 3.10)
# project(XPlanePlugin)

# # Global definitions
# add_definitions(-DIBM=1)
# add_definitions(-DXPLM200=1 -DXLPM210=1 -DXPLM300=1 -DXPLM301=1 -DXPLM303=1)

# # C++ Standard
# set(CMAKE_CXX_STANDARD 17)
# set(CMAKE_CXX_STANDARD_REQUIRED ON)
# set(CMAKE_POSITION_INDEPENDENT_CODE ON) ##libraries mixed into executables might not need? 

# # X-Plane SDK path (adjust as necessary)
# set(XPLANE_SDK_PATH "${CMAKE_SOURCE_DIR}/lib/SDK")

# # X-Plane SDK libraries
# set(XPLANE_LIBS_DIR "${XPLANE_SDK_PATH}/Libraries/Win")
# set(XPLANE_LIBS
#     "${XPLANE_LIBS_DIR}/XPLM_64.lib"
#     "${XPLANE_LIBS_DIR}/XPWidgets_64.lib"
# )

# # X-Plane Plugin
# add_library(xplane_plugin SHARED
#     src/pilotdatasync-xp11.cpp
# )

# target_include_directories(xplane_plugin PRIVATE
#     ${XPLANE_SDK_PATH}/CHeaders/XPLM
#     ${XPLANE_SDK_PATH}/CHeaders/Widgets
#     include
# )

# target_link_libraries(xplane_plugin PRIVATE ${XPLANE_LIBS})

# set_target_properties(xplane_plugin PROPERTIES
#     OUTPUT_NAME "pilotdatasync-xp11"
#     SUFFIX ".xpl"
# )

# # TCPServer Executable
# add_executable(tcp_server
#     src/TCPServer.cpp
# )

# # TCPClient Executable
# add_executable(tcp_client
#     src/TCPClient.cpp
# )

# # Network libraries
# if(WIN32)
#     target_link_libraries(tcp_server PRIVATE ws2_32)
#     target_link_libraries(tcp_client PRIVATE ws2_32)
# else()
#     # For UNIX-like systems
#     target_link_libraries(tcp_server PRIVATE pthread)
#     target_link_libraries(tcp_client PRIVATE pthread)
# endif()

# # Create scripts for running the executables
# if(WIN32)
#     # Windows batch script
#     file(WRITE ${CMAKE_BINARY_DIR}/run_network_components.bat
#         "start \"TCP Server\" cmd /k \"${CMAKE_BINARY_DIR}/${CMAKE_CFG_INTDIR}/tcp_server.exe\"\n"
#         "timeout /t 2\n"
#         "start \"TCP Client\" cmd /k \"${CMAKE_BINARY_DIR}/${CMAKE_CFG_INTDIR}/tcp_client.exe\"\n"
#     )
    
#     add_custom_command(TARGET tcp_client POST_BUILD
#         COMMAND ${CMAKE_BINARY_DIR}/run_network_components.bat
#         WORKING_DIRECTORY ${CMAKE_BINARY_DIR}
#         COMMENT "Running TCP Server and Client..."
#     )
# else()
#     # Unix shell script
#     file(WRITE ${CMAKE_BINARY_DIR}/run_network_components.sh
#         "#!/bin/bash\n"
#         "${CMAKE_BINARY_DIR}/tcp_server & \n"
#         "sleep 2\n"
#         "${CMAKE_BINARY_DIR}/tcp_client\n"
#     )
    
#     # Make the script executable
#     file(CHMOD ${CMAKE_BINARY_DIR}/run_network_components.sh
#         PERMISSIONS OWNER_READ OWNER_WRITE OWNER_EXECUTE GROUP_READ GROUP_EXECUTE WORLD_READ WORLD_EXECUTE
#     )
    
#     add_custom_command(TARGET tcp_client POST_BUILD
#         COMMAND ${CMAKE_BINARY_DIR}/run_network_components.sh
#         WORKING_DIRECTORY ${CMAKE_BINARY_DIR}
#         COMMENT "Running TCP Server and Client..."
#     )
# endif()

##testing stuff
set(This PilotDataSynchronization)

enable_testing()

add_subdirectory(googletest)

# set (HEADERS
#     headingtests.cpp
# )

set (SOURCES     ##add files being tested here :p 
    src/Logger.cpp
)


add_library(${This} STATIC ${SOURCES})  ##add headers here if headers are added 

add_subdirectory(tests)


