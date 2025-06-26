import 'package:flutter/material.dart';
import 'package:fungi_app/src/rust/api/fungi.dart';
import 'package:fungi_app/ui/pages/home/drive_page.dart';
import 'package:fungi_app/ui/pages/settings/settings.dart';
import 'package:get/get.dart';
import 'package:fungi_app/app/controllers/fungi_controller.dart';
import 'package:flutter/services.dart';

class TruncatedId extends StatelessWidget {
  final String id;
  final TextStyle? style;
  final VoidCallback? onCopied;

  const TruncatedId({super.key, required this.id, this.style, this.onCopied});

  String get truncatedId {
    if (id.length <= 12) return id;
    return '${id.substring(0, 4)}***${id.substring(id.length - 4)}';
  }

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: 'Click to copy: $id',
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        child: GestureDetector(
          onTap: () => _copyToClipboard(context),
          child: Text(truncatedId, style: style),
        ),
      ),
    );
  }

  void _copyToClipboard(BuildContext context) async {
    await Clipboard.setData(ClipboardData(text: id));
    if (onCopied != null) {
      onCopied!();
    } else {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('ID copied to clipboard'),
            duration: Duration(seconds: 1),
          ),
        );
      }
    }
  }
}

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
          children: [
            Padding(
              padding: const EdgeInsets.all(25),
              child: Image.asset(
                'assets/images/logo.png',
                width: 80,
                height: 80,
                color: colorScheme.primary,
              ),
            ),
            const SizedBox(width: 10),
            Column(
              mainAxisAlignment: MainAxisAlignment.start,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const SizedBox(height: 18),
                LabeledText(
                  label: 'Hostname',
                  labelStyle: TextStyle(
                    fontSize: 15,
                    color: colorScheme.surfaceTint,
                    fontWeight: FontWeight.bold,
                  ),
                  value: hostName() ?? "Unknown",
                  style: const TextStyle(fontSize: 15),
                ),
                const SizedBox(height: 5),
                LabeledText(
                  label: 'Peer ID',
                  labelStyle: TextStyle(
                    fontSize: 15,
                    color: colorScheme.surfaceTint,
                    fontWeight: FontWeight.bold,
                  ),
                  value: controller.peerId.value.substring(0, 5),
                  valueWidget: TruncatedId(
                    id: controller.peerId.value,
                    style: const TextStyle(fontSize: 15),
                  ),
                ),
                const SizedBox(height: 5),
                LabeledText(
                  label: 'Service state',
                  labelStyle: TextStyle(
                    fontSize: 15,
                    color: colorScheme.surfaceTint,
                    fontWeight: FontWeight.bold,
                  ),
                  value: controller.isServiceRunning.value ? "ON" : "OFF",
                  style: TextStyle(
                    fontSize: 15,
                    color: controller.isServiceRunning.value
                        ? Colors.green
                        : Colors.red,
                  ),
                ),
                const SizedBox(height: 20),
                // TextButton(onPressed: () => {}, child: Text('Settings')),
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
                    kMinInteractiveDimension,
                  ),
                  child: AppBar(
                    backgroundColor: colorScheme.primaryContainer,
                    automaticallyImplyLeading: false,
                    bottom: TabBar(
                      tabs: const <Widget>[
                        Tab(text: "File Transfer"),
                        Tab(text: "Data Tunnel"),
                        Tab(text: "Settings"),
                      ],
                      indicatorColor: colorScheme.primary,
                    ),
                  ),
                ),
                body: const TabBarView(
                  children: <Widget>[
                    SingleChildScrollView(child: FileTransferPage()),
                    Center(child: Text("Coming soon...")),
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
