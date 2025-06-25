import 'package:flutter/material.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:fungi_app/app/routes/app_pages.dart';
import 'package:fungi_app/src/rust/frb_generated.dart';
import 'package:fungi_app/ui/pages/theme/app_theme.dart';
import 'package:get/get.dart';
import 'package:get_storage/get_storage.dart';

void main() async {
  await RustLib.init();
  await GetStorage.init();
  Get.put(FungiController());
  runApp(const MyApp());
}

class MyApp extends GetView<FungiController> {
  const MyApp({super.key});

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
      ),
    );
  }
}
