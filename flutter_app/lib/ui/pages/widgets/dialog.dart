import 'package:flutter/material.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:get/get.dart';
import 'package:flutter_smart_dialog/flutter_smart_dialog.dart';
import '../../widgets/device_selector_dialog.dart';

void showAllowedPeersList() {
  final controller = Get.find<FungiController>();
  SmartDialog.show(
    builder: (context) {
      return AlertDialog(
        title: const Text('Incoming Allowed Peers'),
        content: SizedBox(
          width: double.maxFinite,
          child: Obx(() {
            if (controller.incomingAllowedPeersWithInfo.isEmpty) {
              return const Center(
                child: Padding(
                  padding: EdgeInsets.all(16.0),
                  child: Text('No peers allowed'),
                ),
              );
            }

            return ListView.builder(
              shrinkWrap: true,
              itemCount: controller.incomingAllowedPeersWithInfo.length,
              itemBuilder: (context, index) {
                final peerWithInfo =
                    controller.incomingAllowedPeersWithInfo[index];
                final peerId = peerWithInfo.peerId;
                final peerInfo = peerWithInfo.peerInfo;

                String displayName = peerId;
                String subtitle = peerId;

                if (peerInfo != null && peerInfo.hostname != null) {
                  displayName = peerInfo.hostname!;
                  subtitle = peerId;
                }

                return ListTile(
                  title: SelectableText(
                    displayName,
                    style: const TextStyle(
                      fontSize: 14,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                  subtitle: displayName != subtitle
                      ? SelectableText(
                          subtitle,
                          style: const TextStyle(
                            fontSize: 12,
                            color: Colors.grey,
                          ),
                        )
                      : null,
                  trailing: IconButton(
                    icon: const Icon(
                      Icons.remove_circle_outline,
                      color: Colors.red,
                      size: 20,
                    ),
                    onPressed: () {
                      controller.removeIncomingAllowedPeer(peerId);
                    },
                  ),
                  dense: true,
                );
              },
            );
          }),
        ),
        actions: [
          TextButton(
            onPressed: () => showAddPeerDialog(),
            child: const Text('Add Peer'),
          ),
          TextButton(
            onPressed: () => SmartDialog.dismiss(),
            child: const Text('Close'),
          ),
        ],
      );
    },
  );
}

void showAddPeerDialog() {
  final textPeerIdController = TextEditingController();
  final textAliasController = TextEditingController();
  final errorMessage = RxString('');
  final controller = Get.find<FungiController>();

  SmartDialog.show(
    builder: (context) {
      return AlertDialog(
        title: const Text('Add Peer'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.start,
              children: [
                IconButton(
                  icon: const Icon(Icons.devices_other),
                  tooltip: 'Select from known devices',
                  onPressed: () async {
                    if (controller.knownPeers.isEmpty) {
                      SmartDialog.showToast('No known devices');
                      return;
                    }
                    // TODO add showKnownPeersDialog
                    // final selectedPeer = await showKnownPeersDialog(
                    //   controller.knownPeers,
                    // );
                    // if (selectedPeer != null) {
                    //   textPeerIdController.text = selectedPeer.peerId;
                    //   textAliasController.text = selectedPeer.hostname ?? '';
                    // }
                  },
                ),
                IconButton(
                  icon: const Icon(Icons.devices),
                  tooltip: 'Select from network devices',
                  onPressed: () async {
                    // TODO merge DeviceSelectorDialog with showKnownPeersDialog
                    final selectedDevice = await DeviceSelectorDialog.show(
                      title: 'Select Network Device',
                      dialogId: 'device_selector_add_peer',
                    );
                    if (selectedDevice != null) {
                      textPeerIdController.text = selectedDevice.peerId;
                      textAliasController.text = selectedDevice.hostname ?? '';
                    }
                  },
                ),
              ],
            ),
            TextField(
              controller: textPeerIdController,
              decoration: const InputDecoration(
                labelText: 'Peer ID',
                hintText: 'Enter peer ID',
              ),
              autofocus: true,
            ),
            const SizedBox(height: 8),
            TextField(
              controller: textAliasController,
              decoration: const InputDecoration(
                labelText: 'Alias (Optional)',
                hintText: 'Enter a friendly name for this device',
              ),
            ),
            Obx(
              () => errorMessage.isNotEmpty
                  ? Padding(
                      padding: const EdgeInsets.only(top: 8.0),
                      child: Text(
                        errorMessage.value,
                        style: const TextStyle(color: Colors.red),
                      ),
                    )
                  : const SizedBox.shrink(),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => SmartDialog.dismiss(),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () {
              if (textPeerIdController.text.isEmpty) {
                errorMessage.value = 'Peer ID cannot be empty';
                return;
              }
              try {
                controller.addIncomingAllowedPeer(
                  textPeerIdController.text,
                  textAliasController.text.isNotEmpty
                      ? textAliasController.text
                      : null,
                );
                SmartDialog.dismiss();
              } catch (e) {
                errorMessage.value = 'Failed to add peer: $e';
              }
            },
            child: const Text('Add'),
          ),
        ],
      );
    },
  );
}

class Client {
  final String peerId;
  final String name;
  final bool enabled;
  Client({required this.peerId, required this.name, required this.enabled});
}

void showAddClientDialog(Future<void> Function(Client) onAddClient) {
  final peerIdTextController = TextEditingController();
  final nameTextController = TextEditingController();
  final enabled = RxBool(true);
  final errorMessage = RxString('');
  SmartDialog.show(
    builder: (context) {
      return AlertDialog(
        title: const Text('Add Peer'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: nameTextController,
              decoration: const InputDecoration(
                labelText: 'Device Alias',
                hintText: 'Enter a device alias',
                helperText:
                    'Device alias will be displayed as filename in mount directory',
              ),
            ),
            TextField(
              controller: peerIdTextController,
              decoration: InputDecoration(
                labelText: 'Peer ID',
                hintText: 'Enter peer ID',
                suffixIcon: IconButton(
                  icon: const Icon(Icons.devices),
                  tooltip: 'Select from network devices',
                  onPressed: () async {
                    final selectedDevice = await DeviceSelectorDialog.show(
                      title: 'Select Network Device',
                      dialogId: 'device_selector_add_client',
                    );
                    if (selectedDevice != null) {
                      peerIdTextController.text = selectedDevice.peerId;
                      if (nameTextController.text.isEmpty &&
                          selectedDevice.hostname != null) {
                        nameTextController.text = selectedDevice.hostname!;
                      }
                    }
                  },
                ),
              ),
              autofocus: true,
            ),
            Row(
              children: [
                Obx(
                  () => Checkbox(
                    value: enabled.value,
                    onChanged: (value) {
                      if (value != null) {
                        enabled.value = value;
                      }
                    },
                  ),
                ),
                const Text('Enabled'),
              ],
            ),
            Obx(
              () => errorMessage.isNotEmpty
                  ? Padding(
                      padding: const EdgeInsets.only(top: 8.0),
                      child: Text(
                        errorMessage.value,
                        style: const TextStyle(color: Colors.red),
                      ),
                    )
                  : const SizedBox.shrink(),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => SmartDialog.dismiss(),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () async {
              if (peerIdTextController.text.isEmpty) {
                errorMessage.value = 'Peer ID cannot be empty';
                return;
              }
              if (nameTextController.text.isEmpty) {
                errorMessage.value = 'Name cannot be empty';
                return;
              }
              try {
                final client = Client(
                  peerId: peerIdTextController.text,
                  name: nameTextController.text,
                  enabled: enabled.value,
                );
                await onAddClient(client);
                SmartDialog.dismiss();
              } catch (e) {
                errorMessage.value = 'Failed to add peer: $e';
              }
            },
            child: const Text('Add'),
          ),
        ],
      );
    },
  );
}
