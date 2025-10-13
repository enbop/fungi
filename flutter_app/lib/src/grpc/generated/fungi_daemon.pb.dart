//
//  Generated code. Do not modify.
//  source: fungi_daemon.proto
//
// @dart = 2.12

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_final_fields
// ignore_for_file: unnecessary_import, unnecessary_this, unused_import

import 'dart:core' as $core;

import 'package:fixnum/fixnum.dart' as $fixnum;
import 'package:protobuf/protobuf.dart' as $pb;

class Empty extends $pb.GeneratedMessage {
  factory Empty() => create();
  Empty._() : super();
  factory Empty.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory Empty.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'Empty', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  Empty clone() => Empty()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  Empty copyWith(void Function(Empty) updates) => super.copyWith((message) => updates(message as Empty)) as Empty;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static Empty create() => Empty._();
  Empty createEmptyInstance() => create();
  static $pb.PbList<Empty> createRepeated() => $pb.PbList<Empty>();
  @$core.pragma('dart2js:noInline')
  static Empty getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<Empty>(create);
  static Empty? _defaultInstance;
}

class HostnameResponse extends $pb.GeneratedMessage {
  factory HostnameResponse({
    $core.String? hostname,
  }) {
    final $result = create();
    if (hostname != null) {
      $result.hostname = hostname;
    }
    return $result;
  }
  HostnameResponse._() : super();
  factory HostnameResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory HostnameResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'HostnameResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'hostname')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  HostnameResponse clone() => HostnameResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  HostnameResponse copyWith(void Function(HostnameResponse) updates) => super.copyWith((message) => updates(message as HostnameResponse)) as HostnameResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static HostnameResponse create() => HostnameResponse._();
  HostnameResponse createEmptyInstance() => create();
  static $pb.PbList<HostnameResponse> createRepeated() => $pb.PbList<HostnameResponse>();
  @$core.pragma('dart2js:noInline')
  static HostnameResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<HostnameResponse>(create);
  static HostnameResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get hostname => $_getSZ(0);
  @$pb.TagNumber(1)
  set hostname($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasHostname() => $_has(0);
  @$pb.TagNumber(1)
  void clearHostname() => clearField(1);
}

class StartFungiDaemonRequest extends $pb.GeneratedMessage {
  factory StartFungiDaemonRequest({
    $core.String? fungiDir,
  }) {
    final $result = create();
    if (fungiDir != null) {
      $result.fungiDir = fungiDir;
    }
    return $result;
  }
  StartFungiDaemonRequest._() : super();
  factory StartFungiDaemonRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory StartFungiDaemonRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'StartFungiDaemonRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'fungiDir')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  StartFungiDaemonRequest clone() => StartFungiDaemonRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  StartFungiDaemonRequest copyWith(void Function(StartFungiDaemonRequest) updates) => super.copyWith((message) => updates(message as StartFungiDaemonRequest)) as StartFungiDaemonRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static StartFungiDaemonRequest create() => StartFungiDaemonRequest._();
  StartFungiDaemonRequest createEmptyInstance() => create();
  static $pb.PbList<StartFungiDaemonRequest> createRepeated() => $pb.PbList<StartFungiDaemonRequest>();
  @$core.pragma('dart2js:noInline')
  static StartFungiDaemonRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<StartFungiDaemonRequest>(create);
  static StartFungiDaemonRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get fungiDir => $_getSZ(0);
  @$pb.TagNumber(1)
  set fungiDir($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasFungiDir() => $_has(0);
  @$pb.TagNumber(1)
  void clearFungiDir() => clearField(1);
}

class PeerIdResponse extends $pb.GeneratedMessage {
  factory PeerIdResponse({
    $core.String? peerId,
  }) {
    final $result = create();
    if (peerId != null) {
      $result.peerId = peerId;
    }
    return $result;
  }
  PeerIdResponse._() : super();
  factory PeerIdResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory PeerIdResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'PeerIdResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'peerId')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  PeerIdResponse clone() => PeerIdResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  PeerIdResponse copyWith(void Function(PeerIdResponse) updates) => super.copyWith((message) => updates(message as PeerIdResponse)) as PeerIdResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static PeerIdResponse create() => PeerIdResponse._();
  PeerIdResponse createEmptyInstance() => create();
  static $pb.PbList<PeerIdResponse> createRepeated() => $pb.PbList<PeerIdResponse>();
  @$core.pragma('dart2js:noInline')
  static PeerIdResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<PeerIdResponse>(create);
  static PeerIdResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get peerId => $_getSZ(0);
  @$pb.TagNumber(1)
  set peerId($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasPeerId() => $_has(0);
  @$pb.TagNumber(1)
  void clearPeerId() => clearField(1);
}

class ConfigFilePathResponse extends $pb.GeneratedMessage {
  factory ConfigFilePathResponse({
    $core.String? configFilePath,
  }) {
    final $result = create();
    if (configFilePath != null) {
      $result.configFilePath = configFilePath;
    }
    return $result;
  }
  ConfigFilePathResponse._() : super();
  factory ConfigFilePathResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory ConfigFilePathResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'ConfigFilePathResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'configFilePath')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  ConfigFilePathResponse clone() => ConfigFilePathResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  ConfigFilePathResponse copyWith(void Function(ConfigFilePathResponse) updates) => super.copyWith((message) => updates(message as ConfigFilePathResponse)) as ConfigFilePathResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static ConfigFilePathResponse create() => ConfigFilePathResponse._();
  ConfigFilePathResponse createEmptyInstance() => create();
  static $pb.PbList<ConfigFilePathResponse> createRepeated() => $pb.PbList<ConfigFilePathResponse>();
  @$core.pragma('dart2js:noInline')
  static ConfigFilePathResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ConfigFilePathResponse>(create);
  static ConfigFilePathResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get configFilePath => $_getSZ(0);
  @$pb.TagNumber(1)
  set configFilePath($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasConfigFilePath() => $_has(0);
  @$pb.TagNumber(1)
  void clearConfigFilePath() => clearField(1);
}

class IncomingAllowedPeersListResponse extends $pb.GeneratedMessage {
  factory IncomingAllowedPeersListResponse({
    $core.Iterable<PeerInfo>? peers,
  }) {
    final $result = create();
    if (peers != null) {
      $result.peers.addAll(peers);
    }
    return $result;
  }
  IncomingAllowedPeersListResponse._() : super();
  factory IncomingAllowedPeersListResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory IncomingAllowedPeersListResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'IncomingAllowedPeersListResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..pc<PeerInfo>(1, _omitFieldNames ? '' : 'peers', $pb.PbFieldType.PM, subBuilder: PeerInfo.create)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  IncomingAllowedPeersListResponse clone() => IncomingAllowedPeersListResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  IncomingAllowedPeersListResponse copyWith(void Function(IncomingAllowedPeersListResponse) updates) => super.copyWith((message) => updates(message as IncomingAllowedPeersListResponse)) as IncomingAllowedPeersListResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static IncomingAllowedPeersListResponse create() => IncomingAllowedPeersListResponse._();
  IncomingAllowedPeersListResponse createEmptyInstance() => create();
  static $pb.PbList<IncomingAllowedPeersListResponse> createRepeated() => $pb.PbList<IncomingAllowedPeersListResponse>();
  @$core.pragma('dart2js:noInline')
  static IncomingAllowedPeersListResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<IncomingAllowedPeersListResponse>(create);
  static IncomingAllowedPeersListResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.List<PeerInfo> get peers => $_getList(0);
}

class AddIncomingAllowedPeerRequest extends $pb.GeneratedMessage {
  factory AddIncomingAllowedPeerRequest({
    $core.String? peerId,
  }) {
    final $result = create();
    if (peerId != null) {
      $result.peerId = peerId;
    }
    return $result;
  }
  AddIncomingAllowedPeerRequest._() : super();
  factory AddIncomingAllowedPeerRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory AddIncomingAllowedPeerRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'AddIncomingAllowedPeerRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'peerId')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  AddIncomingAllowedPeerRequest clone() => AddIncomingAllowedPeerRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  AddIncomingAllowedPeerRequest copyWith(void Function(AddIncomingAllowedPeerRequest) updates) => super.copyWith((message) => updates(message as AddIncomingAllowedPeerRequest)) as AddIncomingAllowedPeerRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static AddIncomingAllowedPeerRequest create() => AddIncomingAllowedPeerRequest._();
  AddIncomingAllowedPeerRequest createEmptyInstance() => create();
  static $pb.PbList<AddIncomingAllowedPeerRequest> createRepeated() => $pb.PbList<AddIncomingAllowedPeerRequest>();
  @$core.pragma('dart2js:noInline')
  static AddIncomingAllowedPeerRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddIncomingAllowedPeerRequest>(create);
  static AddIncomingAllowedPeerRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get peerId => $_getSZ(0);
  @$pb.TagNumber(1)
  set peerId($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasPeerId() => $_has(0);
  @$pb.TagNumber(1)
  void clearPeerId() => clearField(1);
}

class RemoveIncomingAllowedPeerRequest extends $pb.GeneratedMessage {
  factory RemoveIncomingAllowedPeerRequest({
    $core.String? peerId,
  }) {
    final $result = create();
    if (peerId != null) {
      $result.peerId = peerId;
    }
    return $result;
  }
  RemoveIncomingAllowedPeerRequest._() : super();
  factory RemoveIncomingAllowedPeerRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory RemoveIncomingAllowedPeerRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'RemoveIncomingAllowedPeerRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'peerId')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  RemoveIncomingAllowedPeerRequest clone() => RemoveIncomingAllowedPeerRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  RemoveIncomingAllowedPeerRequest copyWith(void Function(RemoveIncomingAllowedPeerRequest) updates) => super.copyWith((message) => updates(message as RemoveIncomingAllowedPeerRequest)) as RemoveIncomingAllowedPeerRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static RemoveIncomingAllowedPeerRequest create() => RemoveIncomingAllowedPeerRequest._();
  RemoveIncomingAllowedPeerRequest createEmptyInstance() => create();
  static $pb.PbList<RemoveIncomingAllowedPeerRequest> createRepeated() => $pb.PbList<RemoveIncomingAllowedPeerRequest>();
  @$core.pragma('dart2js:noInline')
  static RemoveIncomingAllowedPeerRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<RemoveIncomingAllowedPeerRequest>(create);
  static RemoveIncomingAllowedPeerRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get peerId => $_getSZ(0);
  @$pb.TagNumber(1)
  set peerId($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasPeerId() => $_has(0);
  @$pb.TagNumber(1)
  void clearPeerId() => clearField(1);
}

class FileTransferServiceEnabledResponse extends $pb.GeneratedMessage {
  factory FileTransferServiceEnabledResponse({
    $core.bool? enabled,
  }) {
    final $result = create();
    if (enabled != null) {
      $result.enabled = enabled;
    }
    return $result;
  }
  FileTransferServiceEnabledResponse._() : super();
  factory FileTransferServiceEnabledResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory FileTransferServiceEnabledResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'FileTransferServiceEnabledResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOB(1, _omitFieldNames ? '' : 'enabled')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  FileTransferServiceEnabledResponse clone() => FileTransferServiceEnabledResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  FileTransferServiceEnabledResponse copyWith(void Function(FileTransferServiceEnabledResponse) updates) => super.copyWith((message) => updates(message as FileTransferServiceEnabledResponse)) as FileTransferServiceEnabledResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static FileTransferServiceEnabledResponse create() => FileTransferServiceEnabledResponse._();
  FileTransferServiceEnabledResponse createEmptyInstance() => create();
  static $pb.PbList<FileTransferServiceEnabledResponse> createRepeated() => $pb.PbList<FileTransferServiceEnabledResponse>();
  @$core.pragma('dart2js:noInline')
  static FileTransferServiceEnabledResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<FileTransferServiceEnabledResponse>(create);
  static FileTransferServiceEnabledResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get enabled => $_getBF(0);
  @$pb.TagNumber(1)
  set enabled($core.bool v) { $_setBool(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasEnabled() => $_has(0);
  @$pb.TagNumber(1)
  void clearEnabled() => clearField(1);
}

class FileTransferServiceRootDirResponse extends $pb.GeneratedMessage {
  factory FileTransferServiceRootDirResponse({
    $core.String? rootDir,
  }) {
    final $result = create();
    if (rootDir != null) {
      $result.rootDir = rootDir;
    }
    return $result;
  }
  FileTransferServiceRootDirResponse._() : super();
  factory FileTransferServiceRootDirResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory FileTransferServiceRootDirResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'FileTransferServiceRootDirResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'rootDir')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  FileTransferServiceRootDirResponse clone() => FileTransferServiceRootDirResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  FileTransferServiceRootDirResponse copyWith(void Function(FileTransferServiceRootDirResponse) updates) => super.copyWith((message) => updates(message as FileTransferServiceRootDirResponse)) as FileTransferServiceRootDirResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static FileTransferServiceRootDirResponse create() => FileTransferServiceRootDirResponse._();
  FileTransferServiceRootDirResponse createEmptyInstance() => create();
  static $pb.PbList<FileTransferServiceRootDirResponse> createRepeated() => $pb.PbList<FileTransferServiceRootDirResponse>();
  @$core.pragma('dart2js:noInline')
  static FileTransferServiceRootDirResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<FileTransferServiceRootDirResponse>(create);
  static FileTransferServiceRootDirResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get rootDir => $_getSZ(0);
  @$pb.TagNumber(1)
  set rootDir($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasRootDir() => $_has(0);
  @$pb.TagNumber(1)
  void clearRootDir() => clearField(1);
}

class StartFileTransferServiceRequest extends $pb.GeneratedMessage {
  factory StartFileTransferServiceRequest({
    $core.String? rootDir,
  }) {
    final $result = create();
    if (rootDir != null) {
      $result.rootDir = rootDir;
    }
    return $result;
  }
  StartFileTransferServiceRequest._() : super();
  factory StartFileTransferServiceRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory StartFileTransferServiceRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'StartFileTransferServiceRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'rootDir')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  StartFileTransferServiceRequest clone() => StartFileTransferServiceRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  StartFileTransferServiceRequest copyWith(void Function(StartFileTransferServiceRequest) updates) => super.copyWith((message) => updates(message as StartFileTransferServiceRequest)) as StartFileTransferServiceRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static StartFileTransferServiceRequest create() => StartFileTransferServiceRequest._();
  StartFileTransferServiceRequest createEmptyInstance() => create();
  static $pb.PbList<StartFileTransferServiceRequest> createRepeated() => $pb.PbList<StartFileTransferServiceRequest>();
  @$core.pragma('dart2js:noInline')
  static StartFileTransferServiceRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<StartFileTransferServiceRequest>(create);
  static StartFileTransferServiceRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get rootDir => $_getSZ(0);
  @$pb.TagNumber(1)
  set rootDir($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasRootDir() => $_has(0);
  @$pb.TagNumber(1)
  void clearRootDir() => clearField(1);
}

class AddFileTransferClientRequest extends $pb.GeneratedMessage {
  factory AddFileTransferClientRequest({
    $core.bool? enabled,
    $core.String? name,
    $core.String? peerId,
  }) {
    final $result = create();
    if (enabled != null) {
      $result.enabled = enabled;
    }
    if (name != null) {
      $result.name = name;
    }
    if (peerId != null) {
      $result.peerId = peerId;
    }
    return $result;
  }
  AddFileTransferClientRequest._() : super();
  factory AddFileTransferClientRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory AddFileTransferClientRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'AddFileTransferClientRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOB(1, _omitFieldNames ? '' : 'enabled')
    ..aOS(2, _omitFieldNames ? '' : 'name')
    ..aOS(3, _omitFieldNames ? '' : 'peerId')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  AddFileTransferClientRequest clone() => AddFileTransferClientRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  AddFileTransferClientRequest copyWith(void Function(AddFileTransferClientRequest) updates) => super.copyWith((message) => updates(message as AddFileTransferClientRequest)) as AddFileTransferClientRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static AddFileTransferClientRequest create() => AddFileTransferClientRequest._();
  AddFileTransferClientRequest createEmptyInstance() => create();
  static $pb.PbList<AddFileTransferClientRequest> createRepeated() => $pb.PbList<AddFileTransferClientRequest>();
  @$core.pragma('dart2js:noInline')
  static AddFileTransferClientRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddFileTransferClientRequest>(create);
  static AddFileTransferClientRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get enabled => $_getBF(0);
  @$pb.TagNumber(1)
  set enabled($core.bool v) { $_setBool(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasEnabled() => $_has(0);
  @$pb.TagNumber(1)
  void clearEnabled() => clearField(1);

  @$pb.TagNumber(2)
  $core.String get name => $_getSZ(1);
  @$pb.TagNumber(2)
  set name($core.String v) { $_setString(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasName() => $_has(1);
  @$pb.TagNumber(2)
  void clearName() => clearField(2);

  @$pb.TagNumber(3)
  $core.String get peerId => $_getSZ(2);
  @$pb.TagNumber(3)
  set peerId($core.String v) { $_setString(2, v); }
  @$pb.TagNumber(3)
  $core.bool hasPeerId() => $_has(2);
  @$pb.TagNumber(3)
  void clearPeerId() => clearField(3);
}

class RemoveFileTransferClientRequest extends $pb.GeneratedMessage {
  factory RemoveFileTransferClientRequest({
    $core.String? peerId,
  }) {
    final $result = create();
    if (peerId != null) {
      $result.peerId = peerId;
    }
    return $result;
  }
  RemoveFileTransferClientRequest._() : super();
  factory RemoveFileTransferClientRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory RemoveFileTransferClientRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'RemoveFileTransferClientRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'peerId')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  RemoveFileTransferClientRequest clone() => RemoveFileTransferClientRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  RemoveFileTransferClientRequest copyWith(void Function(RemoveFileTransferClientRequest) updates) => super.copyWith((message) => updates(message as RemoveFileTransferClientRequest)) as RemoveFileTransferClientRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static RemoveFileTransferClientRequest create() => RemoveFileTransferClientRequest._();
  RemoveFileTransferClientRequest createEmptyInstance() => create();
  static $pb.PbList<RemoveFileTransferClientRequest> createRepeated() => $pb.PbList<RemoveFileTransferClientRequest>();
  @$core.pragma('dart2js:noInline')
  static RemoveFileTransferClientRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<RemoveFileTransferClientRequest>(create);
  static RemoveFileTransferClientRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get peerId => $_getSZ(0);
  @$pb.TagNumber(1)
  set peerId($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasPeerId() => $_has(0);
  @$pb.TagNumber(1)
  void clearPeerId() => clearField(1);
}

class EnableFileTransferClientRequest extends $pb.GeneratedMessage {
  factory EnableFileTransferClientRequest({
    $core.String? peerId,
    $core.bool? enabled,
  }) {
    final $result = create();
    if (peerId != null) {
      $result.peerId = peerId;
    }
    if (enabled != null) {
      $result.enabled = enabled;
    }
    return $result;
  }
  EnableFileTransferClientRequest._() : super();
  factory EnableFileTransferClientRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory EnableFileTransferClientRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'EnableFileTransferClientRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'peerId')
    ..aOB(2, _omitFieldNames ? '' : 'enabled')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  EnableFileTransferClientRequest clone() => EnableFileTransferClientRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  EnableFileTransferClientRequest copyWith(void Function(EnableFileTransferClientRequest) updates) => super.copyWith((message) => updates(message as EnableFileTransferClientRequest)) as EnableFileTransferClientRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static EnableFileTransferClientRequest create() => EnableFileTransferClientRequest._();
  EnableFileTransferClientRequest createEmptyInstance() => create();
  static $pb.PbList<EnableFileTransferClientRequest> createRepeated() => $pb.PbList<EnableFileTransferClientRequest>();
  @$core.pragma('dart2js:noInline')
  static EnableFileTransferClientRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<EnableFileTransferClientRequest>(create);
  static EnableFileTransferClientRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get peerId => $_getSZ(0);
  @$pb.TagNumber(1)
  set peerId($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasPeerId() => $_has(0);
  @$pb.TagNumber(1)
  void clearPeerId() => clearField(1);

  @$pb.TagNumber(2)
  $core.bool get enabled => $_getBF(1);
  @$pb.TagNumber(2)
  set enabled($core.bool v) { $_setBool(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasEnabled() => $_has(1);
  @$pb.TagNumber(2)
  void clearEnabled() => clearField(2);
}

class FileTransferClient extends $pb.GeneratedMessage {
  factory FileTransferClient({
    $core.bool? enabled,
    $core.String? name,
    $core.String? peerId,
  }) {
    final $result = create();
    if (enabled != null) {
      $result.enabled = enabled;
    }
    if (name != null) {
      $result.name = name;
    }
    if (peerId != null) {
      $result.peerId = peerId;
    }
    return $result;
  }
  FileTransferClient._() : super();
  factory FileTransferClient.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory FileTransferClient.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'FileTransferClient', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOB(1, _omitFieldNames ? '' : 'enabled')
    ..aOS(2, _omitFieldNames ? '' : 'name')
    ..aOS(3, _omitFieldNames ? '' : 'peerId')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  FileTransferClient clone() => FileTransferClient()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  FileTransferClient copyWith(void Function(FileTransferClient) updates) => super.copyWith((message) => updates(message as FileTransferClient)) as FileTransferClient;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static FileTransferClient create() => FileTransferClient._();
  FileTransferClient createEmptyInstance() => create();
  static $pb.PbList<FileTransferClient> createRepeated() => $pb.PbList<FileTransferClient>();
  @$core.pragma('dart2js:noInline')
  static FileTransferClient getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<FileTransferClient>(create);
  static FileTransferClient? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get enabled => $_getBF(0);
  @$pb.TagNumber(1)
  set enabled($core.bool v) { $_setBool(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasEnabled() => $_has(0);
  @$pb.TagNumber(1)
  void clearEnabled() => clearField(1);

  @$pb.TagNumber(2)
  $core.String get name => $_getSZ(1);
  @$pb.TagNumber(2)
  set name($core.String v) { $_setString(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasName() => $_has(1);
  @$pb.TagNumber(2)
  void clearName() => clearField(2);

  @$pb.TagNumber(3)
  $core.String get peerId => $_getSZ(2);
  @$pb.TagNumber(3)
  set peerId($core.String v) { $_setString(2, v); }
  @$pb.TagNumber(3)
  $core.bool hasPeerId() => $_has(2);
  @$pb.TagNumber(3)
  void clearPeerId() => clearField(3);
}

class FileTransferClientsResponse extends $pb.GeneratedMessage {
  factory FileTransferClientsResponse({
    $core.Iterable<FileTransferClient>? clients,
  }) {
    final $result = create();
    if (clients != null) {
      $result.clients.addAll(clients);
    }
    return $result;
  }
  FileTransferClientsResponse._() : super();
  factory FileTransferClientsResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory FileTransferClientsResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'FileTransferClientsResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..pc<FileTransferClient>(1, _omitFieldNames ? '' : 'clients', $pb.PbFieldType.PM, subBuilder: FileTransferClient.create)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  FileTransferClientsResponse clone() => FileTransferClientsResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  FileTransferClientsResponse copyWith(void Function(FileTransferClientsResponse) updates) => super.copyWith((message) => updates(message as FileTransferClientsResponse)) as FileTransferClientsResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static FileTransferClientsResponse create() => FileTransferClientsResponse._();
  FileTransferClientsResponse createEmptyInstance() => create();
  static $pb.PbList<FileTransferClientsResponse> createRepeated() => $pb.PbList<FileTransferClientsResponse>();
  @$core.pragma('dart2js:noInline')
  static FileTransferClientsResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<FileTransferClientsResponse>(create);
  static FileTransferClientsResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.List<FileTransferClient> get clients => $_getList(0);
}

class FtpProxyResponse extends $pb.GeneratedMessage {
  factory FtpProxyResponse({
    $core.bool? enabled,
    $core.String? host,
    $core.int? port,
  }) {
    final $result = create();
    if (enabled != null) {
      $result.enabled = enabled;
    }
    if (host != null) {
      $result.host = host;
    }
    if (port != null) {
      $result.port = port;
    }
    return $result;
  }
  FtpProxyResponse._() : super();
  factory FtpProxyResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory FtpProxyResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'FtpProxyResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOB(1, _omitFieldNames ? '' : 'enabled')
    ..aOS(2, _omitFieldNames ? '' : 'host')
    ..a<$core.int>(3, _omitFieldNames ? '' : 'port', $pb.PbFieldType.O3)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  FtpProxyResponse clone() => FtpProxyResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  FtpProxyResponse copyWith(void Function(FtpProxyResponse) updates) => super.copyWith((message) => updates(message as FtpProxyResponse)) as FtpProxyResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static FtpProxyResponse create() => FtpProxyResponse._();
  FtpProxyResponse createEmptyInstance() => create();
  static $pb.PbList<FtpProxyResponse> createRepeated() => $pb.PbList<FtpProxyResponse>();
  @$core.pragma('dart2js:noInline')
  static FtpProxyResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<FtpProxyResponse>(create);
  static FtpProxyResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get enabled => $_getBF(0);
  @$pb.TagNumber(1)
  set enabled($core.bool v) { $_setBool(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasEnabled() => $_has(0);
  @$pb.TagNumber(1)
  void clearEnabled() => clearField(1);

  @$pb.TagNumber(2)
  $core.String get host => $_getSZ(1);
  @$pb.TagNumber(2)
  set host($core.String v) { $_setString(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasHost() => $_has(1);
  @$pb.TagNumber(2)
  void clearHost() => clearField(2);

  @$pb.TagNumber(3)
  $core.int get port => $_getIZ(2);
  @$pb.TagNumber(3)
  set port($core.int v) { $_setSignedInt32(2, v); }
  @$pb.TagNumber(3)
  $core.bool hasPort() => $_has(2);
  @$pb.TagNumber(3)
  void clearPort() => clearField(3);
}

class UpdateFtpProxyRequest extends $pb.GeneratedMessage {
  factory UpdateFtpProxyRequest({
    $core.bool? enabled,
    $core.String? host,
    $core.int? port,
  }) {
    final $result = create();
    if (enabled != null) {
      $result.enabled = enabled;
    }
    if (host != null) {
      $result.host = host;
    }
    if (port != null) {
      $result.port = port;
    }
    return $result;
  }
  UpdateFtpProxyRequest._() : super();
  factory UpdateFtpProxyRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory UpdateFtpProxyRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'UpdateFtpProxyRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOB(1, _omitFieldNames ? '' : 'enabled')
    ..aOS(2, _omitFieldNames ? '' : 'host')
    ..a<$core.int>(3, _omitFieldNames ? '' : 'port', $pb.PbFieldType.O3)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  UpdateFtpProxyRequest clone() => UpdateFtpProxyRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  UpdateFtpProxyRequest copyWith(void Function(UpdateFtpProxyRequest) updates) => super.copyWith((message) => updates(message as UpdateFtpProxyRequest)) as UpdateFtpProxyRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static UpdateFtpProxyRequest create() => UpdateFtpProxyRequest._();
  UpdateFtpProxyRequest createEmptyInstance() => create();
  static $pb.PbList<UpdateFtpProxyRequest> createRepeated() => $pb.PbList<UpdateFtpProxyRequest>();
  @$core.pragma('dart2js:noInline')
  static UpdateFtpProxyRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<UpdateFtpProxyRequest>(create);
  static UpdateFtpProxyRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get enabled => $_getBF(0);
  @$pb.TagNumber(1)
  set enabled($core.bool v) { $_setBool(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasEnabled() => $_has(0);
  @$pb.TagNumber(1)
  void clearEnabled() => clearField(1);

  @$pb.TagNumber(2)
  $core.String get host => $_getSZ(1);
  @$pb.TagNumber(2)
  set host($core.String v) { $_setString(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasHost() => $_has(1);
  @$pb.TagNumber(2)
  void clearHost() => clearField(2);

  @$pb.TagNumber(3)
  $core.int get port => $_getIZ(2);
  @$pb.TagNumber(3)
  set port($core.int v) { $_setSignedInt32(2, v); }
  @$pb.TagNumber(3)
  $core.bool hasPort() => $_has(2);
  @$pb.TagNumber(3)
  void clearPort() => clearField(3);
}

class WebdavProxyResponse extends $pb.GeneratedMessage {
  factory WebdavProxyResponse({
    $core.bool? enabled,
    $core.String? host,
    $core.int? port,
  }) {
    final $result = create();
    if (enabled != null) {
      $result.enabled = enabled;
    }
    if (host != null) {
      $result.host = host;
    }
    if (port != null) {
      $result.port = port;
    }
    return $result;
  }
  WebdavProxyResponse._() : super();
  factory WebdavProxyResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory WebdavProxyResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'WebdavProxyResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOB(1, _omitFieldNames ? '' : 'enabled')
    ..aOS(2, _omitFieldNames ? '' : 'host')
    ..a<$core.int>(3, _omitFieldNames ? '' : 'port', $pb.PbFieldType.O3)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  WebdavProxyResponse clone() => WebdavProxyResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  WebdavProxyResponse copyWith(void Function(WebdavProxyResponse) updates) => super.copyWith((message) => updates(message as WebdavProxyResponse)) as WebdavProxyResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static WebdavProxyResponse create() => WebdavProxyResponse._();
  WebdavProxyResponse createEmptyInstance() => create();
  static $pb.PbList<WebdavProxyResponse> createRepeated() => $pb.PbList<WebdavProxyResponse>();
  @$core.pragma('dart2js:noInline')
  static WebdavProxyResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<WebdavProxyResponse>(create);
  static WebdavProxyResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get enabled => $_getBF(0);
  @$pb.TagNumber(1)
  set enabled($core.bool v) { $_setBool(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasEnabled() => $_has(0);
  @$pb.TagNumber(1)
  void clearEnabled() => clearField(1);

  @$pb.TagNumber(2)
  $core.String get host => $_getSZ(1);
  @$pb.TagNumber(2)
  set host($core.String v) { $_setString(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasHost() => $_has(1);
  @$pb.TagNumber(2)
  void clearHost() => clearField(2);

  @$pb.TagNumber(3)
  $core.int get port => $_getIZ(2);
  @$pb.TagNumber(3)
  set port($core.int v) { $_setSignedInt32(2, v); }
  @$pb.TagNumber(3)
  $core.bool hasPort() => $_has(2);
  @$pb.TagNumber(3)
  void clearPort() => clearField(3);
}

class UpdateWebdavProxyRequest extends $pb.GeneratedMessage {
  factory UpdateWebdavProxyRequest({
    $core.bool? enabled,
    $core.String? host,
    $core.int? port,
  }) {
    final $result = create();
    if (enabled != null) {
      $result.enabled = enabled;
    }
    if (host != null) {
      $result.host = host;
    }
    if (port != null) {
      $result.port = port;
    }
    return $result;
  }
  UpdateWebdavProxyRequest._() : super();
  factory UpdateWebdavProxyRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory UpdateWebdavProxyRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'UpdateWebdavProxyRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOB(1, _omitFieldNames ? '' : 'enabled')
    ..aOS(2, _omitFieldNames ? '' : 'host')
    ..a<$core.int>(3, _omitFieldNames ? '' : 'port', $pb.PbFieldType.O3)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  UpdateWebdavProxyRequest clone() => UpdateWebdavProxyRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  UpdateWebdavProxyRequest copyWith(void Function(UpdateWebdavProxyRequest) updates) => super.copyWith((message) => updates(message as UpdateWebdavProxyRequest)) as UpdateWebdavProxyRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static UpdateWebdavProxyRequest create() => UpdateWebdavProxyRequest._();
  UpdateWebdavProxyRequest createEmptyInstance() => create();
  static $pb.PbList<UpdateWebdavProxyRequest> createRepeated() => $pb.PbList<UpdateWebdavProxyRequest>();
  @$core.pragma('dart2js:noInline')
  static UpdateWebdavProxyRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<UpdateWebdavProxyRequest>(create);
  static UpdateWebdavProxyRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get enabled => $_getBF(0);
  @$pb.TagNumber(1)
  set enabled($core.bool v) { $_setBool(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasEnabled() => $_has(0);
  @$pb.TagNumber(1)
  void clearEnabled() => clearField(1);

  @$pb.TagNumber(2)
  $core.String get host => $_getSZ(1);
  @$pb.TagNumber(2)
  set host($core.String v) { $_setString(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasHost() => $_has(1);
  @$pb.TagNumber(2)
  void clearHost() => clearField(2);

  @$pb.TagNumber(3)
  $core.int get port => $_getIZ(2);
  @$pb.TagNumber(3)
  set port($core.int v) { $_setSignedInt32(2, v); }
  @$pb.TagNumber(3)
  $core.bool hasPort() => $_has(2);
  @$pb.TagNumber(3)
  void clearPort() => clearField(3);
}

class ForwardingRule extends $pb.GeneratedMessage {
  factory ForwardingRule({
    $core.String? localHost,
    $core.int? localPort,
    $core.String? remotePeerId,
    $core.int? remotePort,
  }) {
    final $result = create();
    if (localHost != null) {
      $result.localHost = localHost;
    }
    if (localPort != null) {
      $result.localPort = localPort;
    }
    if (remotePeerId != null) {
      $result.remotePeerId = remotePeerId;
    }
    if (remotePort != null) {
      $result.remotePort = remotePort;
    }
    return $result;
  }
  ForwardingRule._() : super();
  factory ForwardingRule.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory ForwardingRule.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'ForwardingRule', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'localHost')
    ..a<$core.int>(2, _omitFieldNames ? '' : 'localPort', $pb.PbFieldType.O3)
    ..aOS(3, _omitFieldNames ? '' : 'remotePeerId')
    ..a<$core.int>(4, _omitFieldNames ? '' : 'remotePort', $pb.PbFieldType.O3)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  ForwardingRule clone() => ForwardingRule()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  ForwardingRule copyWith(void Function(ForwardingRule) updates) => super.copyWith((message) => updates(message as ForwardingRule)) as ForwardingRule;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static ForwardingRule create() => ForwardingRule._();
  ForwardingRule createEmptyInstance() => create();
  static $pb.PbList<ForwardingRule> createRepeated() => $pb.PbList<ForwardingRule>();
  @$core.pragma('dart2js:noInline')
  static ForwardingRule getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ForwardingRule>(create);
  static ForwardingRule? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get localHost => $_getSZ(0);
  @$pb.TagNumber(1)
  set localHost($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasLocalHost() => $_has(0);
  @$pb.TagNumber(1)
  void clearLocalHost() => clearField(1);

  @$pb.TagNumber(2)
  $core.int get localPort => $_getIZ(1);
  @$pb.TagNumber(2)
  set localPort($core.int v) { $_setSignedInt32(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasLocalPort() => $_has(1);
  @$pb.TagNumber(2)
  void clearLocalPort() => clearField(2);

  @$pb.TagNumber(3)
  $core.String get remotePeerId => $_getSZ(2);
  @$pb.TagNumber(3)
  set remotePeerId($core.String v) { $_setString(2, v); }
  @$pb.TagNumber(3)
  $core.bool hasRemotePeerId() => $_has(2);
  @$pb.TagNumber(3)
  void clearRemotePeerId() => clearField(3);

  @$pb.TagNumber(4)
  $core.int get remotePort => $_getIZ(3);
  @$pb.TagNumber(4)
  set remotePort($core.int v) { $_setSignedInt32(3, v); }
  @$pb.TagNumber(4)
  $core.bool hasRemotePort() => $_has(3);
  @$pb.TagNumber(4)
  void clearRemotePort() => clearField(4);
}

class ListeningRule extends $pb.GeneratedMessage {
  factory ListeningRule({
    $core.String? host,
    $core.int? port,
    $core.Iterable<$core.String>? allowedPeers,
  }) {
    final $result = create();
    if (host != null) {
      $result.host = host;
    }
    if (port != null) {
      $result.port = port;
    }
    if (allowedPeers != null) {
      $result.allowedPeers.addAll(allowedPeers);
    }
    return $result;
  }
  ListeningRule._() : super();
  factory ListeningRule.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory ListeningRule.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'ListeningRule', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'host')
    ..a<$core.int>(2, _omitFieldNames ? '' : 'port', $pb.PbFieldType.O3)
    ..pPS(3, _omitFieldNames ? '' : 'allowedPeers')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  ListeningRule clone() => ListeningRule()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  ListeningRule copyWith(void Function(ListeningRule) updates) => super.copyWith((message) => updates(message as ListeningRule)) as ListeningRule;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static ListeningRule create() => ListeningRule._();
  ListeningRule createEmptyInstance() => create();
  static $pb.PbList<ListeningRule> createRepeated() => $pb.PbList<ListeningRule>();
  @$core.pragma('dart2js:noInline')
  static ListeningRule getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<ListeningRule>(create);
  static ListeningRule? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get host => $_getSZ(0);
  @$pb.TagNumber(1)
  set host($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasHost() => $_has(0);
  @$pb.TagNumber(1)
  void clearHost() => clearField(1);

  @$pb.TagNumber(2)
  $core.int get port => $_getIZ(1);
  @$pb.TagNumber(2)
  set port($core.int v) { $_setSignedInt32(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasPort() => $_has(1);
  @$pb.TagNumber(2)
  void clearPort() => clearField(2);

  @$pb.TagNumber(3)
  $core.List<$core.String> get allowedPeers => $_getList(2);
}

class TcpTunnelingConfigResponse extends $pb.GeneratedMessage {
  factory TcpTunnelingConfigResponse({
    $core.bool? forwardingEnabled,
    $core.bool? listeningEnabled,
    $core.Iterable<ForwardingRule>? forwardingRules,
    $core.Iterable<ListeningRule>? listeningRules,
  }) {
    final $result = create();
    if (forwardingEnabled != null) {
      $result.forwardingEnabled = forwardingEnabled;
    }
    if (listeningEnabled != null) {
      $result.listeningEnabled = listeningEnabled;
    }
    if (forwardingRules != null) {
      $result.forwardingRules.addAll(forwardingRules);
    }
    if (listeningRules != null) {
      $result.listeningRules.addAll(listeningRules);
    }
    return $result;
  }
  TcpTunnelingConfigResponse._() : super();
  factory TcpTunnelingConfigResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory TcpTunnelingConfigResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'TcpTunnelingConfigResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOB(1, _omitFieldNames ? '' : 'forwardingEnabled')
    ..aOB(2, _omitFieldNames ? '' : 'listeningEnabled')
    ..pc<ForwardingRule>(3, _omitFieldNames ? '' : 'forwardingRules', $pb.PbFieldType.PM, subBuilder: ForwardingRule.create)
    ..pc<ListeningRule>(4, _omitFieldNames ? '' : 'listeningRules', $pb.PbFieldType.PM, subBuilder: ListeningRule.create)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  TcpTunnelingConfigResponse clone() => TcpTunnelingConfigResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  TcpTunnelingConfigResponse copyWith(void Function(TcpTunnelingConfigResponse) updates) => super.copyWith((message) => updates(message as TcpTunnelingConfigResponse)) as TcpTunnelingConfigResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static TcpTunnelingConfigResponse create() => TcpTunnelingConfigResponse._();
  TcpTunnelingConfigResponse createEmptyInstance() => create();
  static $pb.PbList<TcpTunnelingConfigResponse> createRepeated() => $pb.PbList<TcpTunnelingConfigResponse>();
  @$core.pragma('dart2js:noInline')
  static TcpTunnelingConfigResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<TcpTunnelingConfigResponse>(create);
  static TcpTunnelingConfigResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.bool get forwardingEnabled => $_getBF(0);
  @$pb.TagNumber(1)
  set forwardingEnabled($core.bool v) { $_setBool(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasForwardingEnabled() => $_has(0);
  @$pb.TagNumber(1)
  void clearForwardingEnabled() => clearField(1);

  @$pb.TagNumber(2)
  $core.bool get listeningEnabled => $_getBF(1);
  @$pb.TagNumber(2)
  set listeningEnabled($core.bool v) { $_setBool(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasListeningEnabled() => $_has(1);
  @$pb.TagNumber(2)
  void clearListeningEnabled() => clearField(2);

  @$pb.TagNumber(3)
  $core.List<ForwardingRule> get forwardingRules => $_getList(2);

  @$pb.TagNumber(4)
  $core.List<ListeningRule> get listeningRules => $_getList(3);
}

class AddTcpForwardingRuleRequest extends $pb.GeneratedMessage {
  factory AddTcpForwardingRuleRequest({
    $core.String? localHost,
    $core.int? localPort,
    $core.String? peerId,
    $core.int? remotePort,
  }) {
    final $result = create();
    if (localHost != null) {
      $result.localHost = localHost;
    }
    if (localPort != null) {
      $result.localPort = localPort;
    }
    if (peerId != null) {
      $result.peerId = peerId;
    }
    if (remotePort != null) {
      $result.remotePort = remotePort;
    }
    return $result;
  }
  AddTcpForwardingRuleRequest._() : super();
  factory AddTcpForwardingRuleRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory AddTcpForwardingRuleRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'AddTcpForwardingRuleRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'localHost')
    ..a<$core.int>(2, _omitFieldNames ? '' : 'localPort', $pb.PbFieldType.O3)
    ..aOS(3, _omitFieldNames ? '' : 'peerId')
    ..a<$core.int>(4, _omitFieldNames ? '' : 'remotePort', $pb.PbFieldType.O3)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  AddTcpForwardingRuleRequest clone() => AddTcpForwardingRuleRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  AddTcpForwardingRuleRequest copyWith(void Function(AddTcpForwardingRuleRequest) updates) => super.copyWith((message) => updates(message as AddTcpForwardingRuleRequest)) as AddTcpForwardingRuleRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static AddTcpForwardingRuleRequest create() => AddTcpForwardingRuleRequest._();
  AddTcpForwardingRuleRequest createEmptyInstance() => create();
  static $pb.PbList<AddTcpForwardingRuleRequest> createRepeated() => $pb.PbList<AddTcpForwardingRuleRequest>();
  @$core.pragma('dart2js:noInline')
  static AddTcpForwardingRuleRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddTcpForwardingRuleRequest>(create);
  static AddTcpForwardingRuleRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get localHost => $_getSZ(0);
  @$pb.TagNumber(1)
  set localHost($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasLocalHost() => $_has(0);
  @$pb.TagNumber(1)
  void clearLocalHost() => clearField(1);

  @$pb.TagNumber(2)
  $core.int get localPort => $_getIZ(1);
  @$pb.TagNumber(2)
  set localPort($core.int v) { $_setSignedInt32(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasLocalPort() => $_has(1);
  @$pb.TagNumber(2)
  void clearLocalPort() => clearField(2);

  @$pb.TagNumber(3)
  $core.String get peerId => $_getSZ(2);
  @$pb.TagNumber(3)
  set peerId($core.String v) { $_setString(2, v); }
  @$pb.TagNumber(3)
  $core.bool hasPeerId() => $_has(2);
  @$pb.TagNumber(3)
  void clearPeerId() => clearField(3);

  @$pb.TagNumber(4)
  $core.int get remotePort => $_getIZ(3);
  @$pb.TagNumber(4)
  set remotePort($core.int v) { $_setSignedInt32(3, v); }
  @$pb.TagNumber(4)
  $core.bool hasRemotePort() => $_has(3);
  @$pb.TagNumber(4)
  void clearRemotePort() => clearField(4);
}

class TcpForwardingRuleResponse extends $pb.GeneratedMessage {
  factory TcpForwardingRuleResponse({
    $core.String? ruleId,
  }) {
    final $result = create();
    if (ruleId != null) {
      $result.ruleId = ruleId;
    }
    return $result;
  }
  TcpForwardingRuleResponse._() : super();
  factory TcpForwardingRuleResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory TcpForwardingRuleResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'TcpForwardingRuleResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'ruleId')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  TcpForwardingRuleResponse clone() => TcpForwardingRuleResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  TcpForwardingRuleResponse copyWith(void Function(TcpForwardingRuleResponse) updates) => super.copyWith((message) => updates(message as TcpForwardingRuleResponse)) as TcpForwardingRuleResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static TcpForwardingRuleResponse create() => TcpForwardingRuleResponse._();
  TcpForwardingRuleResponse createEmptyInstance() => create();
  static $pb.PbList<TcpForwardingRuleResponse> createRepeated() => $pb.PbList<TcpForwardingRuleResponse>();
  @$core.pragma('dart2js:noInline')
  static TcpForwardingRuleResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<TcpForwardingRuleResponse>(create);
  static TcpForwardingRuleResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get ruleId => $_getSZ(0);
  @$pb.TagNumber(1)
  set ruleId($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasRuleId() => $_has(0);
  @$pb.TagNumber(1)
  void clearRuleId() => clearField(1);
}

class RemoveTcpForwardingRuleRequest extends $pb.GeneratedMessage {
  factory RemoveTcpForwardingRuleRequest({
    $core.String? ruleId,
  }) {
    final $result = create();
    if (ruleId != null) {
      $result.ruleId = ruleId;
    }
    return $result;
  }
  RemoveTcpForwardingRuleRequest._() : super();
  factory RemoveTcpForwardingRuleRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory RemoveTcpForwardingRuleRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'RemoveTcpForwardingRuleRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'ruleId')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  RemoveTcpForwardingRuleRequest clone() => RemoveTcpForwardingRuleRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  RemoveTcpForwardingRuleRequest copyWith(void Function(RemoveTcpForwardingRuleRequest) updates) => super.copyWith((message) => updates(message as RemoveTcpForwardingRuleRequest)) as RemoveTcpForwardingRuleRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static RemoveTcpForwardingRuleRequest create() => RemoveTcpForwardingRuleRequest._();
  RemoveTcpForwardingRuleRequest createEmptyInstance() => create();
  static $pb.PbList<RemoveTcpForwardingRuleRequest> createRepeated() => $pb.PbList<RemoveTcpForwardingRuleRequest>();
  @$core.pragma('dart2js:noInline')
  static RemoveTcpForwardingRuleRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<RemoveTcpForwardingRuleRequest>(create);
  static RemoveTcpForwardingRuleRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get ruleId => $_getSZ(0);
  @$pb.TagNumber(1)
  set ruleId($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasRuleId() => $_has(0);
  @$pb.TagNumber(1)
  void clearRuleId() => clearField(1);
}

class AddTcpListeningRuleRequest extends $pb.GeneratedMessage {
  factory AddTcpListeningRuleRequest({
    $core.String? localHost,
    $core.int? localPort,
    $core.Iterable<$core.String>? allowedPeers,
  }) {
    final $result = create();
    if (localHost != null) {
      $result.localHost = localHost;
    }
    if (localPort != null) {
      $result.localPort = localPort;
    }
    if (allowedPeers != null) {
      $result.allowedPeers.addAll(allowedPeers);
    }
    return $result;
  }
  AddTcpListeningRuleRequest._() : super();
  factory AddTcpListeningRuleRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory AddTcpListeningRuleRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'AddTcpListeningRuleRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'localHost')
    ..a<$core.int>(2, _omitFieldNames ? '' : 'localPort', $pb.PbFieldType.O3)
    ..pPS(3, _omitFieldNames ? '' : 'allowedPeers')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  AddTcpListeningRuleRequest clone() => AddTcpListeningRuleRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  AddTcpListeningRuleRequest copyWith(void Function(AddTcpListeningRuleRequest) updates) => super.copyWith((message) => updates(message as AddTcpListeningRuleRequest)) as AddTcpListeningRuleRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static AddTcpListeningRuleRequest create() => AddTcpListeningRuleRequest._();
  AddTcpListeningRuleRequest createEmptyInstance() => create();
  static $pb.PbList<AddTcpListeningRuleRequest> createRepeated() => $pb.PbList<AddTcpListeningRuleRequest>();
  @$core.pragma('dart2js:noInline')
  static AddTcpListeningRuleRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddTcpListeningRuleRequest>(create);
  static AddTcpListeningRuleRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get localHost => $_getSZ(0);
  @$pb.TagNumber(1)
  set localHost($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasLocalHost() => $_has(0);
  @$pb.TagNumber(1)
  void clearLocalHost() => clearField(1);

  @$pb.TagNumber(2)
  $core.int get localPort => $_getIZ(1);
  @$pb.TagNumber(2)
  set localPort($core.int v) { $_setSignedInt32(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasLocalPort() => $_has(1);
  @$pb.TagNumber(2)
  void clearLocalPort() => clearField(2);

  @$pb.TagNumber(3)
  $core.List<$core.String> get allowedPeers => $_getList(2);
}

class TcpListeningRuleResponse extends $pb.GeneratedMessage {
  factory TcpListeningRuleResponse({
    $core.String? ruleId,
  }) {
    final $result = create();
    if (ruleId != null) {
      $result.ruleId = ruleId;
    }
    return $result;
  }
  TcpListeningRuleResponse._() : super();
  factory TcpListeningRuleResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory TcpListeningRuleResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'TcpListeningRuleResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'ruleId')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  TcpListeningRuleResponse clone() => TcpListeningRuleResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  TcpListeningRuleResponse copyWith(void Function(TcpListeningRuleResponse) updates) => super.copyWith((message) => updates(message as TcpListeningRuleResponse)) as TcpListeningRuleResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static TcpListeningRuleResponse create() => TcpListeningRuleResponse._();
  TcpListeningRuleResponse createEmptyInstance() => create();
  static $pb.PbList<TcpListeningRuleResponse> createRepeated() => $pb.PbList<TcpListeningRuleResponse>();
  @$core.pragma('dart2js:noInline')
  static TcpListeningRuleResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<TcpListeningRuleResponse>(create);
  static TcpListeningRuleResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get ruleId => $_getSZ(0);
  @$pb.TagNumber(1)
  set ruleId($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasRuleId() => $_has(0);
  @$pb.TagNumber(1)
  void clearRuleId() => clearField(1);
}

class RemoveTcpListeningRuleRequest extends $pb.GeneratedMessage {
  factory RemoveTcpListeningRuleRequest({
    $core.String? ruleId,
  }) {
    final $result = create();
    if (ruleId != null) {
      $result.ruleId = ruleId;
    }
    return $result;
  }
  RemoveTcpListeningRuleRequest._() : super();
  factory RemoveTcpListeningRuleRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory RemoveTcpListeningRuleRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'RemoveTcpListeningRuleRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'ruleId')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  RemoveTcpListeningRuleRequest clone() => RemoveTcpListeningRuleRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  RemoveTcpListeningRuleRequest copyWith(void Function(RemoveTcpListeningRuleRequest) updates) => super.copyWith((message) => updates(message as RemoveTcpListeningRuleRequest)) as RemoveTcpListeningRuleRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static RemoveTcpListeningRuleRequest create() => RemoveTcpListeningRuleRequest._();
  RemoveTcpListeningRuleRequest createEmptyInstance() => create();
  static $pb.PbList<RemoveTcpListeningRuleRequest> createRepeated() => $pb.PbList<RemoveTcpListeningRuleRequest>();
  @$core.pragma('dart2js:noInline')
  static RemoveTcpListeningRuleRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<RemoveTcpListeningRuleRequest>(create);
  static RemoveTcpListeningRuleRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get ruleId => $_getSZ(0);
  @$pb.TagNumber(1)
  set ruleId($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasRuleId() => $_has(0);
  @$pb.TagNumber(1)
  void clearRuleId() => clearField(1);
}

class PeerInfo extends $pb.GeneratedMessage {
  factory PeerInfo({
    $core.String? peerId,
    $core.String? alias,
    $core.String? hostname,
    $core.String? os,
    $core.String? publicIp,
    $core.Iterable<$core.String>? privateIps,
    $fixnum.Int64? createdAt,
    $fixnum.Int64? lastConnected,
    $core.String? version,
  }) {
    final $result = create();
    if (peerId != null) {
      $result.peerId = peerId;
    }
    if (alias != null) {
      $result.alias = alias;
    }
    if (hostname != null) {
      $result.hostname = hostname;
    }
    if (os != null) {
      $result.os = os;
    }
    if (publicIp != null) {
      $result.publicIp = publicIp;
    }
    if (privateIps != null) {
      $result.privateIps.addAll(privateIps);
    }
    if (createdAt != null) {
      $result.createdAt = createdAt;
    }
    if (lastConnected != null) {
      $result.lastConnected = lastConnected;
    }
    if (version != null) {
      $result.version = version;
    }
    return $result;
  }
  PeerInfo._() : super();
  factory PeerInfo.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory PeerInfo.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'PeerInfo', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'peerId')
    ..aOS(2, _omitFieldNames ? '' : 'alias')
    ..aOS(3, _omitFieldNames ? '' : 'hostname')
    ..aOS(4, _omitFieldNames ? '' : 'os')
    ..aOS(5, _omitFieldNames ? '' : 'publicIp')
    ..pPS(6, _omitFieldNames ? '' : 'privateIps')
    ..aInt64(7, _omitFieldNames ? '' : 'createdAt')
    ..aInt64(8, _omitFieldNames ? '' : 'lastConnected')
    ..aOS(9, _omitFieldNames ? '' : 'version')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  PeerInfo clone() => PeerInfo()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  PeerInfo copyWith(void Function(PeerInfo) updates) => super.copyWith((message) => updates(message as PeerInfo)) as PeerInfo;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static PeerInfo create() => PeerInfo._();
  PeerInfo createEmptyInstance() => create();
  static $pb.PbList<PeerInfo> createRepeated() => $pb.PbList<PeerInfo>();
  @$core.pragma('dart2js:noInline')
  static PeerInfo getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<PeerInfo>(create);
  static PeerInfo? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get peerId => $_getSZ(0);
  @$pb.TagNumber(1)
  set peerId($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasPeerId() => $_has(0);
  @$pb.TagNumber(1)
  void clearPeerId() => clearField(1);

  @$pb.TagNumber(2)
  $core.String get alias => $_getSZ(1);
  @$pb.TagNumber(2)
  set alias($core.String v) { $_setString(1, v); }
  @$pb.TagNumber(2)
  $core.bool hasAlias() => $_has(1);
  @$pb.TagNumber(2)
  void clearAlias() => clearField(2);

  @$pb.TagNumber(3)
  $core.String get hostname => $_getSZ(2);
  @$pb.TagNumber(3)
  set hostname($core.String v) { $_setString(2, v); }
  @$pb.TagNumber(3)
  $core.bool hasHostname() => $_has(2);
  @$pb.TagNumber(3)
  void clearHostname() => clearField(3);

  @$pb.TagNumber(4)
  $core.String get os => $_getSZ(3);
  @$pb.TagNumber(4)
  set os($core.String v) { $_setString(3, v); }
  @$pb.TagNumber(4)
  $core.bool hasOs() => $_has(3);
  @$pb.TagNumber(4)
  void clearOs() => clearField(4);

  @$pb.TagNumber(5)
  $core.String get publicIp => $_getSZ(4);
  @$pb.TagNumber(5)
  set publicIp($core.String v) { $_setString(4, v); }
  @$pb.TagNumber(5)
  $core.bool hasPublicIp() => $_has(4);
  @$pb.TagNumber(5)
  void clearPublicIp() => clearField(5);

  @$pb.TagNumber(6)
  $core.List<$core.String> get privateIps => $_getList(5);

  @$pb.TagNumber(7)
  $fixnum.Int64 get createdAt => $_getI64(6);
  @$pb.TagNumber(7)
  set createdAt($fixnum.Int64 v) { $_setInt64(6, v); }
  @$pb.TagNumber(7)
  $core.bool hasCreatedAt() => $_has(6);
  @$pb.TagNumber(7)
  void clearCreatedAt() => clearField(7);

  @$pb.TagNumber(8)
  $fixnum.Int64 get lastConnected => $_getI64(7);
  @$pb.TagNumber(8)
  set lastConnected($fixnum.Int64 v) { $_setInt64(7, v); }
  @$pb.TagNumber(8)
  $core.bool hasLastConnected() => $_has(7);
  @$pb.TagNumber(8)
  void clearLastConnected() => clearField(8);

  @$pb.TagNumber(9)
  $core.String get version => $_getSZ(8);
  @$pb.TagNumber(9)
  set version($core.String v) { $_setString(8, v); }
  @$pb.TagNumber(9)
  $core.bool hasVersion() => $_has(8);
  @$pb.TagNumber(9)
  void clearVersion() => clearField(9);
}

class PeerInfoListResponse extends $pb.GeneratedMessage {
  factory PeerInfoListResponse({
    $core.Iterable<PeerInfo>? peers,
  }) {
    final $result = create();
    if (peers != null) {
      $result.peers.addAll(peers);
    }
    return $result;
  }
  PeerInfoListResponse._() : super();
  factory PeerInfoListResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory PeerInfoListResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'PeerInfoListResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..pc<PeerInfo>(1, _omitFieldNames ? '' : 'peers', $pb.PbFieldType.PM, subBuilder: PeerInfo.create)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  PeerInfoListResponse clone() => PeerInfoListResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  PeerInfoListResponse copyWith(void Function(PeerInfoListResponse) updates) => super.copyWith((message) => updates(message as PeerInfoListResponse)) as PeerInfoListResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static PeerInfoListResponse create() => PeerInfoListResponse._();
  PeerInfoListResponse createEmptyInstance() => create();
  static $pb.PbList<PeerInfoListResponse> createRepeated() => $pb.PbList<PeerInfoListResponse>();
  @$core.pragma('dart2js:noInline')
  static PeerInfoListResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<PeerInfoListResponse>(create);
  static PeerInfoListResponse? _defaultInstance;

  @$pb.TagNumber(1)
  $core.List<PeerInfo> get peers => $_getList(0);
}

class PeerInfoResponse extends $pb.GeneratedMessage {
  factory PeerInfoResponse({
    PeerInfo? peerInfo,
  }) {
    final $result = create();
    if (peerInfo != null) {
      $result.peerInfo = peerInfo;
    }
    return $result;
  }
  PeerInfoResponse._() : super();
  factory PeerInfoResponse.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory PeerInfoResponse.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'PeerInfoResponse', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOM<PeerInfo>(1, _omitFieldNames ? '' : 'peerInfo', subBuilder: PeerInfo.create)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  PeerInfoResponse clone() => PeerInfoResponse()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  PeerInfoResponse copyWith(void Function(PeerInfoResponse) updates) => super.copyWith((message) => updates(message as PeerInfoResponse)) as PeerInfoResponse;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static PeerInfoResponse create() => PeerInfoResponse._();
  PeerInfoResponse createEmptyInstance() => create();
  static $pb.PbList<PeerInfoResponse> createRepeated() => $pb.PbList<PeerInfoResponse>();
  @$core.pragma('dart2js:noInline')
  static PeerInfoResponse getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<PeerInfoResponse>(create);
  static PeerInfoResponse? _defaultInstance;

  @$pb.TagNumber(1)
  PeerInfo get peerInfo => $_getN(0);
  @$pb.TagNumber(1)
  set peerInfo(PeerInfo v) { setField(1, v); }
  @$pb.TagNumber(1)
  $core.bool hasPeerInfo() => $_has(0);
  @$pb.TagNumber(1)
  void clearPeerInfo() => clearField(1);
  @$pb.TagNumber(1)
  PeerInfo ensurePeerInfo() => $_ensure(0);
}

class AddressBookAddOrUpdateRequest extends $pb.GeneratedMessage {
  factory AddressBookAddOrUpdateRequest({
    PeerInfo? peerInfo,
  }) {
    final $result = create();
    if (peerInfo != null) {
      $result.peerInfo = peerInfo;
    }
    return $result;
  }
  AddressBookAddOrUpdateRequest._() : super();
  factory AddressBookAddOrUpdateRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory AddressBookAddOrUpdateRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'AddressBookAddOrUpdateRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOM<PeerInfo>(1, _omitFieldNames ? '' : 'peerInfo', subBuilder: PeerInfo.create)
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  AddressBookAddOrUpdateRequest clone() => AddressBookAddOrUpdateRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  AddressBookAddOrUpdateRequest copyWith(void Function(AddressBookAddOrUpdateRequest) updates) => super.copyWith((message) => updates(message as AddressBookAddOrUpdateRequest)) as AddressBookAddOrUpdateRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static AddressBookAddOrUpdateRequest create() => AddressBookAddOrUpdateRequest._();
  AddressBookAddOrUpdateRequest createEmptyInstance() => create();
  static $pb.PbList<AddressBookAddOrUpdateRequest> createRepeated() => $pb.PbList<AddressBookAddOrUpdateRequest>();
  @$core.pragma('dart2js:noInline')
  static AddressBookAddOrUpdateRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddressBookAddOrUpdateRequest>(create);
  static AddressBookAddOrUpdateRequest? _defaultInstance;

  @$pb.TagNumber(1)
  PeerInfo get peerInfo => $_getN(0);
  @$pb.TagNumber(1)
  set peerInfo(PeerInfo v) { setField(1, v); }
  @$pb.TagNumber(1)
  $core.bool hasPeerInfo() => $_has(0);
  @$pb.TagNumber(1)
  void clearPeerInfo() => clearField(1);
  @$pb.TagNumber(1)
  PeerInfo ensurePeerInfo() => $_ensure(0);
}

class AddressBookGetPeerRequest extends $pb.GeneratedMessage {
  factory AddressBookGetPeerRequest({
    $core.String? peerId,
  }) {
    final $result = create();
    if (peerId != null) {
      $result.peerId = peerId;
    }
    return $result;
  }
  AddressBookGetPeerRequest._() : super();
  factory AddressBookGetPeerRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory AddressBookGetPeerRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'AddressBookGetPeerRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'peerId')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  AddressBookGetPeerRequest clone() => AddressBookGetPeerRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  AddressBookGetPeerRequest copyWith(void Function(AddressBookGetPeerRequest) updates) => super.copyWith((message) => updates(message as AddressBookGetPeerRequest)) as AddressBookGetPeerRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static AddressBookGetPeerRequest create() => AddressBookGetPeerRequest._();
  AddressBookGetPeerRequest createEmptyInstance() => create();
  static $pb.PbList<AddressBookGetPeerRequest> createRepeated() => $pb.PbList<AddressBookGetPeerRequest>();
  @$core.pragma('dart2js:noInline')
  static AddressBookGetPeerRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddressBookGetPeerRequest>(create);
  static AddressBookGetPeerRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get peerId => $_getSZ(0);
  @$pb.TagNumber(1)
  set peerId($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasPeerId() => $_has(0);
  @$pb.TagNumber(1)
  void clearPeerId() => clearField(1);
}

class AddressBookRemoveRequest extends $pb.GeneratedMessage {
  factory AddressBookRemoveRequest({
    $core.String? peerId,
  }) {
    final $result = create();
    if (peerId != null) {
      $result.peerId = peerId;
    }
    return $result;
  }
  AddressBookRemoveRequest._() : super();
  factory AddressBookRemoveRequest.fromBuffer($core.List<$core.int> i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromBuffer(i, r);
  factory AddressBookRemoveRequest.fromJson($core.String i, [$pb.ExtensionRegistry r = $pb.ExtensionRegistry.EMPTY]) => create()..mergeFromJson(i, r);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(_omitMessageNames ? '' : 'AddressBookRemoveRequest', package: const $pb.PackageName(_omitMessageNames ? '' : 'fungi_daemon'), createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'peerId')
    ..hasRequiredFields = false
  ;

  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.deepCopy] instead. '
  'Will be removed in next major version')
  AddressBookRemoveRequest clone() => AddressBookRemoveRequest()..mergeFromMessage(this);
  @$core.Deprecated(
  'Using this can add significant overhead to your binary. '
  'Use [GeneratedMessageGenericExtensions.rebuild] instead. '
  'Will be removed in next major version')
  AddressBookRemoveRequest copyWith(void Function(AddressBookRemoveRequest) updates) => super.copyWith((message) => updates(message as AddressBookRemoveRequest)) as AddressBookRemoveRequest;

  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static AddressBookRemoveRequest create() => AddressBookRemoveRequest._();
  AddressBookRemoveRequest createEmptyInstance() => create();
  static $pb.PbList<AddressBookRemoveRequest> createRepeated() => $pb.PbList<AddressBookRemoveRequest>();
  @$core.pragma('dart2js:noInline')
  static AddressBookRemoveRequest getDefault() => _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<AddressBookRemoveRequest>(create);
  static AddressBookRemoveRequest? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get peerId => $_getSZ(0);
  @$pb.TagNumber(1)
  set peerId($core.String v) { $_setString(0, v); }
  @$pb.TagNumber(1)
  $core.bool hasPeerId() => $_has(0);
  @$pb.TagNumber(1)
  void clearPeerId() => clearField(1);
}


const _omitFieldNames = $core.bool.fromEnvironment('protobuf.omit_field_names');
const _omitMessageNames = $core.bool.fromEnvironment('protobuf.omit_message_names');
