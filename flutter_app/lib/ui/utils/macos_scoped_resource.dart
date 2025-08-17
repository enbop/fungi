import 'dart:io';

import 'package:get_storage/get_storage.dart';
import 'package:macos_secure_bookmarks/macos_secure_bookmarks.dart';

const String kLastFileTransferServerRootDirKey =
    'lastFileTransferServerRootDir';

Future<void> saveLastFileTransferServerRootDirBookmark(String dir) async {
  final secureBookmarks = SecureBookmarks();
  final bookmark = await secureBookmarks.bookmark(File(dir));
  GetStorage().write(kLastFileTransferServerRootDirKey, bookmark);
}

Future<void> lastFileAccessingSecurityScopedResource() async {
  String? lastDir = GetStorage().read(kLastFileTransferServerRootDirKey);
  if (lastDir != null) {
    final secureBookmarks = SecureBookmarks();
    final resolvedFile = await secureBookmarks.resolveBookmark(lastDir);
    await secureBookmarks.startAccessingSecurityScopedResource(resolvedFile);
  }
}
