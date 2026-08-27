# Link builds itself with cargo; this file is the face it shows a CMake consumer, so that the
# consumer can stay oblivious to how the wire is made. add_subdirectory() this repository and
# it offers exactly three things:
#
#   - the target `lnk`: link against it to compile against include/lnk/. Headers only,
#     deliberately - nothing links Link at build time; the library is loaded at run time
#     behind one exported symbol, exactly as a Program is
#   - lnk_copy_beside(<target>): the residence rule as a function - copies the built library
#     beside the given executable's output, ordered after the build, because Link lives beside
#     the executable that loads it
#   - LNK_LIBRARY_FILE and LNK_FINGERPRINT_FILE: the built library and the recorded protocol
#     fingerprint, for consumers that need the paths themselves (tests pin against both)
#
# LANGUAGES NONE, deliberately: consuming Link enables no compiler in the consumer's build, and
# building this file standalone probes for none.
cmake_minimum_required(VERSION 3.25)

project(Link LANGUAGES NONE)

find_program(LNK_CARGO_EXECUTABLE cargo REQUIRED)

if(WIN32)
    set(LNK_LIBRARY_FILE ${CMAKE_CURRENT_SOURCE_DIR}/target/release/link.dll)
else()
    set(LNK_LIBRARY_FILE ${CMAKE_CURRENT_SOURCE_DIR}/target/release/liblink.so)
endif()
set(LNK_FINGERPRINT_FILE ${CMAKE_CURRENT_SOURCE_DIR}/include/lnk/protocol_fingerprint.txt)

# A consumer's tests read these; a subdirectory's variables do not cross scopes on their own.
# Guarded, because at the top of a standalone build there is no parent to hand them to.
if(NOT CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR)
    set(LNK_LIBRARY_FILE ${LNK_LIBRARY_FILE} PARENT_SCOPE)
    set(LNK_FINGERPRINT_FILE ${LNK_FINGERPRINT_FILE} PARENT_SCOPE)
endif()

# cargo is its own dependency tracker and a fraction of a second when nothing changed, so the
# target simply always runs it.
add_custom_target(link_library ALL
    COMMAND ${LNK_CARGO_EXECUTABLE} build --release
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    COMMENT "Building Link (cargo build --release)"
    VERBATIM
)

# The consumer-facing target. INTERFACE and headers only: the coupling at build time is two C
# headers, and the coupling at run time is one LoadLibrary'd symbol.
add_library(lnk INTERFACE)
target_include_directories(lnk INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}/include)

# An ALL target per consumer rather than POST_BUILD, which only fires when the consumer itself
# relinks - a fresh Link beside a stale consumer would otherwise never arrive.
function(lnk_copy_beside consumer)
    add_custom_target(lnk_copy_beside_${consumer} ALL
        COMMAND ${CMAKE_COMMAND} -E copy_if_different ${LNK_LIBRARY_FILE} $<TARGET_FILE_DIR:${consumer}>/
        COMMENT "Copying Link beside ${consumer}"
        VERBATIM
    )
    add_dependencies(lnk_copy_beside_${consumer} link_library ${consumer})

    # Consumers that share an output directory - a repository's test executables, typically -
    # otherwise get one copy each writing the very same destination file, ordered after nothing
    # but their own consumer, and a build system worth having runs them at the same moment: two
    # copies of one file into one directory, and one of them fails. It did, once, on the
    # flagship's verify leg (2026-08-27): three test executables share src/tests/Release, and the
    # copy beside the first collided with the copy beside another. So the copies are chained -
    # each waits for the one before it - and only ever one is in flight. Each is a file compare
    # that costs nothing, and the chain is a global property because the consumers are spread
    # across directories.
    get_property(lnk_previous_copy GLOBAL PROPERTY LNK_LAST_COPY_TARGET)
    if(lnk_previous_copy)
        add_dependencies(lnk_copy_beside_${consumer} ${lnk_previous_copy})
    endif()
    set_property(GLOBAL PROPERTY LNK_LAST_COPY_TARGET lnk_copy_beside_${consumer})
endfunction()
