import 'package:flutter/material.dart';
import 'package:flutter_app/src/rust/api/fungi.dart';
import 'package:flutter_app/src/rust/frb_generated.dart';

Future<void> main() async {
  await RustLib.init();
  await startFungiDaemon();
  debugPrint('Fungi Daemon started');
  String? id = await peerId();
  debugPrint('Peer ID: $id');

  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        appBar: AppBar(title: const Text('flutter_rust_bridge quickstart')),
        body: Center(child: Text('')),
      ),
    );
  }
}
