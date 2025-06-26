import 'package:flutter/material.dart';
import 'package:fungi_app/src/rust/api/fungi.dart';
import 'package:get/get.dart';
import 'package:get/get_connect/http/src/utils/utils.dart';
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

class FileTransferServerState {
  bool enabled;
  String? error;
  String? rootDir;

  FileTransferServerState({required this.enabled, this.error, this.rootDir});
}

class FungiController extends GetxController {
  final isServiceRunning = false.obs;
  final peerId = ''.obs;
  final configFilePath = ''.obs;

  final _storage = GetStorage();
  final _themeKey = 'theme_option';

  final currentTheme = ThemeOption.system.obs;
  final incomingAllowdPeers = <String>[].obs;
  final fileTransferServerState = FileTransferServerState(enabled: false).obs;
  final fileTransferClients = <FileTransferClient>[].obs;

  final ftpProxy = FtpProxy(enabled: false, host: "", port: 0).obs;
  final webdavProxy = WebdavProxy(enabled: false, host: "", port: 0).obs;

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

  void startFileTransferServer(String rootDir) {
    try {
      fungi.startFileTransferService(rootDir: rootDir);
      fileTransferServerState.value.enabled = true;
      fileTransferServerState.value.error = null;
      debugPrint('File Transfer Server started');
    } catch (e) {
      fileTransferServerState.value.enabled = false;
      fileTransferServerState.value.error = e.toString();
      debugPrint('Failed to start File Transfer Server: $e');
    }
  }

  void stopFileTransferServer() {
    try {
      fungi.stopFileTransferService();
      fileTransferServerState.value.enabled = false;
      fileTransferServerState.value.error = null;
      debugPrint('File Transfer Server stopped');
    } catch (e) {
      fileTransferServerState.value.error = e.toString();
      debugPrint('Failed to stop File Transfer Server: $e');
    }
  }

  Future<void> addFileTransferClient({
    required bool enabled,
    String? name,
    required String peerId,
  }) async {
    await fungi.addFileTransferClient(
      enabled: enabled,
      name: name,
      peerId: peerId,
    );
    fileTransferClients.add(
      FileTransferClient(enabled: enabled, peerId: peerId, name: name),
    );
  }

  Future<void> enableFileTransferClient({
    required FileTransferClient client,
    required bool enabled,
  }) async {
    await fungi.enableFileTransferClient(
      peerId: client.peerId,
      enabled: enabled,
    );

    final newClient = FileTransferClient(
      enabled: enabled,
      peerId: client.peerId,
      name: client.name,
    );

    final index = fileTransferClients.indexWhere(
      (c) => c.peerId == client.peerId,
    );
    if (index != -1) {
      fileTransferClients[index] = newClient;
    }
    debugPrint('File Transfer Client ${client.peerId} enabled: $enabled');
    fileTransferClients.refresh();
  }

  void removeFileTransferClient(String peerId) {
    fungi.removeFileTransferClient(peerId: peerId);
    fileTransferClients.removeWhere((client) => client.peerId == peerId);
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

      try {
        fileTransferServerState.value.enabled = fungi
            .getFileTransferServiceEnabled();
        fileTransferServerState.value.rootDir = fungi
            .getFileTransferServiceRootDir();
      } catch (e) {
        debugPrint('Failed to get file transfer server state: $e');
        fileTransferServerState.value.error = e.toString();
      }

      try {
        final clients = fungi.getAllFileTransferClients();
        fileTransferClients.value = clients.toList();
      } catch (e) {
        debugPrint('Failed to get file transfer clients: $e');
        fileTransferClients.value = [];
      }

      try {
        ftpProxy.value = fungi.getFtpProxy();
        webdavProxy.value = fungi.getWebdavProxy();
      } catch (e) {
        debugPrint('Failed to get proxy infos: $e');
      }

      updateIncomingAllowedPeers();
    } catch (e) {
      isServiceRunning.value = false;
      peerId.value = 'error';
      debugPrint('Failed to init, error: $e');
    }
  }
}
