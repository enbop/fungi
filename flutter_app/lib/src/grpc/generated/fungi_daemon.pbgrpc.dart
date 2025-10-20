//
//  Generated code. Do not modify.
//  source: fungi_daemon.proto
//
// @dart = 2.12

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_final_fields
// ignore_for_file: unnecessary_import, unnecessary_this, unused_import

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:grpc/service_api.dart' as $grpc;
import 'package:protobuf/protobuf.dart' as $pb;

import 'fungi_daemon.pb.dart' as $0;

export 'fungi_daemon.pb.dart';

@$pb.GrpcServiceName('fungi_daemon.FungiDaemon')
class FungiDaemonClient extends $grpc.Client {
  static final _$version = $grpc.ClientMethod<$0.Empty, $0.VersionResponse>(
      '/fungi_daemon.FungiDaemon/Version',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.VersionResponse.fromBuffer(value));
  static final _$peerId = $grpc.ClientMethod<$0.Empty, $0.PeerIdResponse>(
      '/fungi_daemon.FungiDaemon/PeerId',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.PeerIdResponse.fromBuffer(value));
  static final _$configFilePath = $grpc.ClientMethod<$0.Empty, $0.ConfigFilePathResponse>(
      '/fungi_daemon.FungiDaemon/ConfigFilePath',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.ConfigFilePathResponse.fromBuffer(value));
  static final _$hostname = $grpc.ClientMethod<$0.Empty, $0.HostnameResponse>(
      '/fungi_daemon.FungiDaemon/Hostname',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.HostnameResponse.fromBuffer(value));
  static final _$getIncomingAllowedPeers = $grpc.ClientMethod<$0.Empty, $0.IncomingAllowedPeersListResponse>(
      '/fungi_daemon.FungiDaemon/GetIncomingAllowedPeers',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.IncomingAllowedPeersListResponse.fromBuffer(value));
  static final _$addIncomingAllowedPeer = $grpc.ClientMethod<$0.AddIncomingAllowedPeerRequest, $0.Empty>(
      '/fungi_daemon.FungiDaemon/AddIncomingAllowedPeer',
      ($0.AddIncomingAllowedPeerRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));
  static final _$removeIncomingAllowedPeer = $grpc.ClientMethod<$0.RemoveIncomingAllowedPeerRequest, $0.Empty>(
      '/fungi_daemon.FungiDaemon/RemoveIncomingAllowedPeer',
      ($0.RemoveIncomingAllowedPeerRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));
  static final _$getFileTransferServiceEnabled = $grpc.ClientMethod<$0.Empty, $0.FileTransferServiceEnabledResponse>(
      '/fungi_daemon.FungiDaemon/GetFileTransferServiceEnabled',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.FileTransferServiceEnabledResponse.fromBuffer(value));
  static final _$getFileTransferServiceRootDir = $grpc.ClientMethod<$0.Empty, $0.FileTransferServiceRootDirResponse>(
      '/fungi_daemon.FungiDaemon/GetFileTransferServiceRootDir',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.FileTransferServiceRootDirResponse.fromBuffer(value));
  static final _$startFileTransferService = $grpc.ClientMethod<$0.StartFileTransferServiceRequest, $0.Empty>(
      '/fungi_daemon.FungiDaemon/StartFileTransferService',
      ($0.StartFileTransferServiceRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));
  static final _$stopFileTransferService = $grpc.ClientMethod<$0.Empty, $0.Empty>(
      '/fungi_daemon.FungiDaemon/StopFileTransferService',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));
  static final _$addFileTransferClient = $grpc.ClientMethod<$0.AddFileTransferClientRequest, $0.Empty>(
      '/fungi_daemon.FungiDaemon/AddFileTransferClient',
      ($0.AddFileTransferClientRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));
  static final _$removeFileTransferClient = $grpc.ClientMethod<$0.RemoveFileTransferClientRequest, $0.Empty>(
      '/fungi_daemon.FungiDaemon/RemoveFileTransferClient',
      ($0.RemoveFileTransferClientRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));
  static final _$enableFileTransferClient = $grpc.ClientMethod<$0.EnableFileTransferClientRequest, $0.Empty>(
      '/fungi_daemon.FungiDaemon/EnableFileTransferClient',
      ($0.EnableFileTransferClientRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));
  static final _$getAllFileTransferClients = $grpc.ClientMethod<$0.Empty, $0.FileTransferClientsResponse>(
      '/fungi_daemon.FungiDaemon/GetAllFileTransferClients',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.FileTransferClientsResponse.fromBuffer(value));
  static final _$getFtpProxy = $grpc.ClientMethod<$0.Empty, $0.FtpProxyResponse>(
      '/fungi_daemon.FungiDaemon/GetFtpProxy',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.FtpProxyResponse.fromBuffer(value));
  static final _$updateFtpProxy = $grpc.ClientMethod<$0.UpdateFtpProxyRequest, $0.Empty>(
      '/fungi_daemon.FungiDaemon/UpdateFtpProxy',
      ($0.UpdateFtpProxyRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));
  static final _$getWebdavProxy = $grpc.ClientMethod<$0.Empty, $0.WebdavProxyResponse>(
      '/fungi_daemon.FungiDaemon/GetWebdavProxy',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.WebdavProxyResponse.fromBuffer(value));
  static final _$updateWebdavProxy = $grpc.ClientMethod<$0.UpdateWebdavProxyRequest, $0.Empty>(
      '/fungi_daemon.FungiDaemon/UpdateWebdavProxy',
      ($0.UpdateWebdavProxyRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));
  static final _$getTcpTunnelingConfig = $grpc.ClientMethod<$0.Empty, $0.TcpTunnelingConfigResponse>(
      '/fungi_daemon.FungiDaemon/GetTcpTunnelingConfig',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.TcpTunnelingConfigResponse.fromBuffer(value));
  static final _$addTcpForwardingRule = $grpc.ClientMethod<$0.AddTcpForwardingRuleRequest, $0.TcpForwardingRuleResponse>(
      '/fungi_daemon.FungiDaemon/AddTcpForwardingRule',
      ($0.AddTcpForwardingRuleRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.TcpForwardingRuleResponse.fromBuffer(value));
  static final _$removeTcpForwardingRule = $grpc.ClientMethod<$0.RemoveTcpForwardingRuleRequest, $0.Empty>(
      '/fungi_daemon.FungiDaemon/RemoveTcpForwardingRule',
      ($0.RemoveTcpForwardingRuleRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));
  static final _$addTcpListeningRule = $grpc.ClientMethod<$0.AddTcpListeningRuleRequest, $0.TcpListeningRuleResponse>(
      '/fungi_daemon.FungiDaemon/AddTcpListeningRule',
      ($0.AddTcpListeningRuleRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.TcpListeningRuleResponse.fromBuffer(value));
  static final _$removeTcpListeningRule = $grpc.ClientMethod<$0.RemoveTcpListeningRuleRequest, $0.Empty>(
      '/fungi_daemon.FungiDaemon/RemoveTcpListeningRule',
      ($0.RemoveTcpListeningRuleRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));
  static final _$listMdnsDevices = $grpc.ClientMethod<$0.Empty, $0.PeerInfoListResponse>(
      '/fungi_daemon.FungiDaemon/ListMdnsDevices',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.PeerInfoListResponse.fromBuffer(value));
  static final _$listAddressBookPeers = $grpc.ClientMethod<$0.Empty, $0.PeerInfoListResponse>(
      '/fungi_daemon.FungiDaemon/ListAddressBookPeers',
      ($0.Empty value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.PeerInfoListResponse.fromBuffer(value));
  static final _$updateAddressBookPeer = $grpc.ClientMethod<$0.UpdateAddressBookPeerRequest, $0.Empty>(
      '/fungi_daemon.FungiDaemon/UpdateAddressBookPeer',
      ($0.UpdateAddressBookPeerRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));
  static final _$getAddressBookPeer = $grpc.ClientMethod<$0.GetAddressBookPeerRequest, $0.PeerInfoResponse>(
      '/fungi_daemon.FungiDaemon/GetAddressBookPeer',
      ($0.GetAddressBookPeerRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.PeerInfoResponse.fromBuffer(value));
  static final _$removeAddressBookPeer = $grpc.ClientMethod<$0.RemoveAddressBookPeerRequest, $0.Empty>(
      '/fungi_daemon.FungiDaemon/RemoveAddressBookPeer',
      ($0.RemoveAddressBookPeerRequest value) => value.writeToBuffer(),
      ($core.List<$core.int> value) => $0.Empty.fromBuffer(value));

  FungiDaemonClient($grpc.ClientChannel channel,
      {$grpc.CallOptions? options,
      $core.Iterable<$grpc.ClientInterceptor>? interceptors})
      : super(channel, options: options,
        interceptors: interceptors);

  $grpc.ResponseFuture<$0.VersionResponse> version($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$version, request, options: options);
  }

  $grpc.ResponseFuture<$0.PeerIdResponse> peerId($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$peerId, request, options: options);
  }

  $grpc.ResponseFuture<$0.ConfigFilePathResponse> configFilePath($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$configFilePath, request, options: options);
  }

  $grpc.ResponseFuture<$0.HostnameResponse> hostname($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$hostname, request, options: options);
  }

  $grpc.ResponseFuture<$0.IncomingAllowedPeersListResponse> getIncomingAllowedPeers($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$getIncomingAllowedPeers, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> addIncomingAllowedPeer($0.AddIncomingAllowedPeerRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$addIncomingAllowedPeer, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> removeIncomingAllowedPeer($0.RemoveIncomingAllowedPeerRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$removeIncomingAllowedPeer, request, options: options);
  }

  $grpc.ResponseFuture<$0.FileTransferServiceEnabledResponse> getFileTransferServiceEnabled($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$getFileTransferServiceEnabled, request, options: options);
  }

  $grpc.ResponseFuture<$0.FileTransferServiceRootDirResponse> getFileTransferServiceRootDir($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$getFileTransferServiceRootDir, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> startFileTransferService($0.StartFileTransferServiceRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$startFileTransferService, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> stopFileTransferService($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$stopFileTransferService, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> addFileTransferClient($0.AddFileTransferClientRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$addFileTransferClient, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> removeFileTransferClient($0.RemoveFileTransferClientRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$removeFileTransferClient, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> enableFileTransferClient($0.EnableFileTransferClientRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$enableFileTransferClient, request, options: options);
  }

  $grpc.ResponseFuture<$0.FileTransferClientsResponse> getAllFileTransferClients($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$getAllFileTransferClients, request, options: options);
  }

  $grpc.ResponseFuture<$0.FtpProxyResponse> getFtpProxy($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$getFtpProxy, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> updateFtpProxy($0.UpdateFtpProxyRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$updateFtpProxy, request, options: options);
  }

  $grpc.ResponseFuture<$0.WebdavProxyResponse> getWebdavProxy($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$getWebdavProxy, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> updateWebdavProxy($0.UpdateWebdavProxyRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$updateWebdavProxy, request, options: options);
  }

  $grpc.ResponseFuture<$0.TcpTunnelingConfigResponse> getTcpTunnelingConfig($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$getTcpTunnelingConfig, request, options: options);
  }

  $grpc.ResponseFuture<$0.TcpForwardingRuleResponse> addTcpForwardingRule($0.AddTcpForwardingRuleRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$addTcpForwardingRule, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> removeTcpForwardingRule($0.RemoveTcpForwardingRuleRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$removeTcpForwardingRule, request, options: options);
  }

  $grpc.ResponseFuture<$0.TcpListeningRuleResponse> addTcpListeningRule($0.AddTcpListeningRuleRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$addTcpListeningRule, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> removeTcpListeningRule($0.RemoveTcpListeningRuleRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$removeTcpListeningRule, request, options: options);
  }

  $grpc.ResponseFuture<$0.PeerInfoListResponse> listMdnsDevices($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$listMdnsDevices, request, options: options);
  }

  $grpc.ResponseFuture<$0.PeerInfoListResponse> listAddressBookPeers($0.Empty request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$listAddressBookPeers, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> updateAddressBookPeer($0.UpdateAddressBookPeerRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$updateAddressBookPeer, request, options: options);
  }

  $grpc.ResponseFuture<$0.PeerInfoResponse> getAddressBookPeer($0.GetAddressBookPeerRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$getAddressBookPeer, request, options: options);
  }

  $grpc.ResponseFuture<$0.Empty> removeAddressBookPeer($0.RemoveAddressBookPeerRequest request, {$grpc.CallOptions? options}) {
    return $createUnaryCall(_$removeAddressBookPeer, request, options: options);
  }
}

@$pb.GrpcServiceName('fungi_daemon.FungiDaemon')
abstract class FungiDaemonServiceBase extends $grpc.Service {
  $core.String get $name => 'fungi_daemon.FungiDaemon';

  FungiDaemonServiceBase() {
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.VersionResponse>(
        'Version',
        version_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.VersionResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.PeerIdResponse>(
        'PeerId',
        peerId_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.PeerIdResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.ConfigFilePathResponse>(
        'ConfigFilePath',
        configFilePath_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.ConfigFilePathResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.HostnameResponse>(
        'Hostname',
        hostname_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.HostnameResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.IncomingAllowedPeersListResponse>(
        'GetIncomingAllowedPeers',
        getIncomingAllowedPeers_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.IncomingAllowedPeersListResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.AddIncomingAllowedPeerRequest, $0.Empty>(
        'AddIncomingAllowedPeer',
        addIncomingAllowedPeer_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.AddIncomingAllowedPeerRequest.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.RemoveIncomingAllowedPeerRequest, $0.Empty>(
        'RemoveIncomingAllowedPeer',
        removeIncomingAllowedPeer_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.RemoveIncomingAllowedPeerRequest.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.FileTransferServiceEnabledResponse>(
        'GetFileTransferServiceEnabled',
        getFileTransferServiceEnabled_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.FileTransferServiceEnabledResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.FileTransferServiceRootDirResponse>(
        'GetFileTransferServiceRootDir',
        getFileTransferServiceRootDir_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.FileTransferServiceRootDirResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.StartFileTransferServiceRequest, $0.Empty>(
        'StartFileTransferService',
        startFileTransferService_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.StartFileTransferServiceRequest.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.Empty>(
        'StopFileTransferService',
        stopFileTransferService_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.AddFileTransferClientRequest, $0.Empty>(
        'AddFileTransferClient',
        addFileTransferClient_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.AddFileTransferClientRequest.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.RemoveFileTransferClientRequest, $0.Empty>(
        'RemoveFileTransferClient',
        removeFileTransferClient_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.RemoveFileTransferClientRequest.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.EnableFileTransferClientRequest, $0.Empty>(
        'EnableFileTransferClient',
        enableFileTransferClient_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.EnableFileTransferClientRequest.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.FileTransferClientsResponse>(
        'GetAllFileTransferClients',
        getAllFileTransferClients_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.FileTransferClientsResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.FtpProxyResponse>(
        'GetFtpProxy',
        getFtpProxy_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.FtpProxyResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.UpdateFtpProxyRequest, $0.Empty>(
        'UpdateFtpProxy',
        updateFtpProxy_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.UpdateFtpProxyRequest.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.WebdavProxyResponse>(
        'GetWebdavProxy',
        getWebdavProxy_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.WebdavProxyResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.UpdateWebdavProxyRequest, $0.Empty>(
        'UpdateWebdavProxy',
        updateWebdavProxy_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.UpdateWebdavProxyRequest.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.TcpTunnelingConfigResponse>(
        'GetTcpTunnelingConfig',
        getTcpTunnelingConfig_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.TcpTunnelingConfigResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.AddTcpForwardingRuleRequest, $0.TcpForwardingRuleResponse>(
        'AddTcpForwardingRule',
        addTcpForwardingRule_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.AddTcpForwardingRuleRequest.fromBuffer(value),
        ($0.TcpForwardingRuleResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.RemoveTcpForwardingRuleRequest, $0.Empty>(
        'RemoveTcpForwardingRule',
        removeTcpForwardingRule_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.RemoveTcpForwardingRuleRequest.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.AddTcpListeningRuleRequest, $0.TcpListeningRuleResponse>(
        'AddTcpListeningRule',
        addTcpListeningRule_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.AddTcpListeningRuleRequest.fromBuffer(value),
        ($0.TcpListeningRuleResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.RemoveTcpListeningRuleRequest, $0.Empty>(
        'RemoveTcpListeningRule',
        removeTcpListeningRule_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.RemoveTcpListeningRuleRequest.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.PeerInfoListResponse>(
        'ListMdnsDevices',
        listMdnsDevices_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.PeerInfoListResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.Empty, $0.PeerInfoListResponse>(
        'ListAddressBookPeers',
        listAddressBookPeers_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.Empty.fromBuffer(value),
        ($0.PeerInfoListResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.UpdateAddressBookPeerRequest, $0.Empty>(
        'UpdateAddressBookPeer',
        updateAddressBookPeer_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.UpdateAddressBookPeerRequest.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.GetAddressBookPeerRequest, $0.PeerInfoResponse>(
        'GetAddressBookPeer',
        getAddressBookPeer_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.GetAddressBookPeerRequest.fromBuffer(value),
        ($0.PeerInfoResponse value) => value.writeToBuffer()));
    $addMethod($grpc.ServiceMethod<$0.RemoveAddressBookPeerRequest, $0.Empty>(
        'RemoveAddressBookPeer',
        removeAddressBookPeer_Pre,
        false,
        false,
        ($core.List<$core.int> value) => $0.RemoveAddressBookPeerRequest.fromBuffer(value),
        ($0.Empty value) => value.writeToBuffer()));
  }

  $async.Future<$0.VersionResponse> version_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return version(call, await request);
  }

  $async.Future<$0.PeerIdResponse> peerId_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return peerId(call, await request);
  }

  $async.Future<$0.ConfigFilePathResponse> configFilePath_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return configFilePath(call, await request);
  }

  $async.Future<$0.HostnameResponse> hostname_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return hostname(call, await request);
  }

  $async.Future<$0.IncomingAllowedPeersListResponse> getIncomingAllowedPeers_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return getIncomingAllowedPeers(call, await request);
  }

  $async.Future<$0.Empty> addIncomingAllowedPeer_Pre($grpc.ServiceCall call, $async.Future<$0.AddIncomingAllowedPeerRequest> request) async {
    return addIncomingAllowedPeer(call, await request);
  }

  $async.Future<$0.Empty> removeIncomingAllowedPeer_Pre($grpc.ServiceCall call, $async.Future<$0.RemoveIncomingAllowedPeerRequest> request) async {
    return removeIncomingAllowedPeer(call, await request);
  }

  $async.Future<$0.FileTransferServiceEnabledResponse> getFileTransferServiceEnabled_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return getFileTransferServiceEnabled(call, await request);
  }

  $async.Future<$0.FileTransferServiceRootDirResponse> getFileTransferServiceRootDir_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return getFileTransferServiceRootDir(call, await request);
  }

  $async.Future<$0.Empty> startFileTransferService_Pre($grpc.ServiceCall call, $async.Future<$0.StartFileTransferServiceRequest> request) async {
    return startFileTransferService(call, await request);
  }

  $async.Future<$0.Empty> stopFileTransferService_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return stopFileTransferService(call, await request);
  }

  $async.Future<$0.Empty> addFileTransferClient_Pre($grpc.ServiceCall call, $async.Future<$0.AddFileTransferClientRequest> request) async {
    return addFileTransferClient(call, await request);
  }

  $async.Future<$0.Empty> removeFileTransferClient_Pre($grpc.ServiceCall call, $async.Future<$0.RemoveFileTransferClientRequest> request) async {
    return removeFileTransferClient(call, await request);
  }

  $async.Future<$0.Empty> enableFileTransferClient_Pre($grpc.ServiceCall call, $async.Future<$0.EnableFileTransferClientRequest> request) async {
    return enableFileTransferClient(call, await request);
  }

  $async.Future<$0.FileTransferClientsResponse> getAllFileTransferClients_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return getAllFileTransferClients(call, await request);
  }

  $async.Future<$0.FtpProxyResponse> getFtpProxy_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return getFtpProxy(call, await request);
  }

  $async.Future<$0.Empty> updateFtpProxy_Pre($grpc.ServiceCall call, $async.Future<$0.UpdateFtpProxyRequest> request) async {
    return updateFtpProxy(call, await request);
  }

  $async.Future<$0.WebdavProxyResponse> getWebdavProxy_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return getWebdavProxy(call, await request);
  }

  $async.Future<$0.Empty> updateWebdavProxy_Pre($grpc.ServiceCall call, $async.Future<$0.UpdateWebdavProxyRequest> request) async {
    return updateWebdavProxy(call, await request);
  }

  $async.Future<$0.TcpTunnelingConfigResponse> getTcpTunnelingConfig_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return getTcpTunnelingConfig(call, await request);
  }

  $async.Future<$0.TcpForwardingRuleResponse> addTcpForwardingRule_Pre($grpc.ServiceCall call, $async.Future<$0.AddTcpForwardingRuleRequest> request) async {
    return addTcpForwardingRule(call, await request);
  }

  $async.Future<$0.Empty> removeTcpForwardingRule_Pre($grpc.ServiceCall call, $async.Future<$0.RemoveTcpForwardingRuleRequest> request) async {
    return removeTcpForwardingRule(call, await request);
  }

  $async.Future<$0.TcpListeningRuleResponse> addTcpListeningRule_Pre($grpc.ServiceCall call, $async.Future<$0.AddTcpListeningRuleRequest> request) async {
    return addTcpListeningRule(call, await request);
  }

  $async.Future<$0.Empty> removeTcpListeningRule_Pre($grpc.ServiceCall call, $async.Future<$0.RemoveTcpListeningRuleRequest> request) async {
    return removeTcpListeningRule(call, await request);
  }

  $async.Future<$0.PeerInfoListResponse> listMdnsDevices_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return listMdnsDevices(call, await request);
  }

  $async.Future<$0.PeerInfoListResponse> listAddressBookPeers_Pre($grpc.ServiceCall call, $async.Future<$0.Empty> request) async {
    return listAddressBookPeers(call, await request);
  }

  $async.Future<$0.Empty> updateAddressBookPeer_Pre($grpc.ServiceCall call, $async.Future<$0.UpdateAddressBookPeerRequest> request) async {
    return updateAddressBookPeer(call, await request);
  }

  $async.Future<$0.PeerInfoResponse> getAddressBookPeer_Pre($grpc.ServiceCall call, $async.Future<$0.GetAddressBookPeerRequest> request) async {
    return getAddressBookPeer(call, await request);
  }

  $async.Future<$0.Empty> removeAddressBookPeer_Pre($grpc.ServiceCall call, $async.Future<$0.RemoveAddressBookPeerRequest> request) async {
    return removeAddressBookPeer(call, await request);
  }

  $async.Future<$0.VersionResponse> version($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.PeerIdResponse> peerId($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.ConfigFilePathResponse> configFilePath($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.HostnameResponse> hostname($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.IncomingAllowedPeersListResponse> getIncomingAllowedPeers($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.Empty> addIncomingAllowedPeer($grpc.ServiceCall call, $0.AddIncomingAllowedPeerRequest request);
  $async.Future<$0.Empty> removeIncomingAllowedPeer($grpc.ServiceCall call, $0.RemoveIncomingAllowedPeerRequest request);
  $async.Future<$0.FileTransferServiceEnabledResponse> getFileTransferServiceEnabled($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.FileTransferServiceRootDirResponse> getFileTransferServiceRootDir($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.Empty> startFileTransferService($grpc.ServiceCall call, $0.StartFileTransferServiceRequest request);
  $async.Future<$0.Empty> stopFileTransferService($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.Empty> addFileTransferClient($grpc.ServiceCall call, $0.AddFileTransferClientRequest request);
  $async.Future<$0.Empty> removeFileTransferClient($grpc.ServiceCall call, $0.RemoveFileTransferClientRequest request);
  $async.Future<$0.Empty> enableFileTransferClient($grpc.ServiceCall call, $0.EnableFileTransferClientRequest request);
  $async.Future<$0.FileTransferClientsResponse> getAllFileTransferClients($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.FtpProxyResponse> getFtpProxy($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.Empty> updateFtpProxy($grpc.ServiceCall call, $0.UpdateFtpProxyRequest request);
  $async.Future<$0.WebdavProxyResponse> getWebdavProxy($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.Empty> updateWebdavProxy($grpc.ServiceCall call, $0.UpdateWebdavProxyRequest request);
  $async.Future<$0.TcpTunnelingConfigResponse> getTcpTunnelingConfig($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.TcpForwardingRuleResponse> addTcpForwardingRule($grpc.ServiceCall call, $0.AddTcpForwardingRuleRequest request);
  $async.Future<$0.Empty> removeTcpForwardingRule($grpc.ServiceCall call, $0.RemoveTcpForwardingRuleRequest request);
  $async.Future<$0.TcpListeningRuleResponse> addTcpListeningRule($grpc.ServiceCall call, $0.AddTcpListeningRuleRequest request);
  $async.Future<$0.Empty> removeTcpListeningRule($grpc.ServiceCall call, $0.RemoveTcpListeningRuleRequest request);
  $async.Future<$0.PeerInfoListResponse> listMdnsDevices($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.PeerInfoListResponse> listAddressBookPeers($grpc.ServiceCall call, $0.Empty request);
  $async.Future<$0.Empty> updateAddressBookPeer($grpc.ServiceCall call, $0.UpdateAddressBookPeerRequest request);
  $async.Future<$0.PeerInfoResponse> getAddressBookPeer($grpc.ServiceCall call, $0.GetAddressBookPeerRequest request);
  $async.Future<$0.Empty> removeAddressBookPeer($grpc.ServiceCall call, $0.RemoveAddressBookPeerRequest request);
}
