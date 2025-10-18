import 'dart:io';

import 'package:device_info_plus/device_info_plus.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_foreground_task/flutter_foreground_task.dart';

/// Get the path to the fungi binary executable.
///
/// Reads /proc/self/maps to locate the loaded libflutter.so,
/// then searches for libfungi.so in the same directory.
///
/// Returns the full file path, or null if not found.
Future<String?> getFungiBinaryPath() async {
  if (!Platform.isAndroid) return null;

  try {
    final mapsFile = File('/proc/self/maps');
    if (!await mapsFile.exists()) return null;

    final maps = await mapsFile.readAsLines();

    // Find the path to libflutter.so
    for (final line in maps) {
      if (line.contains('libflutter.so')) {
        final parts = line.split(RegExp(r'\s+'));
        if (parts.length >= 6) {
          final flutterPath = parts.sublist(5).join(' ');

          // Get the directory path and construct libfungi.so path
          final libDir = File(flutterPath).parent.path;
          final fungiPath = '$libDir/libfungi.so';

          if (await File(fungiPath).exists()) {
            return fungiPath;
          }

          break;
        }
      }
    }
  } catch (e) {
    debugPrint('⚠️ Error finding fungi binary: $e');
  }

  return null;
}

Future<void> initMobile() async {
  if (Platform.isAndroid) {
    FlutterForegroundTask.initCommunicationPort();

    DeviceInfoPlugin deviceInfo = DeviceInfoPlugin();
    await deviceInfo.androidInfo;

    // Get the fungi binary file path
    final fungiBinaryPath = await getFungiBinaryPath();

    if (fungiBinaryPath != null) {
      debugPrint('🚀 Fungi binary ready at: $fungiBinaryPath');
      // TODO: Start fungi daemon and initialize with device name
      // fungi.initMobileDeviceName(name: androidInfo.name);
    } else {
      debugPrint('⚠️ Warning: Fungi binary not found');
    }
  }
}
