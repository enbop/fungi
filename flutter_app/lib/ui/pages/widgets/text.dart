import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

class TruncatedId extends StatelessWidget {
  final String id;
  final TextStyle? style;
  final VoidCallback? onCopied;

  const TruncatedId({super.key, required this.id, this.style, this.onCopied});

  String get truncatedId {
    if (id.length <= 15) return id;
    return '${id.substring(0, 4)}***${id.substring(id.length - 6)}';
  }

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: 'Click to copy: $id',
      waitDuration: const Duration(milliseconds: 500),
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
