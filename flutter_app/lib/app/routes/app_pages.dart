import 'package:flutter_app/app/controllers/fungi_controller.dart';
import 'package:get/get.dart';
import 'package:flutter_app/ui/pages/home/home_page.dart';

class AppPages {
  static const initial = Routes.home;

  static final routes = [
    GetPage(name: Routes.home, page: () => HomePage(), binding: HomeBinding()),
  ];
}

abstract class Routes {
  static const home = '/home';
  static const settings = '/settings';
}
