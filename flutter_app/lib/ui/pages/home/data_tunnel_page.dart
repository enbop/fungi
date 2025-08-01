import 'package:flutter/material.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:fungi_app/ui/pages/widgets/dialog.dart';
import 'package:fungi_app/ui/pages/widgets/text.dart';
import 'package:fungi_app/ui/pages/widgets/enhanced_card.dart';
import 'package:fungi_app/ui/widgets/device_selector_dialog.dart';
import 'package:get/get.dart';
import 'package:flutter_smart_dialog/flutter_smart_dialog.dart';

class DataTunnelPage extends GetView<FungiController> {
  const DataTunnelPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.all(16),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.start,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            "TCP Port Tunneling",
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          Text(
            "Create TCP tunnels to forward local ports to remote devices or expose local services through P2P connections.",
            style: Theme.of(context).textTheme.labelSmall?.apply(
              color: Theme.of(context).colorScheme.onSurface.withAlpha(150),
            ),
          ),
          SizedBox(height: 20),

          // Forwarding Rules Section
          _buildForwardingSection(context),

          SizedBox(height: 30),

          // Listening Rules Section
          _buildListeningSection(context),
        ],
      ),
    );
  }

  Widget _buildForwardingSection(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(
              Icons.arrow_forward,
              size: 20,
              color: Theme.of(context).colorScheme.primary,
            ),
            SizedBox(width: 8),
            Text(
              "Port Forwarding Rules",
              style: Theme.of(context).textTheme.titleMedium,
            ),
          ],
        ),
        Text(
          "Forward local ports to remote devices",
          style: Theme.of(context).textTheme.labelSmall?.apply(
            color: Theme.of(context).colorScheme.onSurface.withAlpha(150),
          ),
        ),
        SizedBox(height: 10),
        TextButton.icon(
          onPressed: () => _showAddForwardingRuleDialog(),
          icon: Icon(Icons.add_circle),
          label: Text("Add Forwarding Rule"),
        ),
        SizedBox(height: 10),
        Obx(
          () => controller.tcpTunnelingConfig.value.forwardingRules.isEmpty
              ? Text(
                  "-- No forwarding rules. --",
                  style: Theme.of(context).textTheme.bodySmall?.apply(
                    color: Theme.of(
                      context,
                    ).colorScheme.onSurface.withAlpha(150),
                  ),
                )
              : Column(
                  children: controller.tcpTunnelingConfig.value.forwardingRules.map((
                    rule,
                  ) {
                    final ruleId =
                        "forward_${rule.localHost}:${rule.localPort}_to_${rule.remotePeerId}";

                    return EnhancedCard(
                      child: ListTile(
                        title: Text(
                          "${rule.localHost}:${rule.localPort} → Remote:${rule.remotePort}",
                          style: Theme.of(context).textTheme.bodyMedium,
                        ),
                        subtitle: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            TruncatedId(
                              id: rule.remotePeerId,
                              style: Theme.of(context).textTheme.bodySmall
                                  ?.apply(
                                    color: Theme.of(
                                      context,
                                    ).colorScheme.onSurface.withAlpha(150),
                                  ),
                            ),
                            Text(
                              "Protocol: /fungi/tunnel/0.1.0/${rule.remotePort}",
                              style: Theme.of(context).textTheme.bodySmall
                                  ?.apply(
                                    color: Theme.of(
                                      context,
                                    ).colorScheme.onSurface.withAlpha(120),
                                  ),
                            ),
                          ],
                        ),
                        trailing: IconButton(
                          icon: Icon(Icons.delete, size: 20, color: Colors.red),
                          onPressed: () =>
                              controller.removeTcpForwardingRule(ruleId),
                        ),
                      ),
                    );
                  }).toList(),
                ),
        ),
      ],
    );
  }

  Widget _buildListeningSection(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(
              Icons.arrow_back,
              size: 20,
              color: Theme.of(context).colorScheme.secondary,
            ),
            SizedBox(width: 8),
            Text(
              "Port Listening Rules",
              style: Theme.of(context).textTheme.titleMedium,
            ),
          ],
        ),
        Text(
          "Expose local services to remote devices",
          style: Theme.of(context).textTheme.labelSmall?.apply(
            color: Theme.of(context).colorScheme.onSurface.withAlpha(150),
          ),
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
        SizedBox(height: 10),
        TextButton.icon(
          onPressed: () => _showAddListeningRuleDialog(),
          icon: Icon(Icons.add_circle),
          label: Text("Add Listening Rule"),
        ),
        SizedBox(height: 10),
        Obx(
          () => controller.tcpTunnelingConfig.value.listeningRules.isEmpty
              ? Text(
                  "-- No listening rules. --",
                  style: Theme.of(context).textTheme.bodySmall?.apply(
                    color: Theme.of(
                      context,
                    ).colorScheme.onSurface.withAlpha(150),
                  ),
                )
              : Column(
                  children: controller.tcpTunnelingConfig.value.listeningRules.map((
                    rule,
                  ) {
                    final ruleId = "listen_${rule.host}:${rule.port}";

                    return EnhancedCard(
                      accentColor: Theme.of(context).colorScheme.secondary,
                      child: ListTile(
                        title: Text(
                          "Local:${rule.host}:${rule.port}",
                          style: Theme.of(context).textTheme.bodyMedium,
                        ),
                        subtitle: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              "Protocol: /fungi/tunnel/0.1.0/${rule.port}",
                              style: Theme.of(context).textTheme.bodySmall
                                  ?.apply(
                                    color: Theme.of(
                                      context,
                                    ).colorScheme.onSurface.withAlpha(120),
                                  ),
                            ),
                            if (rule.allowedPeers.isNotEmpty)
                              Text(
                                "Allowed peers: ${rule.allowedPeers.length}",
                                style: Theme.of(context).textTheme.bodySmall
                                    ?.apply(
                                      color: Theme.of(
                                        context,
                                      ).colorScheme.onSurface.withAlpha(150),
                                    ),
                              ),
                            // else
                            //   Text(
                            //     "Open to all peers",
                            //     style: Theme.of(context).textTheme.bodySmall
                            //         ?.apply(color: Colors.orange),
                            //   ),
                          ],
                        ),
                        trailing: IconButton(
                          icon: Icon(Icons.delete, size: 20, color: Colors.red),
                          onPressed: () =>
                              controller.removeTcpListeningRule(ruleId),
                        ),
                      ),
                    );
                  }).toList(),
                ),
        ),
      ],
    );
  }

  void _showAddForwardingRuleDialog() {
    final localHostController = TextEditingController(text: "127.0.0.1");
    final localPortController = TextEditingController();
    final peerIdController = TextEditingController();
    final remotePortController = TextEditingController();

    SmartDialog.show(
      builder: (BuildContext context) {
        return AlertDialog(
          title: Text("Add Port Forwarding Rule"),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  "Forward traffic from a local port to a remote device's port",
                  style: Theme.of(context).textTheme.bodySmall?.apply(
                    color: Theme.of(
                      context,
                    ).colorScheme.onSurface.withAlpha(150),
                  ),
                ),
                SizedBox(height: 16),
                TextField(
                  controller: localHostController,
                  decoration: InputDecoration(
                    labelText: "Local Host",
                    hintText: "127.0.0.1",
                    border: OutlineInputBorder(),
                  ),
                ),
                SizedBox(height: 12),
                TextField(
                  controller: localPortController,
                  decoration: InputDecoration(
                    labelText: "Local Port",
                    hintText: "8080",
                    border: OutlineInputBorder(),
                  ),
                  keyboardType: TextInputType.number,
                ),
                SizedBox(height: 12),
                TextField(
                  controller: peerIdController,
                  decoration: InputDecoration(
                    labelText: "Remote Peer ID",
                    hintText: "12D3KooW...",
                    border: OutlineInputBorder(),
                    suffixIcon: IconButton(
                      icon: const Icon(Icons.devices),
                      tooltip: 'Select from network devices',
                      onPressed: () async {
                        final selectedPeerId = await DeviceSelectorDialog.show(
                          title: 'Select Network Device',
                          dialogId: 'device_selector_forwarding',
                        );
                        if (selectedPeerId != null) {
                          peerIdController.text = selectedPeerId;
                        }
                      },
                    ),
                  ),
                ),
                SizedBox(height: 12),
                TextField(
                  controller: remotePortController,
                  decoration: InputDecoration(
                    labelText: "Remote Port",
                    hintText: "8888",
                    border: OutlineInputBorder(),
                  ),
                  keyboardType: TextInputType.number,
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => SmartDialog.dismiss(),
              child: Text("Cancel"),
            ),
            TextButton(
              onPressed: () async {
                final localHost = localHostController.text.trim();
                final localPort = int.tryParse(localPortController.text.trim());
                final peerId = peerIdController.text.trim();
                final remotePort = int.tryParse(
                  remotePortController.text.trim(),
                );

                if (localHost.isEmpty ||
                    localPort == null ||
                    peerId.isEmpty ||
                    remotePort == null) {
                  Get.snackbar(
                    'Error',
                    'Please fill in all fields with valid values',
                    snackPosition: SnackPosition.BOTTOM,
                    backgroundColor: Colors.red.withValues(alpha: 0.1),
                    colorText: Colors.red,
                  );
                  return;
                }

                SmartDialog.dismiss();
                await controller.addTcpForwardingRule(
                  localHost: localHost,
                  localPort: localPort,
                  peerId: peerId,
                  remotePort: remotePort,
                );
              },
              child: Text("Add"),
            ),
          ],
        );
      },
    );
  }

  void _showAddListeningRuleDialog() {
    final localHostController = TextEditingController(text: "127.0.0.1");
    final localPortController = TextEditingController();
    final allowedPeersController = TextEditingController();

    SmartDialog.show(
      builder: (BuildContext context) {
        return AlertDialog(
          title: Text("Add Port Listening Rule"),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  "Expose a local service to remote devices through P2P tunneling",
                  style: Theme.of(context).textTheme.bodySmall?.apply(
                    color: Theme.of(
                      context,
                    ).colorScheme.onSurface.withAlpha(150),
                  ),
                ),
                SizedBox(height: 16),
                TextField(
                  controller: localHostController,
                  decoration: InputDecoration(
                    labelText: "Local Host",
                    hintText: "127.0.0.1",
                    border: OutlineInputBorder(),
                  ),
                ),
                SizedBox(height: 12),
                TextField(
                  controller: localPortController,
                  decoration: InputDecoration(
                    labelText: "Local Port",
                    hintText: "8888",
                    border: OutlineInputBorder(),
                  ),
                  keyboardType: TextInputType.number,
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => SmartDialog.dismiss(),
              child: Text("Cancel"),
            ),
            TextButton(
              onPressed: () async {
                final localHost = localHostController.text.trim();
                final localPort = int.tryParse(localPortController.text.trim());
                final allowedPeersText = allowedPeersController.text.trim();
                final allowedPeers = allowedPeersText.isEmpty
                    ? <String>[]
                    : allowedPeersText
                          .split(',')
                          .map((e) => e.trim())
                          .where((e) => e.isNotEmpty)
                          .toList();

                if (localHost.isEmpty || localPort == null) {
                  Get.snackbar(
                    'Error',
                    'Please fill in all required fields with valid values',
                    snackPosition: SnackPosition.BOTTOM,
                    backgroundColor: Colors.red.withValues(alpha: 0.1),
                    colorText: Colors.red,
                  );
                  return;
                }

                SmartDialog.dismiss();
                await controller.addTcpListeningRule(
                  localHost: localHost,
                  localPort: localPort,
                  allowedPeers: allowedPeers,
                );
              },
              child: Text("Add"),
            ),
          ],
        );
      },
    );
  }
}
