use super::{ConnectionRecordSliceExt, ConnectionSelectionStrategy, SwarmControl};
use crate::{ConnectionGovernanceInfo, ConnectionGovernanceState, ConnectionRecord, State};
use libp2p::swarm::ConnectionId;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

pub(super) const CONNECTION_GOVERNANCE_INTERVAL: Duration = Duration::from_secs(60);
pub(super) const CONNECTION_GOVERNANCE_GRACE_PERIOD: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct ConnectionClosurePlan {
    pub(super) connection_id: ConnectionId,
    pub(super) recommended_connection_id: ConnectionId,
    pub(super) active_stream_count: usize,
    pub(super) reason: String,
    pub(super) action: GovernanceAction,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum GovernanceAction {
    MarkDeprecated,
    MarkClosing,
    CloseNow,
}

pub(super) fn next_governance_action(
    current_governance: Option<&ConnectionGovernanceInfo>,
    reason: &str,
    active_stream_count: usize,
) -> Option<GovernanceAction> {
    if active_stream_count > 0 {
        return Some(GovernanceAction::MarkDeprecated);
    }

    let Some(current_governance) = current_governance else {
        return Some(GovernanceAction::MarkDeprecated);
    };

    if current_governance.reason.as_deref() != Some(reason) {
        return Some(GovernanceAction::MarkDeprecated);
    }

    match current_governance.state {
        ConnectionGovernanceState::Unknown | ConnectionGovernanceState::Recommended => {
            Some(GovernanceAction::MarkDeprecated)
        }
        ConnectionGovernanceState::Deprecated => {
            let Some(changed_at) = current_governance.changed_at else {
                return Some(GovernanceAction::MarkDeprecated);
            };

            if deprecated_long_enough(changed_at) {
                Some(GovernanceAction::MarkClosing)
            } else {
                None
            }
        }
        ConnectionGovernanceState::Closing => Some(GovernanceAction::CloseNow),
    }
}

pub(super) async fn connection_governance_loop(swarm_control: SwarmControl) {
    let mut interval = tokio::time::interval(CONNECTION_GOVERNANCE_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        interval.tick().await;

        let peer_ids = swarm_control.state().connected_peer_ids();

        for peer_id in peer_ids {
            let mut selected = swarm_control.state().get_connections_by_peer_id(&peer_id);
            if selected.is_empty() {
                continue;
            }

            let strategy = swarm_control.connection_selection_strategy();
            selected.sort_by_strategy(strategy);

            let recommended_connection_id = selected[0].connection_id;
            let recommended_reason = SwarmControl::recommended_reason(strategy, &selected);
            swarm_control.state().update_connection_governance(
                &recommended_connection_id,
                ConnectionGovernanceState::Recommended,
                Some(recommended_reason),
            );

            if selected.len() <= 1 {
                continue;
            }

            let governance_info = selected
                .iter()
                .map(|connection| {
                    (
                        connection.connection_id,
                        swarm_control
                            .state()
                            .connection_governance_info(&connection.connection_id)
                            .unwrap_or_default(),
                    )
                })
                .collect::<HashMap<_, _>>();

            let closure_plan = SwarmControl::build_connection_closure_plan(
                swarm_control.state(),
                &selected,
                &governance_info,
            );

            for plan in closure_plan {
                match plan.action {
                    GovernanceAction::MarkDeprecated => {
                        swarm_control.state().update_connection_governance(
                            &plan.connection_id,
                            ConnectionGovernanceState::Deprecated,
                            Some(plan.reason),
                        );
                    }
                    GovernanceAction::MarkClosing => {
                        log::info!(
                            "Marking deprecated idle connection {} for peer {} as closing in favor of recommended connection {} (active_streams={})",
                            plan.connection_id,
                            peer_id,
                            plan.recommended_connection_id,
                            plan.active_stream_count
                        );
                        swarm_control.state().update_connection_governance(
                            &plan.connection_id,
                            ConnectionGovernanceState::Closing,
                            Some(plan.reason),
                        );
                    }
                    GovernanceAction::CloseNow => {
                        log::warn!(
                            "Closing deprecated idle connection {} for peer {} in favor of recommended connection {} (active_streams={})",
                            plan.connection_id,
                            peer_id,
                            plan.recommended_connection_id,
                            plan.active_stream_count
                        );

                        match swarm_control.close_connection(plan.connection_id).await {
                            Ok(true) => {
                                log::info!(
                                    "Closed deprecated idle connection {} for peer {}",
                                    plan.connection_id,
                                    peer_id
                                );
                            }
                            Ok(false) => {
                                log::debug!(
                                    "Connection {} for peer {} was already gone before governance close",
                                    plan.connection_id,
                                    peer_id
                                );
                            }
                            Err(error) => {
                                log::warn!(
                                    "Failed to close deprecated idle connection {} for peer {}: {}",
                                    plan.connection_id,
                                    peer_id,
                                    error
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

impl SwarmControl {
    pub(super) fn build_connection_closure_plan(
        state: &State,
        selected: &[ConnectionRecord],
        governance_info: &HashMap<ConnectionId, ConnectionGovernanceInfo>,
    ) -> Vec<ConnectionClosurePlan> {
        if selected.len() <= 1 {
            return Vec::new();
        }

        let recommended_connection_id = selected[0].connection_id;

        selected
            .iter()
            .skip(1)
            .filter_map(|connection| {
                let reason = lower_priority_reason(recommended_connection_id);
                let active_stream_count = state
                    .active_streams_by_connection(&connection.connection_id)
                    .len();
                let current_governance = governance_info.get(&connection.connection_id);

                let action =
                    next_governance_action(current_governance, &reason, active_stream_count)?;

                Some(ConnectionClosurePlan {
                    connection_id: connection.connection_id,
                    recommended_connection_id,
                    active_stream_count,
                    reason,
                    action,
                })
            })
            .collect()
    }

    fn recommended_reason(
        strategy: ConnectionSelectionStrategy,
        selected: &[ConnectionRecord],
    ) -> String {
        let Some(recommended) = selected.first() else {
            return "selected-by-policy".to_string();
        };
        let alternatives = &selected[1..];

        match strategy {
            ConnectionSelectionStrategy::PreferDirect
                if !recommended.is_relay()
                    && alternatives.iter().any(|candidate| candidate.is_relay()) =>
            {
                return "selected-by-prefer-direct".to_string();
            }
            ConnectionSelectionStrategy::PreferRelay
                if recommended.is_relay()
                    && alternatives.iter().any(|candidate| !candidate.is_relay()) =>
            {
                return "selected-by-prefer-relay".to_string();
            }
            _ => {}
        }

        if alternatives.iter().any(|candidate| {
            established_at_key(Some(recommended.established_at))
                < established_at_key(Some(candidate.established_at))
        }) {
            return "selected-by-earlier-established".to_string();
        }

        match strategy {
            ConnectionSelectionStrategy::PreferDirect => "selected-by-prefer-direct-ordering",
            ConnectionSelectionStrategy::PreferRelay => "selected-by-prefer-relay-ordering",
        }
        .to_string()
    }
}

fn established_at_key(established_at: Option<SystemTime>) -> Duration {
    established_at
        .and_then(|ts| ts.duration_since(SystemTime::UNIX_EPOCH).ok())
        .unwrap_or(Duration::MAX)
}

fn lower_priority_reason(recommended_connection_id: ConnectionId) -> String {
    format!(
        "lower-priority-than-connection-{}",
        recommended_connection_id
    )
}

fn deprecated_long_enough(changed_at: SystemTime) -> bool {
    SystemTime::now()
        .duration_since(changed_at)
        .unwrap_or(Duration::ZERO)
        >= CONNECTION_GOVERNANCE_GRACE_PERIOD
}
