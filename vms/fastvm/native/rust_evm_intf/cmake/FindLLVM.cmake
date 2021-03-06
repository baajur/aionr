if (USE_LLVM)
    set(LLVM_INCLUDE_DIR /usr/include/llvm-4.0)
    set(LLVM_INCLUDE_C_DIR /usr/include/llvm-c-4.0)
    find_path(LLVM_INCLUDE_DIR InitializePasses.h)
    find_library(LLVM_LIBRARY NAMES LLVM-4.0)
    include(FindPackageHandleStandardArgs)
    find_package_handle_standard_args(LLVM DEFAULT_MSG LLVM_LIBRARY LLVM_INCLUDE_DIR)
endif()