import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:fungi_app/ui/widgets/dialogs.dart';
import 'package:get/get.dart';
import 'package:settings_ui/settings_ui.dart';
import 'package:flutter_smart_dialog/flutter_smart_dialog.dart';

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
                _showThemeDialog();
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
                SmartDialog.showToast('Path copied to clipboard');
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
                showAllowedPeersList();
              },
            ),
          ],
        ),
      ],
    );
  }

  void _showThemeDialog() {
    SmartDialog.show(
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
                      SmartDialog.dismiss();
                    }
                  },
                ),
                onTap: () {
                  controller.changeTheme(option);
                  SmartDialog.dismiss();
                },
              );
            }).toList(),
          ),
          actions: [
            TextButton(
              onPressed: () => SmartDialog.dismiss(),
              child: Text('Cancel'),
            ),
          ],
        );
      },
    );
  }
}
