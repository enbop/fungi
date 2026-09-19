use anyhow::Result;
use fungi_config::tcp_tunneling::ListeningRule;

use crate::{
    AppliedService, RuntimeControl, ServiceApplyFailureStage, ServiceManifest,
    controls::TcpTunnelingControl, runtime::service_expose_endpoint_bindings,
    service_state::DesiredServiceState,
};

pub(crate) async fn sync_applied_service_endpoint_listeners(
    runtime: &RuntimeControl,
    tcp_tunneling: &TcpTunnelingControl,
    applied: &mut AppliedService,
) {
    if applied.desired_state != DesiredServiceState::Running || applied.outcome.failure.is_some() {
        return;
    }

    if let Err(error) = sync_service_endpoint_listeners_for_manifest(
        tcp_tunneling,
        applied.previous_manifest.as_ref(),
        false,
    )
    .await
    {
        applied.outcome.record_failure(
            ServiceApplyFailureStage::EndpointListeners,
            format!("failed to update endpoint listeners: {error}"),
        );
        return;
    }

    if let Err(error) = sync_service_endpoint_listeners_by_name(
        runtime,
        tcp_tunneling,
        &applied.instance.name,
        true,
    )
    .await
    {
        applied.outcome.record_failure(
            ServiceApplyFailureStage::EndpointListeners,
            format!("failed to publish endpoint listeners: {error}"),
        );
    }
}

pub(crate) async fn sync_service_endpoint_listeners_by_name(
    runtime: &RuntimeControl,
    tcp_tunneling: &TcpTunnelingControl,
    name: &str,
    enabled: bool,
) -> Result<()> {
    let manifest = runtime.get_service_manifest(name);
    sync_service_endpoint_listeners_for_manifest(tcp_tunneling, manifest.as_ref(), enabled).await
}

pub(crate) async fn sync_service_endpoint_listeners_for_manifest(
    tcp_tunneling: &TcpTunnelingControl,
    manifest: Option<&ServiceManifest>,
    enabled: bool,
) -> Result<()> {
    let Some(manifest) = manifest else {
        return Ok(());
    };

    let listening_rules = tcp_tunneling.get_listening_rules();
    for endpoint in service_expose_endpoint_bindings(manifest) {
        let existing_rule_id = listening_rules
            .iter()
            .find(|(_, rule)| {
                rule.port == endpoint.host_port
                    && rule.protocol.as_deref() == Some(endpoint.protocol.as_str())
            })
            .map(|(rule_id, _)| rule_id.clone());

        if enabled {
            if existing_rule_id.is_none() {
                tcp_tunneling
                    .add_listening_rule(ListeningRule {
                        host: "127.0.0.1".to_string(),
                        port: endpoint.host_port,
                        protocol: Some(endpoint.protocol),
                    })
                    .await?;
            }
        } else if let Some(rule_id) = existing_rule_id {
            tcp_tunneling.remove_listening_rule(&rule_id)?;
        }
    }
    Ok(())
}
