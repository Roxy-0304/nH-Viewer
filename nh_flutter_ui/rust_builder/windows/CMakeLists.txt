cmake_minimum_required(VERSION 3.14)
set(PROJECT_NAME "rust_lib_nh_flutter_ui")
project(${PROJECT_NAME} LANGUAGES CXX)

include("../cargokit/cmake/cargokit.cmake")
apply_cargokit(${PROJECT_NAME} "${CMAKE_SOURCE_DIR}/../../crates/nh-bridge" rust_lib_nh_flutter_ui "")

set(rust_lib_nh_flutter_ui_bundled_libraries
  "${${PROJECT_NAME}_cargokit_lib}"
  PARENT_SCOPE
)