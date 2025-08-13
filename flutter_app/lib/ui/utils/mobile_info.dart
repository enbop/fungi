import 'dart:io';

import 'package:device_info_plus/device_info_plus.dart';
import 'package:fungi_app/src/rust/api/fungi.dart' as fungi;

Future<void> initMobile() async {
  if (Platform.isAndroid) {
    DeviceInfoPlugin deviceInfo = DeviceInfoPlugin();
    AndroidDeviceInfo androidInfo = await deviceInfo.androidInfo;
    fungi.initMobileDeviceName(name: androidInfo.name);
  }
}
