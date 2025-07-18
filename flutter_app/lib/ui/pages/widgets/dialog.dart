import 'package:flutter/material.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:get/get.dart';

void showAllowedPeersList(BuildContext context) {
  final controller = Get.find<FungiController>();
  showDialog(
    context: context,
    builder: (context) {
      return AlertDialog(
        title: const Text('Incoming Allowed Peers'),
        content: SizedBox(
          width: double.maxFinite,
          child: Obx(() {
            if (controller.incomingAllowdPeers.isEmpty) {
              return const Center(
                child: Padding(
                  padding: EdgeInsets.all(16.0),
                  child: Text('No peers allowed'),
                ),
              );
            }

            return ListView.builder(
              shrinkWrap: true,
              itemCount: controller.incomingAllowdPeers.length,
              itemBuilder: (context, index) {
                final peerId = controller.incomingAllowdPeers[index];
                return ListTile(
                  title: SelectableText(
                    peerId,
                    style: const TextStyle(fontSize: 14),
                  ),
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
            onPressed: () => showAddPeerDialog(
              context,
              (String text) => controller.addIncomingAllowedPeer(text),
            ),
            child: const Text('Add Peer'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Close'),
          ),
        ],
      );
    },
  );
}

void showAddPeerDialog(BuildContext context, void Function(String) onAddPeer) {
  final textController = TextEditingController();
  final errorMessage = RxString('');
  showDialog(
    context: context,
    builder: (context) {
      return AlertDialog(
        title: const Text('Add Peer'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: textController,
              decoration: const InputDecoration(
                labelText: 'Peer ID',
                hintText: 'Enter peer ID',
              ),
              autofocus: true,
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
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () {
              if (textController.text.isEmpty) {
                errorMessage.value = 'Peer ID cannot be empty';
                return;
              }
              try {
                onAddPeer(textController.text);
                Navigator.pop(context);
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

void showAddClientDialog(
  BuildContext context,
  Future<void> Function(Client) onAddClient,
) {
  final peerIdTextController = TextEditingController();
  final nameTextController = TextEditingController();
  final enabled = RxBool(true);
  final errorMessage = RxString('');
  showDialog(
    context: context,
    builder: (context) {
      return AlertDialog(
        title: const Text('Add Peer'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: nameTextController,
              decoration: const InputDecoration(
                labelText: 'Name',
                hintText: 'Enter a device name',
              ),
            ),
            TextField(
              controller: peerIdTextController,
              decoration: const InputDecoration(
                labelText: 'Peer ID',
                hintText: 'Enter peer ID',
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
            onPressed: () => Navigator.pop(context),
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
                if (context.mounted) {
                  Navigator.pop(context);
                }
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
