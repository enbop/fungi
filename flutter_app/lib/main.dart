import 'package:flutter/material.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:fungi_app/app/routes/app_pages.dart';
import 'package:fungi_app/app/tray_manager.dart';
import 'package:fungi_app/ui/pages/theme/app_theme.dart';
import 'package:fungi_app/ui/utils/macos_scoped_resource.dart';
import 'package:fungi_app/ui/utils/mobile_info.dart';
import 'package:fungi_app/ui/widgets/daemon_connection_overlay.dart';
import 'package:get/get.dart';
import 'package:get_storage/get_storage.dart';
import 'package:logging/logging.dart';
import 'package:window_manager/window_manager.dart';
import 'package:flutter_smart_dialog/flutter_smart_dialog.dart';
import 'dart:io';

void main() async {
  Logger.root.level = Level.FINE; // defaults to Level.INFO
  Logger.root.onRecord.listen((record) {
    debugPrint('${record.level.name}: ${record.time}: ${record.message}');
  });

  WidgetsFlutterBinding.ensureInitialized();

  await GetStorage.init();

  await initMobile();

  if (Platform.isMacOS) {
    lastFileAccessingSecurityScopedResource();
  }

  if (Platform.isWindows || Platform.isLinux || Platform.isMacOS) {
    await windowManager.ensureInitialized();

    Get.put(AppTrayManager());

    WindowOptions windowOptions = WindowOptions(
      size: Size(600, 720),
      center: true,
      skipTaskbar: false,
    );
    windowManager.waitUntilReadyToShow(windowOptions, () async {
      await windowManager.show();
      await windowManager.setPreventClose(true);
    });
  }

  Get.put(FungiController());
  runApp(const FungiApp());
}

class FungiApp extends GetView<FungiController> {
  const FungiApp({super.key});

  @override
  Widget build(BuildContext context) {
    return Obx(
      () => GetMaterialApp(
        title: 'Fungi App',
        theme: AppTheme.lightTheme,
        darkTheme: AppTheme.darkTheme,
        themeMode: controller.currentTheme.value.themeMode,
        initialRoute: AppPages.initial,
        getPages: AppPages.routes,
        navigatorObservers: [FlutterSmartDialog.observer],
        builder: (context, child) {
          return FlutterSmartDialog.init()(
            context,
            DaemonConnectionOverlay(child: child ?? const SizedBox()),
          );
        },
      ),
    );
  }
}
