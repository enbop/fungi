import 'package:flutter/material.dart';
import 'package:flutter_smart_dialog/flutter_smart_dialog.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:get/get.dart';

import 'dart:math';

import '../../src/grpc/generated/fungi_daemon.pb.dart';

Future<PeerInfo?> showAddressBookSelectorDialog() async {
  final dialogId =
      DateTime.now().millisecondsSinceEpoch.toString() +
      Random().nextInt(100).toString();

  final controller = Get.find<FungiController>();
  return await SmartDialog.show<PeerInfo>(
    tag: dialogId,
    builder: (context) => DeviceSelectorDialogWidget(
      title: "Select From Address Book",
      dialogId: dialogId,
      devices: controller.addressBook,
    ),
    alignment: Alignment.center,
    maskColor: Colors.black54,
    clickMaskDismiss: true,
  );
}

Future<PeerInfo?> showMdnsLocalDevicesSelectorDialog() async {
  final dialogId =
      DateTime.now().millisecondsSinceEpoch.toString() +
      Random().nextInt(100).toString();
  final controller = Get.find<FungiController>();

  final devices = await controller.fungiClient.mdnsGetLocalDevices(Empty());
  return await SmartDialog.show<PeerInfo>(
    tag: dialogId,
    builder: (context) => DeviceSelectorDialogWidget(
      title: "Select From Local Devices(mDNS)",
      dialogId: dialogId,
      devices: devices.peers,
      showOnlineStatus: true,
    ),
    alignment: Alignment.center,
    maskColor: Colors.black54,
    clickMaskDismiss: true,
  );
}

class DeviceSelectorDialogWidget extends StatelessWidget {
  final String title;
  final String dialogId;
  final List<PeerInfo> devices;
  final bool showOnlineStatus;

  const DeviceSelectorDialogWidget({
    super.key,
    required this.title,
    required this.dialogId,
    required this.devices,
    this.showOnlineStatus = false,
  });

  String _truncatePeerId(String peerId) {
    if (peerId.length <= 15) return peerId;
    return '${peerId.substring(0, 4)}***${peerId.substring(peerId.length - 6)}';
  }

  IconData _getOsIcon(String os) {
    switch (os.toLowerCase()) {
      case 'windows':
        return Icons.desktop_windows;
      case 'macos':
        return Icons.laptop_mac;
      case 'linux':
        return Icons.computer;
      case 'android':
        return Icons.android;
      case 'ios':
        return Icons.phone_iphone;
      default:
        return Icons.device_unknown;
    }
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
      child: Container(
        width: 500,
        constraints: const BoxConstraints(maxHeight: 600),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            // Header
            Container(
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                color: Theme.of(context).primaryColor.withValues(alpha: 0.1),
                borderRadius: const BorderRadius.only(
                  topLeft: Radius.circular(12),
                  topRight: Radius.circular(12),
                ),
              ),
              child: Row(
                children: [
                  Icon(Icons.devices, color: Theme.of(context).primaryColor),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      title,
                      style: Theme.of(context).textTheme.titleLarge?.copyWith(
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                  ),
                  IconButton(
                    onPressed: () => SmartDialog.dismiss(tag: dialogId),
                    icon: const Icon(Icons.close),
                    tooltip: 'Close',
                  ),
                ],
              ),
            ),

            // Content
            Flexible(child: _buildContent()),

            // Footer
            Container(
              padding: const EdgeInsets.all(16),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  TextButton(
                    onPressed: () => SmartDialog.dismiss(tag: dialogId),
                    child: const Text('Cancel'),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildContent() {
    if (devices.isEmpty) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(32),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(Icons.devices_other, size: 48, color: Colors.grey),
              SizedBox(height: 16),
              Text(
                'No devices found',
                style: TextStyle(fontSize: 16, color: Colors.grey),
              ),
            ],
          ),
        ),
      );
    }

    return ListView.separated(
      shrinkWrap: true,
      itemCount: devices.length,
      separatorBuilder: (context, index) => const Divider(height: 1),
      itemBuilder: (context, index) {
        final device = devices[index];
        return ListTile(
          leading: Stack(
            children: [
              Icon(
                _getOsIcon(device.os),
                size: 32,
                color: Theme.of(context).secondaryHeaderColor,
              ),
              if (showOnlineStatus)
                Positioned(
                  right: 0,
                  top: 0,
                  child: Container(
                    width: 10,
                    height: 10,
                    decoration: const BoxDecoration(
                      color: Colors.green,
                      shape: BoxShape.circle,
                    ),
                  ),
                ),
            ],
          ),
          title: Text(
            _truncatePeerId(device.peerId),
            style: const TextStyle(fontWeight: FontWeight.bold),
          ),
          subtitle: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Hostname: ${device.hostname ?? 'Unknown'}',
                style: TextStyle(
                  fontFamily: 'monospace',
                  color: Colors.grey[600],
                ),
              ),
              if (device.alias != null && device.alias!.isNotEmpty)
                Text(
                  'Alias: ${device.alias}',
                  style: TextStyle(
                    fontFamily: 'monospace',
                    color: Colors.grey[600],
                  ),
                ),
              if (device.privateIps.isNotEmpty)
                Text(
                  'IP: ${device.privateIps.first}',
                  style: TextStyle(color: Colors.grey[600]),
                ),
              Text(
                'OS: ${device.os}',
                style: TextStyle(color: Colors.grey[500], fontSize: 12),
              ),
            ],
          ),
          trailing: const Icon(Icons.arrow_forward_ios, size: 16),
          onTap: () {
            SmartDialog.dismiss(tag: dialogId, result: device);
          },
        );
      },
    );
  }
}
