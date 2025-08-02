import 'package:flutter/material.dart';
import 'package:flutter_smart_dialog/flutter_smart_dialog.dart';
import '../../src/rust/api/fungi.dart' as fungi_api;

class DeviceSelectorDialog {
  static Future<fungi_api.DeviceInfo?> show({
    required String title,
    String? dialogId,
  }) async {
    return await SmartDialog.show<fungi_api.DeviceInfo>(
      tag: dialogId,
      builder: (context) =>
          DeviceSelectorDialogWidget(title: title, dialogId: dialogId),
      alignment: Alignment.center,
      maskColor: Colors.black54,
      clickMaskDismiss: true,
    );
  }
}

class DeviceSelectorDialogWidget extends StatefulWidget {
  final String title;
  final String? dialogId;

  const DeviceSelectorDialogWidget({
    super.key,
    required this.title,
    this.dialogId,
  });

  @override
  State<DeviceSelectorDialogWidget> createState() =>
      _DeviceSelectorDialogWidgetState();
}

class _DeviceSelectorDialogWidgetState
    extends State<DeviceSelectorDialogWidget> {
  List<fungi_api.DeviceInfo> devices = [];
  bool isLoading = true;
  String? error;

  @override
  void initState() {
    super.initState();
    _loadDevices();
  }

  Future<void> _loadDevices() async {
    try {
      setState(() {
        isLoading = true;
        error = null;
      });

      final result = await fungi_api.getLocalDevices();

      result.sort((a, b) {
        final aName = a.hostname ?? 'Unknown';
        final bName = b.hostname ?? 'Unknown';

        if (aName == 'Unknown' && bName != 'Unknown') return 1;
        if (bName == 'Unknown' && aName != 'Unknown') return -1;

        return aName.compareTo(bName);
      });

      setState(() {
        devices = result;
        isLoading = false;
      });
    } catch (e) {
      setState(() {
        error = e.toString();
        isLoading = false;
      });
    }
  }

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
                      widget.title,
                      style: Theme.of(context).textTheme.titleLarge?.copyWith(
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                  ),
                  IconButton(
                    onPressed: _loadDevices,
                    icon: Icon(
                      Icons.refresh,
                      color: Theme.of(context).primaryColor,
                    ),
                    tooltip: 'Refresh device list',
                  ),
                  IconButton(
                    onPressed: () => SmartDialog.dismiss(tag: widget.dialogId),
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
                    onPressed: () => SmartDialog.dismiss(tag: widget.dialogId),
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
    if (isLoading) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(32),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              CircularProgressIndicator(),
              SizedBox(height: 16),
              Text('Searching for network devices...'),
            ],
          ),
        ),
      );
    }

    if (error != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(32),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                Icons.error_outline,
                size: 48,
                color: Theme.of(context).colorScheme.error,
              ),
              const SizedBox(height: 16),
              Text(
                'Failed to load device list',
                style: Theme.of(context).textTheme.titleMedium,
              ),
              const SizedBox(height: 8),
              Text(
                error!,
                style: Theme.of(context).textTheme.bodySmall,
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 16),
              ElevatedButton(
                onPressed: _loadDevices,
                child: const Text('Retry'),
              ),
            ],
          ),
        ),
      );
    }

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
                'No network devices found',
                style: TextStyle(fontSize: 16, color: Colors.grey),
              ),
              SizedBox(height: 8),
              Text(
                'Make sure target devices are powered on and connected to the same network',
                style: TextStyle(fontSize: 14, color: Colors.grey),
                textAlign: TextAlign.center,
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
                color: Theme.of(context).primaryColor,
              ),
              Positioned(
                right: -2,
                top: -2,
                child: Container(
                  width: 12,
                  height: 12,
                  decoration: const BoxDecoration(
                    color: Colors.green,
                    shape: BoxShape.circle,
                  ),
                ),
              ),
            ],
          ),
          title: Text(
            device.hostname ?? 'Unknown',
            style: const TextStyle(fontWeight: FontWeight.bold),
          ),
          subtitle: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Peer ID: ${_truncatePeerId(device.peerId)}',
                style: TextStyle(
                  fontFamily: 'monospace',
                  color: Colors.grey[600],
                ),
              ),
              if (device.ipAddress != null)
                Text(
                  'IP: ${device.ipAddress}',
                  style: TextStyle(color: Colors.grey[600]),
                ),
              Text(
                device.os,
                style: TextStyle(color: Colors.grey[500], fontSize: 12),
              ),
            ],
          ),
          trailing: const Icon(Icons.arrow_forward_ios, size: 16),
          onTap: () {
            SmartDialog.dismiss(tag: widget.dialogId, result: device);
          },
        );
      },
    );
  }
}
