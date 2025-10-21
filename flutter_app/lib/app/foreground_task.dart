import 'dart:io';

import 'package:device_info_plus/device_info_plus.dart';
import 'package:flutter/material.dart';
import 'package:flutter_foreground_task/flutter_foreground_task.dart';
import 'package:fungi_app/ui/utils/android_binary_path.dart';
import 'package:path_provider/path_provider.dart';
import 'package:path/path.dart' as p;

@pragma('vm:entry-point')
void startCallback() {
  FlutterForegroundTask.setTaskHandler(MyTaskHandler());
}

void onReceiveTaskData(Object data) {
  if (data is Map<String, dynamic>) {
    final dynamic timestampMillis = data["timestampMillis"];
    if (timestampMillis != null) {
      final DateTime timestamp = DateTime.fromMillisecondsSinceEpoch(
        timestampMillis,
        isUtc: true,
      );
      debugPrint('timestamp: ${timestamp.toString()}');
    }
  }
}

class MyTaskHandler extends TaskHandler {
  Process? _daemonProcess;

  @override
  Future<void> onStart(DateTime timestamp, TaskStarter starter) async {
    debugPrint('onStart(starter: ${starter.name})');

    if (Platform.isAndroid) {
      await _startDaemonProcess();
    }
  }

  Future<void> _startDaemonProcess() async {
    try {
      final fungiBinaryPath = await getAndroidFungiBinaryPath();
      if (fungiBinaryPath == null) {
        debugPrint('Failed to find fungi binary');
        return;
      }

      final deviceInfo = DeviceInfoPlugin();
      final androidInfo = await deviceInfo.androidInfo;
      final deviceName = androidInfo.model;

      final appDocumentsDir = await getApplicationDocumentsDirectory();
      final fungiDir = p.join(appDocumentsDir.absolute.path, 'fungi');

      await Directory(fungiDir).create(recursive: true);

      _daemonProcess = await Process.start(fungiBinaryPath, [
        '--default-device-name',
        deviceName,
        'daemon',
        '--fungi-dir',
        fungiDir,
        '--exit-on-stdin-close',
      ], mode: ProcessStartMode.normal);

      _daemonProcess!.stdout
          .transform(SystemEncoding().decoder)
          .listen((data) => debugPrint('[Daemon] $data'));

      _daemonProcess!.stderr
          .transform(SystemEncoding().decoder)
          .listen((data) => debugPrint('[Daemon Error] $data'));

      _daemonProcess!.exitCode.then((exitCode) {
        debugPrint('Daemon exited with code: $exitCode');
        _daemonProcess = null;
      });

      debugPrint('Daemon started successfully');
    } catch (e) {
      debugPrint('Failed to start daemon: $e');
    }
  }

  @override
  @override
  void onRepeatEvent(DateTime timestamp) {
    // Send data to main isolate.
    final Map<String, dynamic> data = {
      "timestampMillis": timestamp.millisecondsSinceEpoch,
    };
    FlutterForegroundTask.sendDataToMain(data);
  }

  @override
  Future<void> onDestroy(DateTime timestamp, bool isTimeout) async {
    debugPrint('onDestroy(isTimeout: $isTimeout)');

    if (_daemonProcess != null) {
      _daemonProcess!.kill(ProcessSignal.sigterm);
      _daemonProcess = null;
    }
  }

  // Called when data is sent using `FlutterForegroundTask.sendDataToTask`.
  @override
  void onReceiveData(Object data) {
    debugPrint('onReceiveData: $data');
  }

  // Called when the notification button is pressed.
  @override
  void onNotificationButtonPressed(String id) {
    debugPrint('onNotificationButtonPressed: $id');
  }

  // Called when the notification itself is pressed.
  @override
  void onNotificationPressed() {
    debugPrint('onNotificationPressed');
  }

  // Called when the notification itself is dismissed.
  @override
  void onNotificationDismissed() {
    debugPrint('onNotificationDismissed');
  }
}

Future<void> requestForegroundPermissions() async {
  // Android 13+, you need to allow notification permission to display foreground service notification.
  //
  // iOS: If you need notification, ask for permission.
  final NotificationPermission notificationPermission =
      await FlutterForegroundTask.checkNotificationPermission();
  if (notificationPermission != NotificationPermission.granted) {
    await FlutterForegroundTask.requestNotificationPermission();
  }

  // Android 12+, there are restrictions on starting a foreground service.
  //
  // To restart the service on device reboot or unexpected problem, you need to allow below permission.
  if (!await FlutterForegroundTask.isIgnoringBatteryOptimizations) {
    // This function requires `android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` permission.
    await FlutterForegroundTask.requestIgnoreBatteryOptimization();
  }
}

void initForegroundService() {
  FlutterForegroundTask.init(
    androidNotificationOptions: AndroidNotificationOptions(
      channelId: 'foreground_service',
      channelName: 'Foreground Service Notification',
      channelDescription:
          'This notification appears when the foreground service is running.',
      onlyAlertOnce: true,
    ),
    iosNotificationOptions: const IOSNotificationOptions(
      showNotification: false,
      playSound: false,
    ),
    foregroundTaskOptions: ForegroundTaskOptions(
      eventAction: ForegroundTaskEventAction.repeat(30000),
      autoRunOnBoot: false,
      autoRunOnMyPackageReplaced: false,
      allowWakeLock: false,
      allowWifiLock: false,
    ),
  );
}

Future<ServiceRequestResult> startForegroundService() async {
  if (await FlutterForegroundTask.isRunningService) {
    return FlutterForegroundTask.restartService();
  } else {
    return FlutterForegroundTask.startService(
      // You can manually specify the foregroundServiceType for the service
      // to be started, as shown in the comment below.
      serviceTypes: [ForegroundServiceTypes.dataSync],
      serviceId: 256,
      notificationTitle: 'Foreground Service is running',
      notificationText: 'Tap to return to the app',
      notificationIcon: NotificationIcon(
        metaDataName: 'logo_transparent_white',
      ),
      notificationInitialRoute: '/',
      callback: startCallback,
    );
  }
}

Future<ServiceRequestResult> stopForegroundService() async {
  return FlutterForegroundTask.stopService();
}
