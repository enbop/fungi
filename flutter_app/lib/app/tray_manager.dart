import 'dart:io';
import 'package:flutter/material.dart';
import 'package:tray_manager/tray_manager.dart';
import 'package:window_manager/window_manager.dart';
import 'package:get/get.dart';

const String kMenuItemKeyShowWindow = 'show_window';
const String kMenuItemKeyExitApp = 'exit_app';

class AppTrayManager extends GetxService with TrayListener, WindowListener {
  static AppTrayManager get instance => Get.find<AppTrayManager>();

  final RxBool _isInitialized = false.obs;
  bool get isInitialized => _isInitialized.value;

  @override
  void onInit() {
    super.onInit();
    _initTray();
    _initWindowManager();
  }

  @override
  void onClose() {
    trayManager.removeListener(this);
    windowManager.removeListener(this);
    super.onClose();
  }

  Future<void> _initTray() async {
    try {
      trayManager.addListener(this);
      await _setTrayIcon();
      await _setTrayMenu();
      _isInitialized.value = true;
    } catch (e) {
      debugPrint('Failed to initialize tray: $e');
    }
  }

  Future<void> _initWindowManager() async {
    try {
      windowManager.addListener(this);
    } catch (e) {
      debugPrint('Failed to initialize window manager: $e');
    }
  }

  Future<void> _setTrayIcon() async {
    await trayManager.setIcon(
      Platform.isWindows
          ? "assets/images/app_icon.ico"
          : "assets/images/tray_icon.png",
      isTemplate: Platform.isMacOS,
    );
  }

  Future<void> _setTrayMenu() async {
    final menu = Menu(
      items: [
        MenuItem(key: kMenuItemKeyShowWindow, label: 'Show Main Window'),
        MenuItem.separator(),
        MenuItem(key: kMenuItemKeyExitApp, label: 'Exit'),
      ],
    );

    await trayManager.setContextMenu(menu);
  }

  Future<void> showWindow() async {
    try {
      await windowManager.show();
    } catch (e) {
      debugPrint('Failed to show window: $e');
    }
  }

  Future<void> hideWindow() async {
    try {
      await windowManager.hide();
    } catch (e) {
      debugPrint('Failed to hide window: $e');
    }
  }

  Future<void> exitApp() async {
    try {
      await trayManager.destroy();
      exit(0);
    } catch (e) {
      debugPrint('Failed to exit app: $e');
      exit(1);
    }
  }

  @override
  void onTrayIconMouseDown() {
    showWindow();
  }

  @override
  void onTrayIconRightMouseDown() {
    trayManager.popUpContextMenu();
  }

  @override
  void onTrayMenuItemClick(MenuItem menuItem) {
    switch (menuItem.key) {
      case kMenuItemKeyShowWindow:
        showWindow();
        break;
      case kMenuItemKeyExitApp:
        exitApp();
        break;
    }
  }

  @override
  void onWindowClose() {
    hideWindow();
  }

  @override
  void onWindowMinimize() {}
}
