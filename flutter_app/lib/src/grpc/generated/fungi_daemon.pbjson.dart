//
//  Generated code. Do not modify.
//  source: fungi_daemon.proto
//
// @dart = 2.12

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_final_fields
// ignore_for_file: unnecessary_import, unnecessary_this, unused_import

import 'dart:convert' as $convert;
import 'dart:core' as $core;
import 'dart:typed_data' as $typed_data;

@$core.Deprecated('Use emptyDescriptor instead')
const Empty$json = {
  '1': 'Empty',
};

/// Descriptor for `Empty`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List emptyDescriptor = $convert.base64Decode(
    'CgVFbXB0eQ==');

@$core.Deprecated('Use versionResponseDescriptor instead')
const VersionResponse$json = {
  '1': 'VersionResponse',
  '2': [
    {'1': 'version', '3': 1, '4': 1, '5': 9, '10': 'version'},
  ],
};

/// Descriptor for `VersionResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List versionResponseDescriptor = $convert.base64Decode(
    'Cg9WZXJzaW9uUmVzcG9uc2USGAoHdmVyc2lvbhgBIAEoCVIHdmVyc2lvbg==');

@$core.Deprecated('Use hostnameResponseDescriptor instead')
const HostnameResponse$json = {
  '1': 'HostnameResponse',
  '2': [
    {'1': 'hostname', '3': 1, '4': 1, '5': 9, '10': 'hostname'},
  ],
};

/// Descriptor for `HostnameResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List hostnameResponseDescriptor = $convert.base64Decode(
    'ChBIb3N0bmFtZVJlc3BvbnNlEhoKCGhvc3RuYW1lGAEgASgJUghob3N0bmFtZQ==');

@$core.Deprecated('Use startFungiDaemonRequestDescriptor instead')
const StartFungiDaemonRequest$json = {
  '1': 'StartFungiDaemonRequest',
  '2': [
    {'1': 'fungi_dir', '3': 1, '4': 1, '5': 9, '10': 'fungiDir'},
  ],
};

/// Descriptor for `StartFungiDaemonRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List startFungiDaemonRequestDescriptor = $convert.base64Decode(
    'ChdTdGFydEZ1bmdpRGFlbW9uUmVxdWVzdBIbCglmdW5naV9kaXIYASABKAlSCGZ1bmdpRGly');

@$core.Deprecated('Use peerIdResponseDescriptor instead')
const PeerIdResponse$json = {
  '1': 'PeerIdResponse',
  '2': [
    {'1': 'peer_id', '3': 1, '4': 1, '5': 9, '10': 'peerId'},
  ],
};

/// Descriptor for `PeerIdResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List peerIdResponseDescriptor = $convert.base64Decode(
    'Cg5QZWVySWRSZXNwb25zZRIXCgdwZWVyX2lkGAEgASgJUgZwZWVySWQ=');

@$core.Deprecated('Use configFilePathResponseDescriptor instead')
const ConfigFilePathResponse$json = {
  '1': 'ConfigFilePathResponse',
  '2': [
    {'1': 'config_file_path', '3': 1, '4': 1, '5': 9, '10': 'configFilePath'},
  ],
};

/// Descriptor for `ConfigFilePathResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List configFilePathResponseDescriptor = $convert.base64Decode(
    'ChZDb25maWdGaWxlUGF0aFJlc3BvbnNlEigKEGNvbmZpZ19maWxlX3BhdGgYASABKAlSDmNvbm'
    'ZpZ0ZpbGVQYXRo');

@$core.Deprecated('Use incomingAllowedPeersListResponseDescriptor instead')
const IncomingAllowedPeersListResponse$json = {
  '1': 'IncomingAllowedPeersListResponse',
  '2': [
    {'1': 'peers', '3': 1, '4': 3, '5': 11, '6': '.fungi_daemon.PeerInfo', '10': 'peers'},
  ],
};

/// Descriptor for `IncomingAllowedPeersListResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List incomingAllowedPeersListResponseDescriptor = $convert.base64Decode(
    'CiBJbmNvbWluZ0FsbG93ZWRQZWVyc0xpc3RSZXNwb25zZRIsCgVwZWVycxgBIAMoCzIWLmZ1bm'
    'dpX2RhZW1vbi5QZWVySW5mb1IFcGVlcnM=');

@$core.Deprecated('Use addIncomingAllowedPeerRequestDescriptor instead')
const AddIncomingAllowedPeerRequest$json = {
  '1': 'AddIncomingAllowedPeerRequest',
  '2': [
    {'1': 'peer_id', '3': 1, '4': 1, '5': 9, '10': 'peerId'},
  ],
};

/// Descriptor for `AddIncomingAllowedPeerRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List addIncomingAllowedPeerRequestDescriptor = $convert.base64Decode(
    'Ch1BZGRJbmNvbWluZ0FsbG93ZWRQZWVyUmVxdWVzdBIXCgdwZWVyX2lkGAEgASgJUgZwZWVySW'
    'Q=');

@$core.Deprecated('Use removeIncomingAllowedPeerRequestDescriptor instead')
const RemoveIncomingAllowedPeerRequest$json = {
  '1': 'RemoveIncomingAllowedPeerRequest',
  '2': [
    {'1': 'peer_id', '3': 1, '4': 1, '5': 9, '10': 'peerId'},
  ],
};

/// Descriptor for `RemoveIncomingAllowedPeerRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List removeIncomingAllowedPeerRequestDescriptor = $convert.base64Decode(
    'CiBSZW1vdmVJbmNvbWluZ0FsbG93ZWRQZWVyUmVxdWVzdBIXCgdwZWVyX2lkGAEgASgJUgZwZW'
    'VySWQ=');

@$core.Deprecated('Use fileTransferServiceEnabledResponseDescriptor instead')
const FileTransferServiceEnabledResponse$json = {
  '1': 'FileTransferServiceEnabledResponse',
  '2': [
    {'1': 'enabled', '3': 1, '4': 1, '5': 8, '10': 'enabled'},
  ],
};

/// Descriptor for `FileTransferServiceEnabledResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List fileTransferServiceEnabledResponseDescriptor = $convert.base64Decode(
    'CiJGaWxlVHJhbnNmZXJTZXJ2aWNlRW5hYmxlZFJlc3BvbnNlEhgKB2VuYWJsZWQYASABKAhSB2'
    'VuYWJsZWQ=');

@$core.Deprecated('Use fileTransferServiceRootDirResponseDescriptor instead')
const FileTransferServiceRootDirResponse$json = {
  '1': 'FileTransferServiceRootDirResponse',
  '2': [
    {'1': 'root_dir', '3': 1, '4': 1, '5': 9, '10': 'rootDir'},
  ],
};

/// Descriptor for `FileTransferServiceRootDirResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List fileTransferServiceRootDirResponseDescriptor = $convert.base64Decode(
    'CiJGaWxlVHJhbnNmZXJTZXJ2aWNlUm9vdERpclJlc3BvbnNlEhkKCHJvb3RfZGlyGAEgASgJUg'
    'dyb290RGly');

@$core.Deprecated('Use startFileTransferServiceRequestDescriptor instead')
const StartFileTransferServiceRequest$json = {
  '1': 'StartFileTransferServiceRequest',
  '2': [
    {'1': 'root_dir', '3': 1, '4': 1, '5': 9, '10': 'rootDir'},
  ],
};

/// Descriptor for `StartFileTransferServiceRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List startFileTransferServiceRequestDescriptor = $convert.base64Decode(
    'Ch9TdGFydEZpbGVUcmFuc2ZlclNlcnZpY2VSZXF1ZXN0EhkKCHJvb3RfZGlyGAEgASgJUgdyb2'
    '90RGly');

@$core.Deprecated('Use addFileTransferClientRequestDescriptor instead')
const AddFileTransferClientRequest$json = {
  '1': 'AddFileTransferClientRequest',
  '2': [
    {'1': 'enabled', '3': 1, '4': 1, '5': 8, '10': 'enabled'},
    {'1': 'name', '3': 2, '4': 1, '5': 9, '10': 'name'},
    {'1': 'peer_id', '3': 3, '4': 1, '5': 9, '10': 'peerId'},
  ],
};

/// Descriptor for `AddFileTransferClientRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List addFileTransferClientRequestDescriptor = $convert.base64Decode(
    'ChxBZGRGaWxlVHJhbnNmZXJDbGllbnRSZXF1ZXN0EhgKB2VuYWJsZWQYASABKAhSB2VuYWJsZW'
    'QSEgoEbmFtZRgCIAEoCVIEbmFtZRIXCgdwZWVyX2lkGAMgASgJUgZwZWVySWQ=');

@$core.Deprecated('Use removeFileTransferClientRequestDescriptor instead')
const RemoveFileTransferClientRequest$json = {
  '1': 'RemoveFileTransferClientRequest',
  '2': [
    {'1': 'peer_id', '3': 1, '4': 1, '5': 9, '10': 'peerId'},
  ],
};

/// Descriptor for `RemoveFileTransferClientRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List removeFileTransferClientRequestDescriptor = $convert.base64Decode(
    'Ch9SZW1vdmVGaWxlVHJhbnNmZXJDbGllbnRSZXF1ZXN0EhcKB3BlZXJfaWQYASABKAlSBnBlZX'
    'JJZA==');

@$core.Deprecated('Use enableFileTransferClientRequestDescriptor instead')
const EnableFileTransferClientRequest$json = {
  '1': 'EnableFileTransferClientRequest',
  '2': [
    {'1': 'peer_id', '3': 1, '4': 1, '5': 9, '10': 'peerId'},
    {'1': 'enabled', '3': 2, '4': 1, '5': 8, '10': 'enabled'},
  ],
};

/// Descriptor for `EnableFileTransferClientRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List enableFileTransferClientRequestDescriptor = $convert.base64Decode(
    'Ch9FbmFibGVGaWxlVHJhbnNmZXJDbGllbnRSZXF1ZXN0EhcKB3BlZXJfaWQYASABKAlSBnBlZX'
    'JJZBIYCgdlbmFibGVkGAIgASgIUgdlbmFibGVk');

@$core.Deprecated('Use fileTransferClientDescriptor instead')
const FileTransferClient$json = {
  '1': 'FileTransferClient',
  '2': [
    {'1': 'enabled', '3': 1, '4': 1, '5': 8, '10': 'enabled'},
    {'1': 'name', '3': 2, '4': 1, '5': 9, '10': 'name'},
    {'1': 'peer_id', '3': 3, '4': 1, '5': 9, '10': 'peerId'},
  ],
};

/// Descriptor for `FileTransferClient`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List fileTransferClientDescriptor = $convert.base64Decode(
    'ChJGaWxlVHJhbnNmZXJDbGllbnQSGAoHZW5hYmxlZBgBIAEoCFIHZW5hYmxlZBISCgRuYW1lGA'
    'IgASgJUgRuYW1lEhcKB3BlZXJfaWQYAyABKAlSBnBlZXJJZA==');

@$core.Deprecated('Use fileTransferClientsResponseDescriptor instead')
const FileTransferClientsResponse$json = {
  '1': 'FileTransferClientsResponse',
  '2': [
    {'1': 'clients', '3': 1, '4': 3, '5': 11, '6': '.fungi_daemon.FileTransferClient', '10': 'clients'},
  ],
};

/// Descriptor for `FileTransferClientsResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List fileTransferClientsResponseDescriptor = $convert.base64Decode(
    'ChtGaWxlVHJhbnNmZXJDbGllbnRzUmVzcG9uc2USOgoHY2xpZW50cxgBIAMoCzIgLmZ1bmdpX2'
    'RhZW1vbi5GaWxlVHJhbnNmZXJDbGllbnRSB2NsaWVudHM=');

@$core.Deprecated('Use ftpProxyResponseDescriptor instead')
const FtpProxyResponse$json = {
  '1': 'FtpProxyResponse',
  '2': [
    {'1': 'enabled', '3': 1, '4': 1, '5': 8, '10': 'enabled'},
    {'1': 'host', '3': 2, '4': 1, '5': 9, '10': 'host'},
    {'1': 'port', '3': 3, '4': 1, '5': 5, '10': 'port'},
  ],
};

/// Descriptor for `FtpProxyResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List ftpProxyResponseDescriptor = $convert.base64Decode(
    'ChBGdHBQcm94eVJlc3BvbnNlEhgKB2VuYWJsZWQYASABKAhSB2VuYWJsZWQSEgoEaG9zdBgCIA'
    'EoCVIEaG9zdBISCgRwb3J0GAMgASgFUgRwb3J0');

@$core.Deprecated('Use updateFtpProxyRequestDescriptor instead')
const UpdateFtpProxyRequest$json = {
  '1': 'UpdateFtpProxyRequest',
  '2': [
    {'1': 'enabled', '3': 1, '4': 1, '5': 8, '10': 'enabled'},
    {'1': 'host', '3': 2, '4': 1, '5': 9, '10': 'host'},
    {'1': 'port', '3': 3, '4': 1, '5': 5, '10': 'port'},
  ],
};

/// Descriptor for `UpdateFtpProxyRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateFtpProxyRequestDescriptor = $convert.base64Decode(
    'ChVVcGRhdGVGdHBQcm94eVJlcXVlc3QSGAoHZW5hYmxlZBgBIAEoCFIHZW5hYmxlZBISCgRob3'
    'N0GAIgASgJUgRob3N0EhIKBHBvcnQYAyABKAVSBHBvcnQ=');

@$core.Deprecated('Use webdavProxyResponseDescriptor instead')
const WebdavProxyResponse$json = {
  '1': 'WebdavProxyResponse',
  '2': [
    {'1': 'enabled', '3': 1, '4': 1, '5': 8, '10': 'enabled'},
    {'1': 'host', '3': 2, '4': 1, '5': 9, '10': 'host'},
    {'1': 'port', '3': 3, '4': 1, '5': 5, '10': 'port'},
  ],
};

/// Descriptor for `WebdavProxyResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List webdavProxyResponseDescriptor = $convert.base64Decode(
    'ChNXZWJkYXZQcm94eVJlc3BvbnNlEhgKB2VuYWJsZWQYASABKAhSB2VuYWJsZWQSEgoEaG9zdB'
    'gCIAEoCVIEaG9zdBISCgRwb3J0GAMgASgFUgRwb3J0');

@$core.Deprecated('Use updateWebdavProxyRequestDescriptor instead')
const UpdateWebdavProxyRequest$json = {
  '1': 'UpdateWebdavProxyRequest',
  '2': [
    {'1': 'enabled', '3': 1, '4': 1, '5': 8, '10': 'enabled'},
    {'1': 'host', '3': 2, '4': 1, '5': 9, '10': 'host'},
    {'1': 'port', '3': 3, '4': 1, '5': 5, '10': 'port'},
  ],
};

/// Descriptor for `UpdateWebdavProxyRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List updateWebdavProxyRequestDescriptor = $convert.base64Decode(
    'ChhVcGRhdGVXZWJkYXZQcm94eVJlcXVlc3QSGAoHZW5hYmxlZBgBIAEoCFIHZW5hYmxlZBISCg'
    'Rob3N0GAIgASgJUgRob3N0EhIKBHBvcnQYAyABKAVSBHBvcnQ=');

@$core.Deprecated('Use forwardingRuleDescriptor instead')
const ForwardingRule$json = {
  '1': 'ForwardingRule',
  '2': [
    {'1': 'local_host', '3': 1, '4': 1, '5': 9, '10': 'localHost'},
    {'1': 'local_port', '3': 2, '4': 1, '5': 5, '10': 'localPort'},
    {'1': 'remote_peer_id', '3': 3, '4': 1, '5': 9, '10': 'remotePeerId'},
    {'1': 'remote_port', '3': 4, '4': 1, '5': 5, '10': 'remotePort'},
  ],
};

/// Descriptor for `ForwardingRule`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List forwardingRuleDescriptor = $convert.base64Decode(
    'Cg5Gb3J3YXJkaW5nUnVsZRIdCgpsb2NhbF9ob3N0GAEgASgJUglsb2NhbEhvc3QSHQoKbG9jYW'
    'xfcG9ydBgCIAEoBVIJbG9jYWxQb3J0EiQKDnJlbW90ZV9wZWVyX2lkGAMgASgJUgxyZW1vdGVQ'
    'ZWVySWQSHwoLcmVtb3RlX3BvcnQYBCABKAVSCnJlbW90ZVBvcnQ=');

@$core.Deprecated('Use listeningRuleDescriptor instead')
const ListeningRule$json = {
  '1': 'ListeningRule',
  '2': [
    {'1': 'host', '3': 1, '4': 1, '5': 9, '10': 'host'},
    {'1': 'port', '3': 2, '4': 1, '5': 5, '10': 'port'},
    {'1': 'allowed_peers', '3': 3, '4': 3, '5': 9, '10': 'allowedPeers'},
  ],
};

/// Descriptor for `ListeningRule`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List listeningRuleDescriptor = $convert.base64Decode(
    'Cg1MaXN0ZW5pbmdSdWxlEhIKBGhvc3QYASABKAlSBGhvc3QSEgoEcG9ydBgCIAEoBVIEcG9ydB'
    'IjCg1hbGxvd2VkX3BlZXJzGAMgAygJUgxhbGxvd2VkUGVlcnM=');

@$core.Deprecated('Use tcpTunnelingConfigResponseDescriptor instead')
const TcpTunnelingConfigResponse$json = {
  '1': 'TcpTunnelingConfigResponse',
  '2': [
    {'1': 'forwarding_enabled', '3': 1, '4': 1, '5': 8, '10': 'forwardingEnabled'},
    {'1': 'listening_enabled', '3': 2, '4': 1, '5': 8, '10': 'listeningEnabled'},
    {'1': 'forwarding_rules', '3': 3, '4': 3, '5': 11, '6': '.fungi_daemon.ForwardingRule', '10': 'forwardingRules'},
    {'1': 'listening_rules', '3': 4, '4': 3, '5': 11, '6': '.fungi_daemon.ListeningRule', '10': 'listeningRules'},
  ],
};

/// Descriptor for `TcpTunnelingConfigResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List tcpTunnelingConfigResponseDescriptor = $convert.base64Decode(
    'ChpUY3BUdW5uZWxpbmdDb25maWdSZXNwb25zZRItChJmb3J3YXJkaW5nX2VuYWJsZWQYASABKA'
    'hSEWZvcndhcmRpbmdFbmFibGVkEisKEWxpc3RlbmluZ19lbmFibGVkGAIgASgIUhBsaXN0ZW5p'
    'bmdFbmFibGVkEkcKEGZvcndhcmRpbmdfcnVsZXMYAyADKAsyHC5mdW5naV9kYWVtb24uRm9yd2'
    'FyZGluZ1J1bGVSD2ZvcndhcmRpbmdSdWxlcxJECg9saXN0ZW5pbmdfcnVsZXMYBCADKAsyGy5m'
    'dW5naV9kYWVtb24uTGlzdGVuaW5nUnVsZVIObGlzdGVuaW5nUnVsZXM=');

@$core.Deprecated('Use addTcpForwardingRuleRequestDescriptor instead')
const AddTcpForwardingRuleRequest$json = {
  '1': 'AddTcpForwardingRuleRequest',
  '2': [
    {'1': 'local_host', '3': 1, '4': 1, '5': 9, '10': 'localHost'},
    {'1': 'local_port', '3': 2, '4': 1, '5': 5, '10': 'localPort'},
    {'1': 'peer_id', '3': 3, '4': 1, '5': 9, '10': 'peerId'},
    {'1': 'remote_port', '3': 4, '4': 1, '5': 5, '10': 'remotePort'},
  ],
};

/// Descriptor for `AddTcpForwardingRuleRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List addTcpForwardingRuleRequestDescriptor = $convert.base64Decode(
    'ChtBZGRUY3BGb3J3YXJkaW5nUnVsZVJlcXVlc3QSHQoKbG9jYWxfaG9zdBgBIAEoCVIJbG9jYW'
    'xIb3N0Eh0KCmxvY2FsX3BvcnQYAiABKAVSCWxvY2FsUG9ydBIXCgdwZWVyX2lkGAMgASgJUgZw'
    'ZWVySWQSHwoLcmVtb3RlX3BvcnQYBCABKAVSCnJlbW90ZVBvcnQ=');

@$core.Deprecated('Use tcpForwardingRuleResponseDescriptor instead')
const TcpForwardingRuleResponse$json = {
  '1': 'TcpForwardingRuleResponse',
  '2': [
    {'1': 'rule_id', '3': 1, '4': 1, '5': 9, '10': 'ruleId'},
  ],
};

/// Descriptor for `TcpForwardingRuleResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List tcpForwardingRuleResponseDescriptor = $convert.base64Decode(
    'ChlUY3BGb3J3YXJkaW5nUnVsZVJlc3BvbnNlEhcKB3J1bGVfaWQYASABKAlSBnJ1bGVJZA==');

@$core.Deprecated('Use removeTcpForwardingRuleRequestDescriptor instead')
const RemoveTcpForwardingRuleRequest$json = {
  '1': 'RemoveTcpForwardingRuleRequest',
  '2': [
    {'1': 'rule_id', '3': 1, '4': 1, '5': 9, '10': 'ruleId'},
  ],
};

/// Descriptor for `RemoveTcpForwardingRuleRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List removeTcpForwardingRuleRequestDescriptor = $convert.base64Decode(
    'Ch5SZW1vdmVUY3BGb3J3YXJkaW5nUnVsZVJlcXVlc3QSFwoHcnVsZV9pZBgBIAEoCVIGcnVsZU'
    'lk');

@$core.Deprecated('Use addTcpListeningRuleRequestDescriptor instead')
const AddTcpListeningRuleRequest$json = {
  '1': 'AddTcpListeningRuleRequest',
  '2': [
    {'1': 'local_host', '3': 1, '4': 1, '5': 9, '10': 'localHost'},
    {'1': 'local_port', '3': 2, '4': 1, '5': 5, '10': 'localPort'},
    {'1': 'allowed_peers', '3': 3, '4': 3, '5': 9, '10': 'allowedPeers'},
  ],
};

/// Descriptor for `AddTcpListeningRuleRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List addTcpListeningRuleRequestDescriptor = $convert.base64Decode(
    'ChpBZGRUY3BMaXN0ZW5pbmdSdWxlUmVxdWVzdBIdCgpsb2NhbF9ob3N0GAEgASgJUglsb2NhbE'
    'hvc3QSHQoKbG9jYWxfcG9ydBgCIAEoBVIJbG9jYWxQb3J0EiMKDWFsbG93ZWRfcGVlcnMYAyAD'
    'KAlSDGFsbG93ZWRQZWVycw==');

@$core.Deprecated('Use tcpListeningRuleResponseDescriptor instead')
const TcpListeningRuleResponse$json = {
  '1': 'TcpListeningRuleResponse',
  '2': [
    {'1': 'rule_id', '3': 1, '4': 1, '5': 9, '10': 'ruleId'},
  ],
};

/// Descriptor for `TcpListeningRuleResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List tcpListeningRuleResponseDescriptor = $convert.base64Decode(
    'ChhUY3BMaXN0ZW5pbmdSdWxlUmVzcG9uc2USFwoHcnVsZV9pZBgBIAEoCVIGcnVsZUlk');

@$core.Deprecated('Use removeTcpListeningRuleRequestDescriptor instead')
const RemoveTcpListeningRuleRequest$json = {
  '1': 'RemoveTcpListeningRuleRequest',
  '2': [
    {'1': 'rule_id', '3': 1, '4': 1, '5': 9, '10': 'ruleId'},
  ],
};

/// Descriptor for `RemoveTcpListeningRuleRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List removeTcpListeningRuleRequestDescriptor = $convert.base64Decode(
    'Ch1SZW1vdmVUY3BMaXN0ZW5pbmdSdWxlUmVxdWVzdBIXCgdydWxlX2lkGAEgASgJUgZydWxlSW'
    'Q=');

@$core.Deprecated('Use peerInfoDescriptor instead')
const PeerInfo$json = {
  '1': 'PeerInfo',
  '2': [
    {'1': 'peer_id', '3': 1, '4': 1, '5': 9, '10': 'peerId'},
    {'1': 'alias', '3': 2, '4': 1, '5': 9, '10': 'alias'},
    {'1': 'hostname', '3': 3, '4': 1, '5': 9, '10': 'hostname'},
    {'1': 'os', '3': 4, '4': 1, '5': 9, '10': 'os'},
    {'1': 'public_ip', '3': 5, '4': 1, '5': 9, '10': 'publicIp'},
    {'1': 'private_ips', '3': 6, '4': 3, '5': 9, '10': 'privateIps'},
    {'1': 'created_at', '3': 7, '4': 1, '5': 3, '10': 'createdAt'},
    {'1': 'last_connected', '3': 8, '4': 1, '5': 3, '10': 'lastConnected'},
    {'1': 'version', '3': 9, '4': 1, '5': 9, '10': 'version'},
  ],
};

/// Descriptor for `PeerInfo`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List peerInfoDescriptor = $convert.base64Decode(
    'CghQZWVySW5mbxIXCgdwZWVyX2lkGAEgASgJUgZwZWVySWQSFAoFYWxpYXMYAiABKAlSBWFsaW'
    'FzEhoKCGhvc3RuYW1lGAMgASgJUghob3N0bmFtZRIOCgJvcxgEIAEoCVICb3MSGwoJcHVibGlj'
    'X2lwGAUgASgJUghwdWJsaWNJcBIfCgtwcml2YXRlX2lwcxgGIAMoCVIKcHJpdmF0ZUlwcxIdCg'
    'pjcmVhdGVkX2F0GAcgASgDUgljcmVhdGVkQXQSJQoObGFzdF9jb25uZWN0ZWQYCCABKANSDWxh'
    'c3RDb25uZWN0ZWQSGAoHdmVyc2lvbhgJIAEoCVIHdmVyc2lvbg==');

@$core.Deprecated('Use peerInfoListResponseDescriptor instead')
const PeerInfoListResponse$json = {
  '1': 'PeerInfoListResponse',
  '2': [
    {'1': 'peers', '3': 1, '4': 3, '5': 11, '6': '.fungi_daemon.PeerInfo', '10': 'peers'},
  ],
};

/// Descriptor for `PeerInfoListResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List peerInfoListResponseDescriptor = $convert.base64Decode(
    'ChRQZWVySW5mb0xpc3RSZXNwb25zZRIsCgVwZWVycxgBIAMoCzIWLmZ1bmdpX2RhZW1vbi5QZW'
    'VySW5mb1IFcGVlcnM=');

@$core.Deprecated('Use peerInfoResponseDescriptor instead')
const PeerInfoResponse$json = {
  '1': 'PeerInfoResponse',
  '2': [
    {'1': 'peer_info', '3': 1, '4': 1, '5': 11, '6': '.fungi_daemon.PeerInfo', '10': 'peerInfo'},
  ],
};

/// Descriptor for `PeerInfoResponse`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List peerInfoResponseDescriptor = $convert.base64Decode(
    'ChBQZWVySW5mb1Jlc3BvbnNlEjMKCXBlZXJfaW5mbxgBIAEoCzIWLmZ1bmdpX2RhZW1vbi5QZW'
    'VySW5mb1IIcGVlckluZm8=');

@$core.Deprecated('Use addressBookAddOrUpdateRequestDescriptor instead')
const AddressBookAddOrUpdateRequest$json = {
  '1': 'AddressBookAddOrUpdateRequest',
  '2': [
    {'1': 'peer_info', '3': 1, '4': 1, '5': 11, '6': '.fungi_daemon.PeerInfo', '10': 'peerInfo'},
  ],
};

/// Descriptor for `AddressBookAddOrUpdateRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List addressBookAddOrUpdateRequestDescriptor = $convert.base64Decode(
    'Ch1BZGRyZXNzQm9va0FkZE9yVXBkYXRlUmVxdWVzdBIzCglwZWVyX2luZm8YASABKAsyFi5mdW'
    '5naV9kYWVtb24uUGVlckluZm9SCHBlZXJJbmZv');

@$core.Deprecated('Use addressBookGetPeerRequestDescriptor instead')
const AddressBookGetPeerRequest$json = {
  '1': 'AddressBookGetPeerRequest',
  '2': [
    {'1': 'peer_id', '3': 1, '4': 1, '5': 9, '10': 'peerId'},
  ],
};

/// Descriptor for `AddressBookGetPeerRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List addressBookGetPeerRequestDescriptor = $convert.base64Decode(
    'ChlBZGRyZXNzQm9va0dldFBlZXJSZXF1ZXN0EhcKB3BlZXJfaWQYASABKAlSBnBlZXJJZA==');

@$core.Deprecated('Use addressBookRemoveRequestDescriptor instead')
const AddressBookRemoveRequest$json = {
  '1': 'AddressBookRemoveRequest',
  '2': [
    {'1': 'peer_id', '3': 1, '4': 1, '5': 9, '10': 'peerId'},
  ],
};

/// Descriptor for `AddressBookRemoveRequest`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List addressBookRemoveRequestDescriptor = $convert.base64Decode(
    'ChhBZGRyZXNzQm9va1JlbW92ZVJlcXVlc3QSFwoHcGVlcl9pZBgBIAEoCVIGcGVlcklk');

