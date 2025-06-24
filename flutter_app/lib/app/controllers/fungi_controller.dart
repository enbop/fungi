import 'package:flutter/material.dart';
import 'package:get/get.dart';
import 'package:fungi_app/src/rust/api/fungi.dart' as fungi;

class HomeBinding implements Bindings {
  @override
  void dependencies() {
    Get.put(FungiController());
  }
}

class FungiController extends GetxController {
  var isServiceRunning = false.obs;
  var peerId = ''.obs;

  @override
  void onInit() {
    super.onInit();
    initFungi();
  }

  Future<void> initFungi() async {
    try {
      await fungi.startFungiDaemon();
      isServiceRunning.value = true;
      debugPrint('Fungi Daemon started');

      String id = fungi.peerId();
      peerId.value = id;
      debugPrint('Peer ID: $id');
    } catch (e) {
      isServiceRunning.value = false;
      peerId.value = 'error';
      debugPrint('Failed to init, error: $e');
    }
  }
}
