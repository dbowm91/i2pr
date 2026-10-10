cmake_minimum_required(VERSION 3.20)
project(i2pd_current_ntcp2_driver LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT DEFINED I2PD_SOURCE_DIR OR NOT DEFINED I2PD_LIBRARY_DIR)
    message(FATAL_ERROR "I2PD_SOURCE_DIR and I2PD_LIBRARY_DIR are required")
endif()

find_package(Boost 1.83 REQUIRED COMPONENTS system thread filesystem program_options)
find_package(OpenSSL REQUIRED)
find_package(Threads REQUIRED)
find_package(ZLIB REQUIRED)

file(GLOB I2PD_ARCHIVES "${I2PD_LIBRARY_DIR}/libi2pd*.a")
if(NOT I2PD_ARCHIVES)
    message(FATAL_ERROR "no pristine i2pd archives found in I2PD_LIBRARY_DIR")
endif()

add_executable(i2pd_current_ntcp2_driver
    src/i2pd_current_ntcp2_driver.cpp
)
target_compile_definitions(i2pd_current_ntcp2_driver PRIVATE I2PD_PLAN076_LINKED=1)
target_compile_options(i2pd_current_ntcp2_driver PRIVATE
    -O2 -fvisibility=hidden -fstack-protector-strong -Wall -Wextra
    -Werror=return-type -Werror=switch
)
target_include_directories(i2pd_current_ntcp2_driver PRIVATE
    "${I2PD_SOURCE_DIR}/libi2pd"
    "${I2PD_SOURCE_DIR}/libi2pd_client"
    "${CMAKE_CURRENT_SOURCE_DIR}/src"
)
target_link_libraries(i2pd_current_ntcp2_driver PRIVATE
    ${I2PD_ARCHIVES}
    ${Boost_LIBRARIES}
    OpenSSL::SSL OpenSSL::Crypto ZLIB::ZLIB Threads::Threads
)
