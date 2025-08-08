import 'package:flutter/material.dart';
import 'package:fungi_app/src/rust/api/fungi.dart';
import 'package:fungi_app/ui/pages/home/drive_page.dart';
import 'package:fungi_app/ui/pages/home/data_tunnel_page.dart';
import 'package:fungi_app/ui/pages/settings/settings.dart';
import 'package:fungi_app/ui/widgets/text.dart';
import 'package:get/get.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';

class LabeledText extends StatelessWidget {
  final String label;
  final String value;
  final Widget? valueWidget;
  final String separator;
  final TextStyle? labelStyle;
  final TextStyle? style;
  final double labelWidth;

  const LabeledText({
    super.key,
    required this.label,
    required this.value,
    this.valueWidget,
    this.separator = ":  ",
    this.labelStyle,
    this.style,
    this.labelWidth = 120,
  });

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Container(
          width: labelWidth,
          alignment: Alignment.centerLeft,
          child: Text(label, style: labelStyle ?? style),
        ),
        Text(separator, style: labelStyle ?? style),
        Flexible(
          child:
              valueWidget ??
              Text(
                value,
                style: style,
                softWrap: true,
                overflow: TextOverflow.ellipsis,
              ),
        ),
      ],
    );
  }
}

class HomeHeader extends GetView<FungiController> {
  const HomeHeader({super.key});

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;

    return Container(
      decoration: BoxDecoration(color: colorScheme.primaryContainer),
      child: Obx(
        () => Row(
          mainAxisAlignment: MainAxisAlignment.spaceAround,
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 5, horizontal: 20),
              child: Image.asset(
                'assets/images/logo.png',
                width: 50,
                height: 50,
                color: colorScheme.primary,
              ),
            ),
            Column(
              mainAxisAlignment: MainAxisAlignment.start,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                LabeledText(
                  label: 'Hostname',
                  labelStyle: TextStyle(
                    fontSize: 12,
                    color: colorScheme.surfaceTint,
                    fontWeight: FontWeight.bold,
                  ),
                  value: hostName() ?? "Unknown",
                  style: const TextStyle(fontSize: 12),
                ),
                LabeledText(
                  label: 'Peer ID',
                  labelStyle: TextStyle(
                    fontSize: 12,
                    color: colorScheme.surfaceTint,
                    fontWeight: FontWeight.bold,
                  ),
                  value: controller.peerId.value.substring(0, 5),
                  valueWidget: TruncatedId(
                    id: controller.peerId.value,
                    style: const TextStyle(fontSize: 12),
                  ),
                ),
                LabeledText(
                  label: 'Service state',
                  labelStyle: TextStyle(
                    fontSize: 12,
                    color: colorScheme.surfaceTint,
                    fontWeight: FontWeight.bold,
                  ),
                  value: controller.isServiceRunning.value ? "ON" : "OFF",
                  style: TextStyle(
                    fontSize: 12,
                    color: controller.isServiceRunning.value
                        ? Colors.green
                        : Colors.red,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class HomePage extends StatelessWidget {
  const HomePage({super.key});

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;

    return Scaffold(
      body: Column(
        children: [
          const HomeHeader(),
          Expanded(
            child: DefaultTabController(
              initialIndex: 0,
              length: 3,
              child: Scaffold(
                backgroundColor: Colors.transparent,
                appBar: PreferredSize(
                  preferredSize: const Size.fromHeight(
                    kMinInteractiveDimension - 15,
                  ),
                  child: AppBar(
                    backgroundColor: colorScheme.primaryContainer,
                    automaticallyImplyLeading: false,
                    bottom: TabBar(
                      tabs: const <Widget>[
                        Tab(text: "File Transfer", height: 30),
                        Tab(text: "Data Tunnel", height: 30),
                        Tab(text: "Settings", height: 30),
                      ],
                      indicatorColor: colorScheme.primary,
                    ),
                  ),
                ),
                body: const TabBarView(
                  children: <Widget>[
                    SingleChildScrollView(child: FileTransferPage()),
                    SingleChildScrollView(child: DataTunnelPage()),
                    Settings(),
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
