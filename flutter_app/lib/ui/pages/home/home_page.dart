import 'package:flutter/material.dart';
import 'package:get/get.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';

class HomePage extends StatelessWidget {
  const HomePage({super.key});

  @override
  Widget build(BuildContext context) {
    final controller = Get.find<FungiController>();

    return Scaffold(
      appBar: AppBar(title: const Text('Fungi App')),
      body: Center(
        child: Obx(() {
          if (controller.isLoading.value) {
            return const CircularProgressIndicator();
          } else {
            return Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text(
                  'Service state: ${controller.isServiceRunning.value ? "ON" : "OFF"}',
                  style: const TextStyle(fontSize: 18),
                ),
                const SizedBox(height: 20),
                Text(
                  'Peer ID: ${controller.peerId.value}',
                  style: const TextStyle(fontSize: 18),
                ),
                const SizedBox(height: 20),
              ],
            );
          }
        }),
      ),
    );
  }
}
