import 'package:flutter/material.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:fungi_app/src/rust/api/fungi.dart';
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
            onPressed: () => showAddAllowedPeerDialog(),
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

void showAddAllowedPeerDialog() {
  final textPeerIdController = TextEditingController();
  final textAliasController = TextEditingController();
  final Rx<PeerInfo> selectedPeer = PeerInfo.empty().obs;
  final errorMessage = RxString('');
  final controller = Get.find<FungiController>();

  SmartDialog.show(
    builder: (context) {
      return AlertDialog(
        title: const Text('Add Peer'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextButton.icon(
              icon: const Icon(Icons.bookmarks_outlined),
              label: const Text('Select from Address Book'),
              onPressed: () async {
                final newSelectedPeer = await showAddressBookSelectorDialog();
                if (newSelectedPeer == null) return;
                selectedPeer.value = newSelectedPeer;
                textPeerIdController.text = selectedPeer.value.peerId;
                textAliasController.text = selectedPeer.value.hostname ?? '';
              },
            ),
            TextButton.icon(
              icon: const Icon(Icons.devices),
              label: const Text('Select from Local Devices(mDNS)'),
              onPressed: () async {
                final newSelectedPeer =
                    await showMdnsLocalDevicesSelectorDialog();
                if (newSelectedPeer == null) return;
                selectedPeer.value = newSelectedPeer;
                textPeerIdController.text = selectedPeer.value.peerId;
                textAliasController.text = selectedPeer.value.hostname ?? '';
              },
            ),
            Obx(
              () => TextField(
                controller: textPeerIdController,
                decoration: InputDecoration(
                  labelText: 'Peer ID',
                  hintText: 'Enter peer ID',
                  helperText:
                      selectedPeer.value.peerId == textPeerIdController.text
                      ? selectedPeer.value.hostname
                      : null,
                ),
              ),
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

              if (selectedPeer.value.peerId != textPeerIdController.text) {
                // reset the selectedPeer
                selectedPeer.value = PeerInfo.empty();
              }

              selectedPeer.value.peerId = textPeerIdController.text;
              selectedPeer.value.alias = textAliasController.text.isEmpty
                  ? null
                  : textAliasController.text;
              try {
                controller.addIncomingAllowedPeer(selectedPeer.value);
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

void showAddFileClientDialog() {
  final textPeerIdController = TextEditingController();
  final textAliasController = TextEditingController();
  final Rx<PeerInfo> selectedPeer = PeerInfo.empty().obs;
  final enabled = RxBool(true);
  final errorMessage = RxString('');
  final controller = Get.find<FungiController>();

  SmartDialog.show(
    builder: (context) {
      return AlertDialog(
        title: const Text('Add Peer'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextButton.icon(
              icon: const Icon(Icons.bookmarks_outlined),
              label: const Text('Select from Address Book'),
              onPressed: () async {
                final newSelectedPeer = await showAddressBookSelectorDialog();
                if (newSelectedPeer == null) return;
                selectedPeer.value = newSelectedPeer;
                textPeerIdController.text = selectedPeer.value.peerId;
                textAliasController.text = selectedPeer.value.hostname ?? '';
              },
            ),
            TextButton.icon(
              icon: const Icon(Icons.devices),
              label: const Text('Select from Local Devices(mDNS)'),
              onPressed: () async {
                final newSelectedPeer =
                    await showMdnsLocalDevicesSelectorDialog();
                if (newSelectedPeer == null) return;
                selectedPeer.value = newSelectedPeer;
                textPeerIdController.text = selectedPeer.value.peerId;
                textAliasController.text = selectedPeer.value.hostname ?? '';
              },
            ),
            TextField(
              controller: textPeerIdController,
              decoration: InputDecoration(
                labelText: 'Peer ID',
                hintText: 'Enter peer ID',
                helperText:
                    selectedPeer.value.peerId == textPeerIdController.text
                    ? selectedPeer.value.hostname
                    : null,
              ),
              autofocus: true,
            ),
            TextField(
              controller: textAliasController,
              decoration: const InputDecoration(
                labelText: 'Device Alias',
                hintText: 'Enter a device alias',
                helperText:
                    'Device alias will be displayed as filename in mount directory',
              ),
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
              if (textPeerIdController.text.isEmpty) {
                errorMessage.value = 'Peer ID cannot be empty';
                return;
              }
              if (selectedPeer.value.peerId != textPeerIdController.text) {
                // reset the selectedPeer
                selectedPeer.value = PeerInfo.empty();
              }
              selectedPeer.value.peerId = textPeerIdController.text;
              selectedPeer.value.alias = textAliasController.text.isEmpty
                  ? null
                  : textAliasController.text;
              try {
                await controller.addFileTransferClient(
                  enabled: enabled.value,
                  peerInfo: selectedPeer.value,
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
