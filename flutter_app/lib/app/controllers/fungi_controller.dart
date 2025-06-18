import 'package:flutter/material.dart';
import 'package:get/get.dart';
import 'package:flutter_app/src/rust/api/fungi.dart' as fungi;

class HomeBinding implements Bindings {
  @override
  void dependencies() {
    Get.put(FungiController());
  }
}

class FungiController extends GetxController {
  var isServiceRunning = false.obs;
  var peerId = ''.obs;
  var isLoading = true.obs;

  @override
  void onInit() {
    super.onInit();
    initFungi();
  }

  Future<void> initFungi() async {
    isLoading.value = true;

    try {
      await fungi.startFungiDaemon();
      isServiceRunning.value = true;
      debugPrint('Fungi Daemon started');

      String? id = await fungi.peerId();
      if (id != null) {
        peerId.value = id;
        debugPrint('Peer ID: $id');
      } else {
        peerId.value = 'error';
      }
    } catch (e) {
      isServiceRunning.value = false;
      peerId.value = 'error';
      debugPrint('Failed to init, error: $e');
    } finally {
      isLoading.value = false;
    }
  }
}
