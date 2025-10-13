// try connect to daemon or run fungi daemon
// TODO:
// 1. read config get grpc address
// 2. check grpc server is running (try connect and ping)
// 3. if not running, run fungi daemon process
// 4. connect to grpc server again

// * add a state to show if current daemon is a child process

import 'dart:io';

Future<String> readRpcAddress() async {
  final executablePath = Platform.resolvedExecutable;

  var fungiExecutable = "";

  if (Platform.isMacOS) {
    fungiExecutable =
        '${Directory(executablePath).parent.parent.path}/Resources/fungi';
  } else {
    throw UnsupportedError('Unsupported platform: ${Platform.operatingSystem}');
  }

  final file = File(fungiExecutable);
  if (!await file.exists()) {
    throw FileSystemException('Fungi executable not found', fungiExecutable);
  }

  final result = await Process.run(fungiExecutable, ['rpc-address']);

  if (result.exitCode != 0) {
    throw ProcessException(
      fungiExecutable,
      ['rpc-address'],
      'Process exited with code ${result.exitCode}\nstderr: ${result.stderr}',
    );
  }

  return result.stdout.toString().trim();
}
