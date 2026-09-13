# Added to the pinned Dolphin source tree by build.py; never a shipping dependency.
add_library(emulia_dolphin_probe SHARED bridge.mm)
target_link_libraries(emulia_dolphin_probe PRIVATE core uicommon ${APPKIT_LIBRARY})
target_compile_options(emulia_dolphin_probe PRIVATE -fobjc-arc)
