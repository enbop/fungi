import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_smart_dialog/flutter_smart_dialog.dart';

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
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Tooltip(
          message: id,
          waitDuration: const Duration(milliseconds: 500),
          child: Text(truncatedId, style: style),
        ),
        const SizedBox(width: 4),
        Tooltip(
          message: 'Copy',
          waitDuration: const Duration(milliseconds: 500),
          child: MouseRegion(
            cursor: SystemMouseCursors.click,
            child: GestureDetector(
              onTap: () => _copyToClipboard(context),
              child: Icon(
                Icons.copy,
                size: 12,
                color: Theme.of(context).colorScheme.onSurface.withAlpha(150),
              ),
            ),
          ),
        ),
      ],
    );
  }

  void _copyToClipboard(BuildContext context) async {
    await Clipboard.setData(ClipboardData(text: id));
    if (onCopied != null) {
      onCopied!();
    } else {
      if (context.mounted) {
        SmartDialog.showToast('ID copied to clipboard');
      }
    }
  }
}
