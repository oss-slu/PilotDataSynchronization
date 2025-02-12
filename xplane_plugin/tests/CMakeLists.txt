cmake_minimum_required(VERSION 3.10)
set(This ExampleTests)

set (SOURCES    ##test files here 
    headingtests.cpp
)

add_executable(${This} ${SOURCES})
target_link_libraries(${This} PUBLIC
    gtest_main
    PilotDataSynchronization
)

add_test(
    NAME ${This}
    COMMAND ${This}
)