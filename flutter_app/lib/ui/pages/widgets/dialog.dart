import 'package:flutter/material.dart';
import 'package:get/get.dart';

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
