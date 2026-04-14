cmake_minimum_required(VERSION 3.20)
project(kde-fan-control-gui VERSION 1.0.0 LANGUAGES CXX)

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

# KF6 StatusNotifierItem for system tray
find_package(KF6StatusNotifierItem REQUIRED)

# KF6 Notifications for desktop notifications
find_package(KF6Notifications REQUIRED)

# KF6 i18n for exposing i18n() into QML
find_package(KF6I18n REQUIRED)

qt_add_executable(gui_app src/main.cpp)

set_target_properties(gui_app PROPERTIES OUTPUT_NAME kde-fan-control-gui)

qt_add_qml_module(gui_app
    URI org.kde.fancontrol
    VERSION 1.0
    QML_FILES
        qml/Main.qml
        qml/OverviewPage.qml
        qml/InventoryPage.qml
        qml/FanDetailPage.qml
        qml/WizardDialog.qml
        qml/TrayPopover.qml
        qml/delegates/FanRowDelegate.qml
        qml/delegates/FanTrayDelegate.qml
        qml/components/StateBadge.qml
        qml/components/OutputBar.qml
        qml/components/TemperatureDisplay.qml
        qml/components/PidField.qml
        qml/components/RenameDialog.qml
        qml/components/MultiSelectComboBox.qml
    NO_CACHEGEN
    SOURCES
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
        src/models/overview_fan_row.h
        src/models/overview_model.h
        src/models/overview_model.cpp
        src/tray_icon.h
        src/tray_icon.cpp
        src/notification_handler.h
        src/notification_handler.cpp
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
    KF6::I18nQml
    KF6::StatusNotifierItem
    KF6::Notifications
)

# KF6 IconThemes (provides KIconColors, QIcon from theme) — link if found
if(KF6IconThemes_FOUND)
    target_link_libraries(gui_app PRIVATE KF6::IconThemes)
endif()

# Kirigami QML module — note: at runtime the QML engine resolves the
# org.kde.kirigami import via the QML plugin path. We don't need a
# compile-time link because Kirigami is a pure QML/attached-properties
# module without a public C++ link target in KF6.

# Install the KNotification event configuration
# KNotification looks for .notifyrc files in the data directory
install(FILES data/kdefancontrol.notifyrc
    DESTINATION ${CMAKE_INSTALL_DATAROOTDIR}/knotifications6
)

# Install the desktop entry file
install(FILES ${CMAKE_CURRENT_SOURCE_DIR}/../packaging/org.kde.fancontrol.desktop
    DESTINATION ${CMAKE_INSTALL_DATAROOTDIR}/applications
)

# Install the application icons to the hicolor icon theme
install(FILES ${CMAKE_CURRENT_SOURCE_DIR}/../packaging/icons/hicolor/scalable/apps/kde-fan-control.svg
    DESTINATION ${CMAKE_INSTALL_DATAROOTDIR}/icons/hicolor/scalable/apps
)
install(FILES ${CMAKE_CURRENT_SOURCE_DIR}/../packaging/icons/hicolor/48x48/apps/kde-fan-control.png
    DESTINATION ${CMAKE_INSTALL_DATAROOTDIR}/icons/hicolor/48x48/apps
)
install(FILES ${CMAKE_CURRENT_SOURCE_DIR}/../packaging/icons/hicolor/128x128/apps/kde-fan-control.png
    DESTINATION ${CMAKE_INSTALL_DATAROOTDIR}/icons/hicolor/128x128/apps
)

# Install the AppStream metadata
install(FILES ${CMAKE_CURRENT_SOURCE_DIR}/../packaging/org.kde.fancontrol.metainfo.xml
    DESTINATION ${CMAKE_INSTALL_DATAROOTDIR}/metainfo
)

# Install the QML module descriptor so the engine can resolve types
# when the binary is launched from outside the build tree.
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/org/kde/fancontrol/qmldir
              ${CMAKE_CURRENT_BINARY_DIR}/org/kde/fancontrol/gui_app.qmltypes
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/qt6/qml/org/kde/fancontrol
)

install(TARGETS gui_app
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
)
