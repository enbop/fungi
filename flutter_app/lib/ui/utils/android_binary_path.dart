import 'dart:io';

import 'package:logging/logging.dart';

final _logger = Logger('AndroidBinaryPath');

Future<String?> getAndroidFungiBinaryPath() async {
  if (!Platform.isAndroid) return null;

  try {
    final mapsFile = File('/proc/self/maps');
    if (!await mapsFile.exists()) return null;

    final maps = await mapsFile.readAsLines();

    for (final line in maps) {
      if (line.contains('libflutter.so')) {
        final parts = line.split(RegExp(r'\s+'));
        if (parts.length >= 6) {
          final flutterPath = parts.sublist(5).join(' ');
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
    _logger.warning('Error finding fungi binary: $e');
  }

  return null;
}
