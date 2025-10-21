import 'dart:io';

import 'package:fungi_app/src/grpc/generated/fungi_daemon.pbgrpc.dart';
import 'package:fungi_app/ui/utils/android_binary_path.dart';
import 'package:grpc/grpc.dart';
import 'package:logging/logging.dart';
import 'package:path_provider/path_provider.dart';
import 'package:path/path.dart' as p;

final _logger = Logger('DaemonClient');

String _getFungiExecutablePath() {
  final executablePath = Platform.resolvedExecutable;

  if (Platform.isMacOS) {
    return '${Directory(executablePath).parent.parent.path}/Resources/fungi';
  } else if (Platform.isWindows) {
    return '${Directory(executablePath).parent.path}\\fungi.exe';
  } else if (Platform.isLinux) {
    return '${Directory(executablePath).parent.path}/fungi';
  } else {
    throw UnsupportedError('Unsupported platform: ${Platform.operatingSystem}');
  }
}

Future<String> readRpcAddress() async {
  String fungiExecutable;
  List<String> args = ['info', 'rpc-address'];

  if (Platform.isAndroid) {
    final appDocumentsDir = await getApplicationDocumentsDirectory();
    final fungiDir = p.join(appDocumentsDir.absolute.path, 'fungi');

    fungiExecutable = await getAndroidFungiBinaryPath() ?? '';
    if (fungiExecutable.isEmpty) {
      throw FileSystemException('Fungi executable not found on Android');
    }

    // --default-device-name no need here
    args = ['--default-device-name', 'null', '--fungi-dir', fungiDir, ...args];
  } else {
    fungiExecutable = _getFungiExecutablePath();
    final file = File(fungiExecutable);

    if (!await file.exists()) {
      throw FileSystemException('Fungi executable not found', fungiExecutable);
    }
  }

  final result = await Process.run(fungiExecutable, args);

  if (result.exitCode != 0) {
    throw ProcessException(
      fungiExecutable,
      args,
      'Process exited with code ${result.exitCode}\nstderr: ${result.stderr}',
    );
  }

  return result.stdout.toString().trim();
}

FungiDaemonClient _createClient(String address) {
  final parts = address.split(':');
  if (parts.length != 2) {
    throw FormatException('Invalid RPC address format: $address');
  }

  final host = parts[0];
  final port = int.tryParse(parts[1]);

  if (port == null) {
    throw FormatException('Invalid port number in RPC address: $address');
  }

  final channel = ClientChannel(
    host,
    port: port,
    options: const ChannelOptions(
      credentials: ChannelCredentials.insecure(),
      connectTimeout: Duration(seconds: 3),
    ),
  );

  return FungiDaemonClient(channel);
}

FungiDaemonClient fungiDaemonClientPlaceholder() {
  return FungiDaemonClient(
    ClientChannel(
      'localhost',
      port: 0,
      options: const ChannelOptions(credentials: ChannelCredentials.insecure()),
    ),
  );
}

Future<FungiDaemonClient> getFungiClient() async {
  final address = await readRpcAddress();
  _logger.info('Connecting to Fungi daemon at $address');
  final client = _createClient(address);

  try {
    await client.version(Empty());
    _logger.info('Connected to Fungi daemon at $address');
    return client;
  } catch (e) {
    _logger.severe('Failed to connect to daemon: $e');
    throw Exception('Daemon not available. Please ensure daemon is running.');
  }
}
