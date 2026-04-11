cmake_minimum_required(VERSION 3.20)
project(kde-fan-control-gui VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

# Qt6 components
find_package(Qt6 REQUIRED COMPONENTS Core Quick Qml QuickControls2 DBus Widgets)

# Kirigami is a QML-only module — no C++ link target needed.
# We locate its QML dir for the qt_add_qml_module resource install.
find_package(KF6Kirigami QUIET)
if(NOT KF6Kirigami_FOUND)
    message(STATUS "KF6Kirigami CMake package not found — runtime QML import will be used instead")
endif()

# KF6 IconThemes is optional at build time; we need it only if we plan
# to use its C++ API. For now the GUI only uses QML icon lookups.
find_package(KF6IconThemes QUIET)

# KF6 StatusNotifierItem for system tray (Plan 03 dependency)
find_package(KF6StatusNotifierItem QUIET)

qt_add_qml_module(gui_app
    URI org.kde.fancontrol
    VERSION 1.0
    QML_FILES
        qml/Main.qml
        qml/OverviewPage.qml
        qml/InventoryPage.qml
        qml/delegates/FanRowDelegate.qml
        qml/components/StateBadge.qml
        qml/components/OutputBar.qml
        qml/components/TemperatureDisplay.qml
    SOURCES
        src/main.cpp
        src/daemon_interface.h
        src/daemon_interface.cpp
        src/status_monitor.h
        src/status_monitor.cpp
        src/types.h
        src/types.cpp
        src/models/fan_list_model.h
        src/models/fan_list_model.cpp
        src/models/sensor_list_model.h
        src/models/sensor_list_model.cpp
)

target_include_directories(gui_app PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/src
    ${CMAKE_CURRENT_SOURCE_DIR}/src/models
)

target_link_libraries(gui_app PRIVATE
    Qt6::Core
    Qt6::Quick
    Qt6::Qml
    Qt6::DBus
    Qt6::Widgets
)

# KF6 IconThemes (provides KIconColors, QIcon from theme) — link if found
if(KF6IconThemes_FOUND)
    target_link_libraries(gui_app PRIVATE KF6::IconThemes)
endif()

# KF6 StatusNotifierItem — link if found (tray support)
if(KF6StatusNotifierItem_FOUND)
    target_link_libraries(gui_app PRIVATE KF6::StatusNotifierItem)
endif()

# Kirigami QML module — note: at runtime the QML engine resolves the
# org.kde.kirigami import via the QML plugin path. We don't need a
# compile-time link because Kirigami is a pure QML/attached-properties
# module without a public C++ link target in KF6.

install(TARGETS gui_app
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
)