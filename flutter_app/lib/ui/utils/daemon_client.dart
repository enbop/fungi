import 'dart:async';
import 'dart:io';

import 'package:fungi_app/src/grpc/generated/fungi_daemon.pbgrpc.dart';
import 'package:grpc/grpc.dart';
import 'package:logging/logging.dart';

final _logger = Logger('FungiDaemonProcess');

final fungiDaemonProcessManager = FungiDaemonProcessManager();

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

class FungiDaemonProcessManager {
  Process? _process;
  bool _isRunning = false;

  bool get isRunning => _isRunning;

  bool get hasProcess => _process != null;

  Future<bool> start() async {
    if (_isRunning) {
      return true;
    }

    final fungiExecutable = _getFungiExecutablePath();
    final file = File(fungiExecutable);

    if (!await file.exists()) {
      throw FileSystemException('Fungi executable not found', fungiExecutable);
    }

    _process = await Process.start(fungiExecutable, [
      'daemon',
      '--exit-on-stdin-close',
    ], mode: ProcessStartMode.normal);

    _isRunning = true;

    _process!.stdout
        .transform(SystemEncoding().decoder)
        .listen(
          (data) => _logger.fine('[Daemon] $data'),
          onDone: () => _logger.warning('[Daemon] stdout closed'),
        );

    _process!.stderr
        .transform(SystemEncoding().decoder)
        .listen(
          (data) => _logger.fine('[Daemon Error] $data'),
          onDone: () => _logger.warning('[Daemon] stderr closed'),
        );

    _process!.exitCode.then((exitCode) {
      _logger.info('Daemon exited with code: $exitCode');
      _isRunning = false;
      _process = null;
    });

    await Future.delayed(const Duration(seconds: 1));
    return _isRunning;
  }

  Future<void> stop() async {
    if (_process == null) {
      _logger.warning('No daemon process to stop');
      return;
    }

    _logger.info('Stopping daemon...');

    final killed = _process!.kill(ProcessSignal.sigterm);

    if (killed) {
      try {
        await _process!.exitCode.timeout(const Duration(seconds: 5));
        _logger.info('Daemon stopped gracefully');
      } on TimeoutException {
        _logger.warning('Timeout, force killing daemon...');
        _process!.kill(ProcessSignal.sigkill);
      }
    }

    _isRunning = false;
    _process = null;
  }
}

Future<String> readRpcAddress() async {
  final fungiExecutable = _getFungiExecutablePath();
  final file = File(fungiExecutable);

  if (!await file.exists()) {
    throw FileSystemException('Fungi executable not found', fungiExecutable);
  }

  final result = await Process.run(fungiExecutable, ['info', 'rpc-address']);

  if (result.exitCode != 0) {
    throw ProcessException(
      fungiExecutable,
      ['info', 'rpc-address'],
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
  } catch (e) {
    _logger.severe('Failed to connect to daemon: $e');
    _logger.warning('Try to start daemon...');
    if (!await fungiDaemonProcessManager.start()) {
      throw Exception('Failed to start Fungi daemon');
    }
  }

  for (int i = 0; i < 5; i++) {
    try {
      await client.version(Empty());
      _logger.info('Connected to Fungi daemon at $address');
      return client;
    } catch (e) {
      _logger.warning('Attempt ${i + 1} to connect to daemon failed: $e');
      await Future.delayed(const Duration(seconds: 2));
    }
  }

  throw Exception('Failed to connect to Fungi daemon after multiple attempts');
}
