import 'package:flutter/material.dart';
import 'package:get/get.dart';
import 'package:get_storage/get_storage.dart';
import 'package:fungi_app/src/rust/api/fungi.dart' as fungi;

enum ThemeOption { light, dark, system }

extension ThemeOptionExtension on ThemeOption {
  String get name {
    switch (this) {
      case ThemeOption.light:
        return 'Light';
      case ThemeOption.dark:
        return 'Dark';
      case ThemeOption.system:
        return 'Follow System';
    }
  }

  ThemeMode get themeMode {
    switch (this) {
      case ThemeOption.light:
        return ThemeMode.light;
      case ThemeOption.dark:
        return ThemeMode.dark;
      case ThemeOption.system:
        return ThemeMode.system;
    }
  }
}

class FungiController extends GetxController {
  final isServiceRunning = false.obs;
  final peerId = ''.obs;
  final configFilePath = ''.obs;

  final _storage = GetStorage();
  final _themeKey = 'theme_option';

  final Rx<ThemeOption> currentTheme = ThemeOption.system.obs;

  final incomingAllowdPeers = <String>[].obs;

  @override
  void onInit() {
    super.onInit();
    initFungi();
    loadThemeOption();
  }

  void loadThemeOption() {
    final savedTheme = _storage.read(_themeKey);
    if (savedTheme != null) {
      currentTheme.value = ThemeOption.values[savedTheme];
    }
  }

  void changeTheme(ThemeOption option) {
    currentTheme.value = option;
    Get.changeThemeMode(option.themeMode);
    _storage.write(_themeKey, option.index);
  }

  void updateIncomingAllowedPeers() {
    incomingAllowdPeers.value = fungi.getIncomingAllowedPeersList();
  }

  void addIncomingAllowedPeer(String peerId) {
    fungi.addIncomingAllowedPeer(peerId: peerId);
    updateIncomingAllowedPeers();
  }

  void removeIncomingAllowedPeer(String peerId) {
    fungi.removeIncomingAllowedPeer(peerId: peerId);
    updateIncomingAllowedPeers();
  }

  Future<void> initFungi() async {
    try {
      await fungi.startFungiDaemon();
      isServiceRunning.value = true;
      debugPrint('Fungi Daemon started');

      String id = fungi.peerId();
      peerId.value = id;
      debugPrint('Peer ID: $id');

      configFilePath.value = fungi.configFilePath();

      updateIncomingAllowedPeers();
    } catch (e) {
      isServiceRunning.value = false;
      peerId.value = 'error';
      debugPrint('Failed to init, error: $e');
    }
  }
}
