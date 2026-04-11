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

# KF6 Notifications for desktop notifications (Plan 03 dependency)
find_package(KF6Notifications QUIET)

qt_add_qml_module(gui_app
    URI org.kde.fancontrol
    VERSION 1.0
    QML_FILES
        qml/Main.qml
        qml/OverviewPage.qml
        qml/InventoryPage.qml
        qml/FanDetailPage.qml
        qml/TrayPopover.qml
        qml/delegates/FanRowDelegate.qml
        qml/delegates/FanTrayDelegate.qml
        qml/components/StateBadge.qml
        qml/components/OutputBar.qml
        qml/components/TemperatureDisplay.qml
        qml/components/PidField.qml
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
        src/models/draft_model.h
        src/models/draft_model.cpp
        src/models/lifecycle_event_model.h
        src/models/lifecycle_event_model.cpp
        src/tray_icon.h
        src/tray_icon.cpp
        src/notification_handler.h
        src/notification_handler.cpp
)

target_include_directories(gui_app PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/src
    ${CMAKE_CURRENT_SOURCE_DIR}/src/models
    /usr/include/KF5
    /usr/include/KF5/KNotifications
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

# KF6 StatusNotifierItem + KF6 Notifications for tray icon and desktop notifications.
# On this system the KF6 runtime shared libraries are available but dev packages
# (CMake configs, .so symlinks) are not installed. The KF5 compat headers at
# /usr/include/KF5/KNotifications/ provide the C++ API. We link directly against
# the KF6 shared library files.
set(KF6_SNI_LIB "/usr/lib/x86_64-linux-gnu/libKF6StatusNotifierItem.so.6")
set(KF6_NOTIF_LIB "/usr/lib/x86_64-linux-gnu/libKF6Notifications.so.6")
target_link_libraries(gui_app PRIVATE
    ${KF6_SNI_LIB}
    ${KF6_NOTIF_LIB}
)

# Kirigami QML module — note: at runtime the QML engine resolves the
# org.kde.kirigami import via the QML plugin path. We don't need a
# compile-time link because Kirigami is a pure QML/attached-properties
# module without a public C++ link target in KF6.

# Install the KNotification event configuration
# KNotification looks for .notifyrc files in the data directory
install(FILES data/kdefancontrol.notifyrc
    DESTINATION ${CMAKE_INSTALL_DATAROOTDIR}/knotifications5
)

install(TARGETS gui_app
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
)