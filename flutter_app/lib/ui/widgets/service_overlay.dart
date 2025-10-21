import 'package:flutter/material.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:get/get.dart';

/// Overlay widget that shows a message when the service is disabled
/// Only shows when the service is explicitly disabled (not connecting or failed)
class ServiceOverlay extends GetView<FungiController> {
  final Widget child;

  const ServiceOverlay({super.key, required this.child});

  @override
  Widget build(BuildContext context) {
    return Obx(() {
      final state = controller.daemonConnectionState.value;
      // Only show overlay when service is explicitly disabled
      final showOverlay = state.isDisabled;

      return Stack(
        children: [
          child,
          if (showOverlay)
            Positioned.fill(
              child: Container(
                color: Colors.black.withValues(alpha: 0.5),
                child: Center(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      const Icon(
                        Icons.power_off,
                        size: 64,
                        color: Colors.white70,
                      ),
                      const SizedBox(height: 16),
                      const Text(
                        'Please start the service',
                        style: TextStyle(
                          color: Colors.white,
                          fontSize: 18,
                          fontWeight: FontWeight.w500,
                        ),
                        textAlign: TextAlign.center,
                      ),
                    ],
                  ),
                ),
              ),
            ),
        ],
      );
    });
  }
}
