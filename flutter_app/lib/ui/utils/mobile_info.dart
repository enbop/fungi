import 'dart:io';

import 'package:device_info_plus/device_info_plus.dart';
import 'package:flutter_foreground_task/flutter_foreground_task.dart';

Future<void> initMobile() async {
  if (Platform.isAndroid) {
    DeviceInfoPlugin deviceInfo = DeviceInfoPlugin();
    AndroidDeviceInfo androidInfo = await deviceInfo.androidInfo;
    // TODO
    // fungi.initMobileDeviceName(name: androidInfo.name);

    FlutterForegroundTask.initCommunicationPort();
  }
}
