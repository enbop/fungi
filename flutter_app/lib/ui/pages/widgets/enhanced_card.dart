import 'package:flutter/material.dart';

class EnhancedCard extends StatelessWidget {
  final Widget child;
  final Color? accentColor;
  final double elevation;
  final double borderRadius;
  final double borderWidth;
  final double gradientOpacity;

  const EnhancedCard({
    super.key,
    required this.child,
    this.accentColor,
    this.elevation = 2,
    this.borderRadius = 8,
    this.borderWidth = 1,
    this.gradientOpacity = 0.08,
  });

  @override
  Widget build(BuildContext context) {
    final Color effectiveAccentColor =
        accentColor ?? Theme.of(context).colorScheme.primary;

    return Card(
      elevation: elevation,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(borderRadius),
        side: BorderSide(
          color: effectiveAccentColor.withOpacity(0.3),
          width: borderWidth,
        ),
      ),
      child: Container(
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(borderRadius),
          gradient: LinearGradient(
            begin: Alignment.topLeft,
            end: Alignment.bottomRight,
            colors: [
              effectiveAccentColor.withOpacity(gradientOpacity),
              Theme.of(context).colorScheme.surface,
            ],
          ),
        ),
        child: child,
      ),
    );
  }
}
