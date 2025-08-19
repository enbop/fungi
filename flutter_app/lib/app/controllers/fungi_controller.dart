import 'dart:io';

import 'package:flutter_foreground_task/flutter_foreground_task.dart';
import 'package:fungi_app/app/foreground_task.dart';
import 'package:path/path.dart' as p;
import 'package:flutter/material.dart';
import 'package:fungi_app/src/rust/api/fungi.dart';
import 'package:get/get.dart';
import 'package:get_storage/get_storage.dart';
import 'package:fungi_app/src/rust/api/fungi.dart' as fungi;
import 'package:path_provider/path_provider.dart';
import 'package:permission_handler/permission_handler.dart';

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
  final isForegroundServiceRunning = false.obs;

  final _storage = GetStorage();
  final _themeKey = 'theme_option';
  final _foregroundServiceKey = 'foreground_service_enabled';

  final currentTheme = ThemeOption.system.obs;
  final preventClose = false.obs;
  final incomingAllowdPeers = <String>[].obs;
  final incomingAllowedPeersWithInfo = <PeerWithInfo>[].obs;
  final addressBook = <PeerInfo>[].obs;
  final fileTransferServerState = FileTransferServerState(enabled: false).obs;
  final fileTransferClients = <FileTransferClient>[].obs;

  // TCP Tunneling state
  final tcpTunnelingConfig = TcpTunnelingConfig(
    forwardingEnabled: false,
    listeningEnabled: false,
    forwardingRules: [],
    listeningRules: [],
  ).obs;

  final ftpProxy = FtpProxy(enabled: false, host: "", port: 0).obs;
  final webdavProxy = WebdavProxy(enabled: false, host: "", port: 0).obs;

  @override
  void onInit() {
    super.onInit();
    initFungi();
    loadThemeOption();

    if (Platform.isAndroid) {
      FlutterForegroundTask.addTaskDataCallback(onReceiveTaskData);
      // Check current foreground service status
      checkForegroundServiceStatus();
      // Load and restore previous foreground service state
      loadForegroundServiceState();
    }
  }

  @override
  void dispose() {
    // Remove a callback to receive data sent from the TaskHandler.
    FlutterForegroundTask.removeTaskDataCallback(onReceiveTaskData);
    super.dispose();
  }

  // Foreground service control methods
  Future<void> toggleForegroundService() async {
    if (isForegroundServiceRunning.value) {
      await stopForegroundServiceController();
    } else {
      await startForegroundServiceController();
    }
  }

  Future<void> startForegroundServiceController() async {
    try {
      await requestForegroundPermissions();
      initForegroundService();
      await startForegroundService();
      isForegroundServiceRunning.value = true;
      saveForegroundServiceState(true);
      debugPrint('Foreground service started successfully');
    } catch (e) {
      debugPrint('Failed to start foreground service: $e');
    }
  }

  Future<void> stopForegroundServiceController() async {
    try {
      await stopForegroundService();
      isForegroundServiceRunning.value = false;
      saveForegroundServiceState(false);
      debugPrint('Foreground service stopped successfully');
    } catch (e) {
      debugPrint('Failed to stop foreground service: $e');
    }
  }

  Future<void> checkForegroundServiceStatus() async {
    if (Platform.isAndroid) {
      final isRunning = await FlutterForegroundTask.isRunningService;
      isForegroundServiceRunning.value = isRunning;
    }
  }

  void saveForegroundServiceState(bool enabled) {
    _storage.write(_foregroundServiceKey, enabled);
  }

  void loadForegroundServiceState() {
    final savedState = _storage.read(_foregroundServiceKey);
    if (savedState != null && savedState is bool) {
      // Only restore if the service was enabled before
      if (savedState) {
        (() async {
          await Future.delayed(const Duration(seconds: 1));
          await startForegroundServiceController();
        })();
      }
    }
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
    incomingAllowedPeersWithInfo.value = fungi
        .getIncomingAllowedPeersWithInfo();
  }

  void updateAddressBook() {
    addressBook.value = fungi.getAllAddressBook();
  }

  void addIncomingAllowedPeer(PeerInfo peerInfo) {
    fungi.addIncomingAllowedPeer(peerId: peerInfo.peerId);
    updateIncomingAllowedPeers();
    // Also add to address books

    // Update the address books list to reflect the new peer
    addressBookAddOrUpdate(peerInfo);
  }

  void removeIncomingAllowedPeer(String peerId) {
    fungi.removeIncomingAllowedPeer(peerId: peerId);
    updateIncomingAllowedPeers();
  }

  void addressBookAddOrUpdate(PeerInfo peerInfo) {
    fungi.addressBookAddOrUpdate(peerInfo: peerInfo);
    updateAddressBook();
  }

  PeerInfo? addressBookGetPeer(String peerId) {
    return fungi.addressBookGetPeer(peerId: peerId);
  }

  void addressBookRemove(String peerId) {
    fungi.addressBookRemove(peerId: peerId);
    updateAddressBook();
  }

  Future<void> startFileTransferServer(String rootDir) async {
    try {
      if (Platform.isAndroid) {
        final status = await Permission.manageExternalStorage.request();
        if (!status.isGranted) {
          Get.snackbar(
            'Permission required',
            'Please try again and grant "Manage External Storage" permission to use File Transfer Server.',
            snackPosition: SnackPosition.BOTTOM,
            backgroundColor: Colors.red.withValues(alpha: 0.1),
            colorText: Colors.red,
          );
          return;
        }
      }

      await fungi.startFileTransferService(rootDir: rootDir);
      fileTransferServerState.value.enabled = true;
      fileTransferServerState.value.error = null;
      debugPrint('File Transfer Server started');
    } catch (e) {
      fileTransferServerState.value.enabled = false;
      fileTransferServerState.value.error = e.toString();
      debugPrint('Failed to start File Transfer Server: $e');
    }
    fileTransferServerState.value.rootDir = rootDir;
    fileTransferServerState.refresh();
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
    fileTransferServerState.refresh();
  }

  Future<void> addFileTransferClient({
    required bool enabled,
    required PeerInfo peerInfo,
  }) async {
    await fungi.addFileTransferClient(
      enabled: enabled,
      name: peerInfo.alias,
      peerId: peerInfo.peerId,
    );
    fileTransferClients.add(
      FileTransferClient(
        enabled: enabled,
        peerId: peerInfo.peerId,
        name: peerInfo.alias,
      ),
    );
    // add to address books
    addressBookAddOrUpdate(peerInfo);
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
      if (Platform.isAndroid) {
        final appDocumentsDir = await getApplicationDocumentsDirectory();
        final fungiDir = p.join(appDocumentsDir.absolute.path, 'fungi');
        await fungi.startFungiDaemon(fungiDir: fungiDir);
      }
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
        final dir = fungi.getFileTransferServiceRootDir();
        if (dir.isNotEmpty) {
          fileTransferServerState.value.rootDir = dir;
        } else {
          fileTransferServerState.value.rootDir = null;
        }
      } catch (e) {
        debugPrint('Failed to get file transfer server state: $e');
        fileTransferServerState.value.error = e.toString();
      }
      fileTransferServerState.refresh();

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
      updateAddressBook();
      // Load TCP tunneling config
      refreshTcpTunnelingConfig();
    } catch (e) {
      isServiceRunning.value = false;
      peerId.value = 'error';
      debugPrint('Failed to init, error: $e');
    }
  }

  // TCP Tunneling methods
  void refreshTcpTunnelingConfig() {
    try {
      final config = fungi.getTcpTunnelingConfig();
      tcpTunnelingConfig.value = config;
    } catch (e) {
      debugPrint('Failed to get TCP tunneling config: $e');
    }
  }

  Future<void> addTcpForwardingRule({
    required String localHost,
    required int localPort,
    required int remotePort,
    required PeerInfo peerInfo,
  }) async {
    try {
      await fungi.addTcpForwardingRule(
        localHost: localHost,
        localPort: localPort,
        peerId: peerInfo.peerId,
        remotePort: remotePort,
      );
      refreshTcpTunnelingConfig();

      // add to address books
      addressBookAddOrUpdate(peerInfo);
      Get.snackbar(
        'Success',
        'Forwarding rule added successfully',
        snackPosition: SnackPosition.BOTTOM,
        backgroundColor: Colors.green.withValues(alpha: 0.1),
        colorText: Colors.green,
      );
    } catch (e) {
      Get.snackbar(
        'Error',
        'Failed to add forwarding rule: $e',
        snackPosition: SnackPosition.BOTTOM,
        backgroundColor: Colors.red.withValues(alpha: 0.1),
        colorText: Colors.red,
      );
    }
  }

  Future<void> removeTcpForwardingRule(String ruleId) async {
    try {
      fungi.removeTcpForwardingRule(ruleId: ruleId);
      refreshTcpTunnelingConfig();
      Get.snackbar(
        'Success',
        'Forwarding rule removed successfully',
        snackPosition: SnackPosition.BOTTOM,
        backgroundColor: Colors.green.withValues(alpha: 0.1),
        colorText: Colors.green,
      );
    } catch (e) {
      Get.snackbar(
        'Error',
        'Failed to remove forwarding rule: $e',
        snackPosition: SnackPosition.BOTTOM,
        backgroundColor: Colors.red.withValues(alpha: 0.1),
        colorText: Colors.red,
      );
    }
  }

  Future<void> addTcpListeningRule({
    required String localHost,
    required int localPort,
    required List<String> allowedPeers,
  }) async {
    try {
      await fungi.addTcpListeningRule(
        localHost: localHost,
        localPort: localPort,
        allowedPeers: allowedPeers,
      );
      refreshTcpTunnelingConfig();
      Get.snackbar(
        'Success',
        'Listening rule added successfully',
        snackPosition: SnackPosition.BOTTOM,
        backgroundColor: Colors.green.withValues(alpha: 0.1),
        colorText: Colors.green,
      );
    } catch (e) {
      Get.snackbar(
        'Error',
        'Failed to add listening rule: $e',
        snackPosition: SnackPosition.BOTTOM,
        backgroundColor: Colors.red.withValues(alpha: 0.1),
        colorText: Colors.red,
      );
    }
  }

  Future<void> removeTcpListeningRule(String ruleId) async {
    try {
      fungi.removeTcpListeningRule(ruleId: ruleId);
      refreshTcpTunnelingConfig();
      Get.snackbar(
        'Success',
        'Listening rule removed successfully',
        snackPosition: SnackPosition.BOTTOM,
        backgroundColor: Colors.green.withValues(alpha: 0.1),
        colorText: Colors.green,
      );
    } catch (e) {
      Get.snackbar(
        'Error',
        'Failed to remove listening rule: $e',
        snackPosition: SnackPosition.BOTTOM,
        backgroundColor: Colors.red.withValues(alpha: 0.1),
        colorText: Colors.red,
      );
    }
  }
}
