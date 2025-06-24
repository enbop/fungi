import 'package:flutter/material.dart';
import 'package:get/get.dart';

class AppTheme {
  AppTheme._();

  static final ColorScheme lightColorScheme = const ColorScheme(
    primary: Color(0xFF8B5A2B),
    primaryContainer: Color(0xFFDBC1A0),
    onPrimary: Colors.white,

    secondary: Color(0xFFD2691E),
    secondaryContainer: Color(0xFFFFDBC0),
    onSecondary: Colors.white,

    surface: Color(0xFFF9F5F0),
    onSurface: Color(0xFF33302E),

    error: Color(0xFFB71C1C),
    onError: Colors.white,

    brightness: Brightness.light,

    surfaceTint: Color(0xFF8B5A2B),
    outline: Color(0xFFBEAEA0),
    shadow: Color(0x40000000),

    errorContainer: Color(0xFFFFCDD2),
    onErrorContainer: Color(0xFF5F1D1D),
    onPrimaryContainer: Color(0xFF3E2E16),
    onSecondaryContainer: Color(0xFF4A3114),
    outlineVariant: Color(0xFFD8CFC7),
    scrim: Color(0x99000000),
    inversePrimary: Color(0xFFC18953),
    inverseSurface: Color(0xFF33302E),
    onInverseSurface: Color(0xFFF5F2EE),
    onSurfaceVariant: Color(0xFF7D7069),
  );

  static final ColorScheme darkColorScheme = const ColorScheme(
    primary: Color(0xFFD4A76A),
    primaryContainer: Color(0xFF6B4226),
    onPrimary: Color(0xFF1C1A18),

    secondary: Color(0xFFE07A5F),
    secondaryContainer: Color(0xFF7D3C21),
    onSecondary: Color(0xFF1C1A18),

    surface: Color(0xFF252220),
    onSurface: Color(0xFFECE6DF),

    error: Color(0xFFE57373),
    onError: Color(0xFF1C1A18),

    brightness: Brightness.dark,

    surfaceTint: Color(0xFFD4A76A),
    outline: Color(0xFF8C7B6D),
    shadow: Color(0x40FFFFFF),

    errorContainer: Color(0xFF8B0000),
    onErrorContainer: Color(0xFFFFCDD2),
    onPrimaryContainer: Color(0xFFF3E8D7),
    onSecondaryContainer: Color(0xFFFFE6D9),
    outlineVariant: Color(0xFF6A5F55),
    scrim: Color(0x99000000),
    inversePrimary: Color(0xFF6B4226),
    inverseSurface: Color(0xFFECE6DF),
    onInverseSurface: Color(0xFF252220),
    onSurfaceVariant: Color(0xFFCEC0B3),
  );

  static ThemeData lightTheme = ThemeData(
    useMaterial3: true,
    colorScheme: lightColorScheme,
  );

  static ThemeData darkTheme = ThemeData(
    useMaterial3: true,
    colorScheme: darkColorScheme,
  );
}

class ThemeManager extends GetxController {
  final _isDarkMode = false.obs;

  bool get isDarkMode => _isDarkMode.value;

  void toggleTheme() {
    _isDarkMode.value = !_isDarkMode.value;
    Get.changeThemeMode(_isDarkMode.value ? ThemeMode.dark : ThemeMode.light);
  }

  ColorScheme get colorScheme =>
      _isDarkMode.value ? AppTheme.darkColorScheme : AppTheme.lightColorScheme;
}
