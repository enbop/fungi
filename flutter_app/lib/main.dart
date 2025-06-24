import 'package:flutter/material.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:fungi_app/app/routes/app_pages.dart';
import 'package:fungi_app/src/rust/frb_generated.dart';
import 'package:fungi_app/ui/pages/theme/app_theme.dart';
import 'package:get/get.dart';

void main() async {
  await RustLib.init();
  Get.put(ThemeManager());
  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return GetMaterialApp(
      title: 'Fungi App',
      theme: AppTheme.lightTheme,
      darkTheme: AppTheme.darkTheme,
      themeMode: ThemeMode.system,
      initialBinding: HomeBinding(),
      initialRoute: AppPages.initial,
      getPages: AppPages.routes,
    );
  }
}
