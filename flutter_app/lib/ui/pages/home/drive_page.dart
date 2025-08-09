import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:fungi_app/ui/pages/home/home_page.dart';
import 'package:fungi_app/ui/widgets/dialogs.dart';
import 'package:fungi_app/ui/widgets/text.dart';
import 'package:fungi_app/ui/widgets/enhanced_card.dart';
import 'package:get/get.dart';

class RemoteFileAccess extends GetView<FungiController> {
  const RemoteFileAccess({super.key});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.all(16),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.start,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            "Remote File Access",
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          Text(
            "Map remote device folders as local FTP/WebDAV drives. \nAccess remote folders at the addresses below.",
            style: Theme.of(context).textTheme.labelSmall?.apply(
              color: Theme.of(context).colorScheme.onSurface.withAlpha(150),
            ),
          ),
          SizedBox(height: 10),
          LabeledText(
            label: "- FTP Server",
            labelStyle: Theme.of(context).textTheme.labelSmall,
            value: "",
            valueWidget: Obx(
              () => controller.ftpProxy.value.enabled
                  ? SelectableText(
                      "ftp://${controller.ftpProxy.value.host}:${controller.ftpProxy.value.port}",
                      style: Theme.of(context).textTheme.bodySmall,
                    )
                  : Text(
                      "Disabled",
                      style: Theme.of(
                        context,
                      ).textTheme.bodySmall?.apply(color: Colors.red),
                    ),
            ),
          ),
          LabeledText(
            label: "- WebDAV Server",
            labelStyle: Theme.of(context).textTheme.labelSmall,
            value: "",
            valueWidget: Obx(
              () => controller.webdavProxy.value.enabled
                  ? SelectableText(
                      "http://${controller.webdavProxy.value.host}:${controller.webdavProxy.value.port}",
                      style: Theme.of(context).textTheme.bodySmall,
                    )
                  : Text(
                      "Disabled",
                      style: Theme.of(
                        context,
                      ).textTheme.bodySmall?.apply(color: Colors.red),
                    ),
            ),
            style: Theme.of(context).textTheme.bodySmall,
          ),
          SizedBox(height: 10),
          TextButton.icon(
            onPressed: () => showAddFileClientDialog(),
            icon: Icon(Icons.add_circle),
            label: Text("Add Remote Device"),
          ),
          SizedBox(height: 10),
          Obx(
            () => controller.fileTransferClients.isEmpty
                ? Text(
                    "-- No remote devices. --",
                    style: Theme.of(context).textTheme.bodySmall?.apply(
                      color: Theme.of(
                        context,
                      ).colorScheme.onSurface.withAlpha(150),
                    ),
                  )
                : Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: controller.fileTransferClients.map((client) {
                      return EnhancedCard(
                        child: ListTile(
                          title: Tooltip(
                            message: client.name ?? client.peerId,
                            waitDuration: const Duration(milliseconds: 1000),
                            child: Text(
                              client.name ?? client.peerId,
                              overflow: TextOverflow.ellipsis,
                              maxLines: 1,
                            ),
                          ),
                          subtitle: TruncatedId(
                            id: client.peerId,
                            style: Theme.of(context).textTheme.bodySmall?.apply(
                              color: Theme.of(
                                context,
                              ).colorScheme.onSurface.withAlpha(150),
                            ),
                          ),
                          trailing: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Transform.scale(
                                scale: 0.6,
                                child: Switch(
                                  value: client.enabled,
                                  onChanged: (value) {
                                    controller.enableFileTransferClient(
                                      client: client,
                                      enabled: value,
                                    );
                                  },
                                ),
                              ),
                              IconButton(
                                icon: Icon(
                                  Icons.delete,
                                  size: 20,
                                  color: Colors.red,
                                ),
                                padding: EdgeInsets.zero,
                                constraints: BoxConstraints(),
                                onPressed: () {
                                  controller.removeFileTransferClient(
                                    client.peerId,
                                  );
                                },
                              ),
                            ],
                          ),
                        ),
                      );
                    }).toList(),
                  ),
          ),
          SizedBox(height: 10),
        ],
      ),
    );
  }
}

class FileServer extends GetView<FungiController> {
  const FileServer({super.key});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.all(16),
      child: Obx(
        () => Column(
          mainAxisAlignment: MainAxisAlignment.start,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text("File Server", style: Theme.of(context).textTheme.bodyLarge),
            Text(
              "Share files to other devices.",
              style: Theme.of(context).textTheme.labelSmall?.apply(
                color: Theme.of(context).colorScheme.onSurface.withAlpha(150),
              ),
            ),
            SizedBox(height: 10),
            Row(
              mainAxisAlignment: MainAxisAlignment.start,
              crossAxisAlignment: CrossAxisAlignment.center,
              children: [
                Text("State", style: Theme.of(context).textTheme.bodyMedium),
                SizedBox(width: 5),
                IconButton(
                  onPressed: () async {
                    if (controller.fileTransferServerState.value.enabled) {
                      controller.stopFileTransferServer();
                    } else {
                      if (controller.fileTransferServerState.value.rootDir ==
                          null) {
                        debugPrint(
                          'Root directory not set. Please select a directory first.',
                        );
                        return;
                      }
                      await controller.startFileTransferServer(
                        controller.fileTransferServerState.value.rootDir!,
                      );
                    }
                  },
                  icon: controller.fileTransferServerState.value.enabled
                      ? Icon(Icons.toggle_on, color: Colors.green)
                      : Icon(Icons.toggle_off),
                ),
              ],
            ),
            Row(
              mainAxisAlignment: MainAxisAlignment.start,
              crossAxisAlignment: CrossAxisAlignment.center,
              children: [
                Text(
                  "Shared Directory: ",
                  style: Theme.of(context).textTheme.bodyMedium,
                ),
                SizedBox(width: 5),
                SelectableText(
                  controller.fileTransferServerState.value.rootDir != null
                      ? controller.fileTransferServerState.value.rootDir!
                      : "Not set",
                  style: Theme.of(context).textTheme.bodyMedium,
                ),
                IconButton(
                  onPressed: () async {
                    String? selectedDirectory = await FilePicker.platform
                        .getDirectoryPath();
                    if (selectedDirectory == null) {
                      // User canceled the picker
                      return;
                    }
                    debugPrint('Selected directory: $selectedDirectory');
                    await controller.startFileTransferServer(selectedDirectory);
                  },
                  icon: Icon(Icons.edit, size: 15),
                ),
              ],
            ),
            Row(
              mainAxisAlignment: MainAxisAlignment.start,
              crossAxisAlignment: CrossAxisAlignment.center,
              children: [
                Text(
                  "Incoming Allowed Peers: ",
                  style: Theme.of(context).textTheme.bodyMedium,
                ),
                SizedBox(width: 5),
                SelectableText(
                  "${controller.incomingAllowdPeers.length}",
                  style: Theme.of(context).textTheme.bodyMedium,
                ),
                IconButton(
                  onPressed: () {
                    showAllowedPeersList();
                  },
                  icon: Icon(Icons.edit, size: 15),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class FileTransferPage extends GetView<FungiController> {
  const FileTransferPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisAlignment: MainAxisAlignment.start,
      children: [RemoteFileAccess(), Divider(), FileServer()],
    );
  }
}
