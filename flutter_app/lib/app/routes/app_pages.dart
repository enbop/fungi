import 'package:get/get.dart';
import 'package:fungi_app/ui/pages/home/home_page.dart';

class AppPages {
  static const initial = Routes.home;

  static final routes = [GetPage(name: Routes.home, page: () => HomePage())];
}

abstract class Routes {
  static const home = '/home';
  static const settings = '/settings';
}
