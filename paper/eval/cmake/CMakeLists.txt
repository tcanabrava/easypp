cmake_minimum_required(VERSION 3.20)
project(blockfall LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
add_compile_options(-Wall -pedantic -Werror)
if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Debug)
endif()
set(SANITIZE -fsanitize=address,undefined -fno-sanitize-recover=undefined -fno-omit-frame-pointer)
add_compile_options("$<$<CONFIG:Debug>:${SANITIZE}>")
add_link_options("$<$<CONFIG:Debug>:${SANITIZE}>")

find_package(PkgConfig REQUIRED)
pkg_check_modules(SDL2 REQUIRED IMPORTED_TARGET sdl2)

add_library(pieces STATIC src/libs/pieces/pieces.cpp)
target_include_directories(pieces PUBLIC src/libs)

add_library(bag STATIC src/libs/bag/bag.cpp)
target_link_libraries(bag PUBLIC pieces)

add_library(board STATIC src/libs/board/board.cpp)
target_link_libraries(board PUBLIC pieces)

add_library(game STATIC src/libs/game/game.cpp)
target_link_libraries(game PUBLIC bag board)

add_library(pixelfont INTERFACE)
target_include_directories(pixelfont INTERFACE src/libs)
target_link_libraries(pixelfont INTERFACE PkgConfig::SDL2)

add_library(render STATIC src/libs/render/render.cpp)
target_link_libraries(render PUBLIC game PRIVATE pixelfont)

add_library(check INTERFACE)
target_include_directories(check INTERFACE src/libs)

add_executable(blockfall src/bin/blockfall.cpp)
target_link_libraries(blockfall PRIVATE render PkgConfig::SDL2)

enable_testing()
foreach(name pieces bag board game)
  add_executable(${name}_test src/tests/${name}_test.cpp)
  target_link_libraries(${name}_test PRIVATE ${name} check)
  add_test(NAME ${name}_test COMMAND ${name}_test)
endforeach()
