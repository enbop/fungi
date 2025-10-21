import 'dart:io';

import 'package:flutter_foreground_task/flutter_foreground_task.dart';

Future<void> initMobile() async {
  if (Platform.isAndroid) {
    FlutterForegroundTask.initCommunicationPort();
  }
}
