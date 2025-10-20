import 'dart:io';

import 'package:flutter_foreground_task/flutter_foreground_task.dart';
import 'package:fungi_app/app/foreground_task.dart';
import 'package:fungi_app/src/grpc/generated/fungi_daemon.pbgrpc.dart';
import 'package:flutter/material.dart';
import 'package:fungi_app/ui/utils/daemon_client.dart';
import 'package:get/get.dart';
import 'package:get_storage/get_storage.dart';
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

enum DaemonConnectionState { connecting, connected, failed }

extension DaemonConnectionStateExtension on DaemonConnectionState {
  bool get isConnecting => this == DaemonConnectionState.connecting;
  bool get isConnected => this == DaemonConnectionState.connected;
  bool get isFailed => this == DaemonConnectionState.failed;
}

class FileTransferServerState {
  bool enabled;
  String? error;
  String? rootDir;

  FileTransferServerState({required this.enabled, this.error, this.rootDir});
}

class FungiController extends GetxController {
  FungiDaemonClient fungiClient;

  final daemonConnectionState = DaemonConnectionState.connecting.obs;
  final daemonError = ''.obs;

  final peerId = ''.obs;
  final hostname = ''.obs;
  final configFilePath = ''.obs;
  final isForegroundServiceRunning = false.obs;

  final _storage = GetStorage();
  final _themeKey = 'theme_option';
  final _foregroundServiceKey = 'foreground_service_enabled';

  final currentTheme = ThemeOption.system.obs;
  final preventClose = false.obs;
  final incomingAllowedPeers = <PeerInfo>[].obs;
  final addressBook = <PeerInfo>[].obs;
  final fileTransferServerState = FileTransferServerState(enabled: false).obs;
  final fileTransferClients = <FileTransferClient>[].obs;

  // TCP Tunneling state
  final tcpTunnelingConfig = TcpTunnelingConfigResponse().obs;

  final ftpProxy = FtpProxyResponse(enabled: false, host: "", port: 0).obs;
  final webdavProxy = WebdavProxyResponse().obs;

  FungiController()
    : fungiClient = fungiDaemonClientPlaceholder(); // Placeholder client

  @override
  void onInit() {
    super.onInit();
    loadThemeOption();
    if (Platform.isAndroid) {
      // TODO init fungi daemon process manager for Android
      FlutterForegroundTask.addTaskDataCallback(onReceiveTaskData);
      // Check current foreground service status
      checkForegroundServiceStatus();
      // Load and restore previous foreground service state
      loadForegroundServiceState();
    }

    initFungi();
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

  Future<void> updateIncomingAllowedPeers() async {
    incomingAllowedPeers.value = (await fungiClient.getIncomingAllowedPeers(
      Empty(),
    )).peers;
  }

  Future<void> updateAddressBook() async {
    addressBook.value = (await fungiClient.listAddressBookPeers(Empty())).peers;
  }

  Future<void> addIncomingAllowedPeer(PeerInfo peerInfo) async {
    await fungiClient.addIncomingAllowedPeer(
      AddIncomingAllowedPeerRequest()..peerId = peerInfo.peerId,
    );
    await updateIncomingAllowedPeers();
    // Also add to address books

    // Update the address books list to reflect the new peer
    await updateAddressBookPeer(peerInfo);
  }

  Future<void> removeIncomingAllowedPeer(String peerId) async {
    await fungiClient.removeIncomingAllowedPeer(
      RemoveIncomingAllowedPeerRequest()..peerId = peerId,
    );
    await updateIncomingAllowedPeers();
  }

  Future<void> updateAddressBookPeer(PeerInfo peerInfo) async {
    await fungiClient.updateAddressBookPeer(
      UpdateAddressBookPeerRequest()..peerInfo = peerInfo,
    );
    await updateAddressBook();
  }

  Future<PeerInfo?> getAddressBookPeer(String peerId) async {
    return (await fungiClient.getAddressBookPeer(
      GetAddressBookPeerRequest()..peerId = peerId,
    )).peerInfo;
  }

  Future<void> removeAddressBookPeer(String peerId) async {
    await fungiClient.removeAddressBookPeer(
      RemoveAddressBookPeerRequest()..peerId = peerId,
    );
    await updateAddressBook();
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

      await fungiClient.startFileTransferService(
        StartFileTransferServiceRequest()..rootDir = rootDir,
      );
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

  Future<void> stopFileTransferServer() async {
    try {
      await fungiClient.stopFileTransferService(Empty());
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
    await fungiClient.addFileTransferClient(
      AddFileTransferClientRequest()
        ..enabled = enabled
        ..name = peerInfo.alias
        ..peerId = peerInfo.peerId,
    );
    fileTransferClients.add(
      FileTransferClient(
        enabled: enabled,
        peerId: peerInfo.peerId,
        name: peerInfo.alias,
      ),
    );
    // add to address books
    await updateAddressBookPeer(peerInfo);
  }

  Future<void> enableFileTransferClient({
    required FileTransferClient client,
    required bool enabled,
  }) async {
    await fungiClient.enableFileTransferClient(
      EnableFileTransferClientRequest()
        ..peerId = client.peerId
        ..enabled = enabled,
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

  Future<void> removeFileTransferClient(String peerId) async {
    await fungiClient.removeFileTransferClient(
      RemoveFileTransferClientRequest()..peerId = peerId,
    );
    fileTransferClients.removeWhere((client) => client.peerId == peerId);
  }

  Future<void> initFungi() async {
    daemonConnectionState.value = DaemonConnectionState.connecting;
    daemonError.value = '';

    try {
      try {
        fungiClient = await getFungiClient();
      } catch (e) {
        debugPrint('Failed to get Fungi client: $e');
        daemonError.value = e.toString();
        daemonConnectionState.value = DaemonConnectionState.failed;
        return;
      }
      daemonConnectionState.value = DaemonConnectionState.connected;

      peerId.value = (await fungiClient.peerId(Empty())).peerId;
      debugPrint('Peer ID: ${peerId.value}');

      hostname.value = (await fungiClient.hostname(Empty())).hostname;

      configFilePath.value = (await fungiClient.configFilePath(
        Empty(),
      )).configFilePath;

      try {
        fileTransferServerState.value.enabled =
            (await fungiClient.getFileTransferServiceEnabled(Empty())).enabled;
        final dir = (await fungiClient.getFileTransferServiceRootDir(
          Empty(),
        )).rootDir;
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
        final clients = (await fungiClient.getAllFileTransferClients(
          Empty(),
        )).clients;
        fileTransferClients.value = clients.toList();
      } catch (e) {
        debugPrint('Failed to get file transfer clients: $e');
        fileTransferClients.value = [];
      }

      try {
        ftpProxy.value = await fungiClient.getFtpProxy(Empty());
        webdavProxy.value = await fungiClient.getWebdavProxy(Empty());
      } catch (e) {
        debugPrint('Failed to get proxy infos: $e');
      }
      await updateIncomingAllowedPeers();
      await updateAddressBook();
      // Load TCP tunneling config
      await refreshTcpTunnelingConfig();
    } catch (e) {
      daemonConnectionState.value = DaemonConnectionState.failed;
      daemonError.value = e.toString();
      peerId.value = 'error';
      debugPrint('Failed to init, error: $e');
    }
  }

  Future<void> retryConnection() async {
    await initFungi();
  }

  // TCP Tunneling methods
  Future<void> refreshTcpTunnelingConfig() async {
    try {
      final config = await fungiClient.getTcpTunnelingConfig(Empty());
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
      await fungiClient.addTcpForwardingRule(
        AddTcpForwardingRuleRequest()
          ..localHost = localHost
          ..localPort = localPort
          ..peerId = peerInfo.peerId
          ..remotePort = remotePort,
      );
      await refreshTcpTunnelingConfig();

      // add to address books
      await updateAddressBookPeer(peerInfo);
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
      await fungiClient.removeTcpForwardingRule(
        RemoveTcpForwardingRuleRequest()..ruleId = ruleId,
      );
      await refreshTcpTunnelingConfig();
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
      await fungiClient.addTcpListeningRule(
        AddTcpListeningRuleRequest()
          ..localHost = localHost
          ..localPort = localPort
          ..allowedPeers.addAll(allowedPeers),
      );
      await refreshTcpTunnelingConfig();
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
      await fungiClient.removeTcpListeningRule(
        RemoveTcpListeningRuleRequest()..ruleId = ruleId,
      );
      await refreshTcpTunnelingConfig();
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
