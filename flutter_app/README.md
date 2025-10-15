# fungi_app

A new Flutter project.

## Getting Started

This project is a starting point for a Flutter application.

A few resources to get you started if this is your first Flutter project:

- [Lab: Write your first Flutter app](https://docs.flutter.dev/get-started/codelab)
- [Cookbook: Useful Flutter samples](https://docs.flutter.dev/cookbook)

For help getting started with Flutter development, view the
[online documentation](https://docs.flutter.dev/), which offers tutorials,
samples, guidance on mobile development, and a full API reference.

## Protobuf

Install protoc_plugin
```
dart pub global activate protoc_plugin 21.1.2
```

Update path
```
export PATH="$PATH:$HOME/.pub-cache/bin"
```

Regenerate gRPC code
```
protoc --dart_out=grpc:lib/src/grpc/generated -I../crates/daemon-grpc/proto ../crates/daemon-grpc/proto/fungi_daemon.proto
```