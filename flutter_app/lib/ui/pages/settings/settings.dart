import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:get/get.dart';
import 'package:settings_ui/settings_ui.dart';

class Settings extends GetView<FungiController> {
  const Settings({super.key});

  @override
  Widget build(BuildContext context) {
    return SettingsList(
      platform: DevicePlatform.android,
      sections: [
        SettingsSection(
          title: Text('Common'),
          tiles: <SettingsTile>[
            SettingsTile.navigation(
              leading: Icon(Icons.language),
              title: Text('Language'),
              value: Text('English'),
            ),
            SettingsTile.navigation(
              leading: Icon(Icons.format_paint),
              title: Text('Theme'),
              value: Obx(() => Text(controller.currentTheme.value.name)),
              onPressed: (context) {
                _showThemeDialog(context);
              },
            ),
            SettingsTile.navigation(
              leading: Icon(Icons.file_open),
              title: Text('Config file path'),
              value: Obx(() => Text(controller.configFilePath.value)),
              onPressed: (context) {
                Clipboard.setData(
                  ClipboardData(text: controller.configFilePath.value),
                );
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(
                    content: Text('Path copied to clipboard'),
                    duration: Duration(seconds: 1),
                  ),
                );
              },
            ),
          ],
        ),
        SettingsSection(
          title: Text('Network'),
          tiles: <SettingsTile>[
            SettingsTile.navigation(
              leading: Icon(Icons.security),
              title: Text('Incoming Allowed Peers'),
              value: Obx(
                () => Text('${controller.incomingAllowdPeers.length} peers'),
              ),
              onPressed: (context) {
                _showAllowedPeersList(context);
              },
            ),
          ],
        ),
      ],
    );
  }

  void _showThemeDialog(BuildContext context) {
    showDialog(
      context: context,
      builder: (context) {
        return AlertDialog(
          title: Text('Select Theme'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: ThemeOption.values.map((option) {
              return ListTile(
                title: Text(option.name),
                leading: Radio<ThemeOption>(
                  value: option,
                  groupValue: controller.currentTheme.value,
                  onChanged: (ThemeOption? value) {
                    if (value != null) {
                      controller.changeTheme(value);
                      Navigator.pop(context);
                    }
                  },
                ),
                onTap: () {
                  controller.changeTheme(option);
                  Navigator.pop(context);
                },
              );
            }).toList(),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text('Cancel'),
            ),
          ],
        );
      },
    );
  }

  void _showAllowedPeersList(BuildContext context) {
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
                    title: Text(peerId, style: const TextStyle(fontSize: 14)),
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
              onPressed: () => _showAddPeerDialog(context),
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

  void _showAddPeerDialog(BuildContext context) {
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
                if (textController.text.isNotEmpty) {
                  try {
                    controller.addIncomingAllowedPeer(textController.text);
                    Navigator.pop(context);
                  } catch (e) {
                    errorMessage.value = 'Failed to add peer: $e';
                  }
                }
              },
              child: const Text('Add'),
            ),
          ],
        );
      },
    );
  }
}
