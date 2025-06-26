import 'package:flutter/material.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:fungi_app/ui/pages/home/home_page.dart';
import 'package:fungi_app/ui/pages/widgets/dialog.dart';
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
            style: Theme.of(context).textTheme.headlineSmall,
          ),
          Text(
            "Connect to remote devices to access and manage files.",
            style: Theme.of(context).textTheme.bodySmall,
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
            onPressed: () => showAddClientDialog(
              context,
              (Client client) => controller.addFileTransferClient(
                enabled: client.enabled,
                peerId: client.peerId,
                name: client.name,
              ),
            ),
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
                      return ListTile(
                        title: Tooltip(
                          message: client.name ?? client.peerId,
                          child: Text(
                            client.name ?? client.peerId,
                            overflow: TextOverflow.ellipsis,
                            maxLines: 1,
                          ),
                        ),
                        subtitle: Tooltip(
                          message: client.peerId,
                          child: Text(
                            client.peerId,
                            overflow: TextOverflow.ellipsis,
                            maxLines: 1,
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
                              icon: Icon(Icons.remove_circle, size: 20),
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
    return Text(
      'File Server',
      style: Theme.of(context).textTheme.headlineSmall,
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
