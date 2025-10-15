import 'package:flutter/material.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:get/get.dart';

class DaemonConnectionOverlay extends GetView<FungiController> {
  final Widget child;

  const DaemonConnectionOverlay({super.key, required this.child});

  @override
  Widget build(BuildContext context) {
    return Obx(() {
      final state = controller.daemonConnectionState.value;

      if (state.isConnected) {
        return child;
      }

      return Stack(
        children: [
          child,
          Container(
            color: Colors.black.withValues(alpha: 0.7),
            child: Center(
              child: Card(
                margin: const EdgeInsets.all(32),
                child: Padding(
                  padding: const EdgeInsets.all(32),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      if (state.isConnecting) ...[
                        const CircularProgressIndicator(),
                        const SizedBox(height: 24),
                        const Text(
                          'Connecting to Fungi Daemon...',
                          style: TextStyle(fontSize: 16),
                        ),
                      ] else if (state.isFailed) ...[
                        const Icon(
                          Icons.error_outline,
                          size: 64,
                          color: Colors.red,
                        ),
                        const SizedBox(height: 24),
                        const Text(
                          'Failed to Connect',
                          style: TextStyle(
                            fontSize: 20,
                            fontWeight: FontWeight.bold,
                          ),
                        ),
                        const SizedBox(height: 16),
                        Text(
                          controller.daemonError.value,
                          textAlign: TextAlign.center,
                          style: TextStyle(
                            fontSize: 14,
                            color: Colors.red.shade700,
                          ),
                        ),
                        const SizedBox(height: 24),
                        TextButton.icon(
                          onPressed: () => controller.retryConnection(),
                          icon: const Icon(Icons.refresh),
                          label: const Text('Retry'),
                          style: ElevatedButton.styleFrom(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 32,
                              vertical: 16,
                            ),
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
              ),
            ),
          ),
        ],
      );
    });
  }
}
