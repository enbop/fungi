use super::{ConnectionSelectionStrategy, SelectedConnection, SwarmControl};
use crate::{ConnectionGovernanceInfo, ConnectionGovernanceState};
use libp2p::swarm::ConnectionId;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

pub(super) const CONNECTION_GOVERNANCE_INTERVAL: Duration = Duration::from_secs(60);
pub(super) const CONNECTION_GOVERNANCE_GRACE_PERIOD: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct ConnectionClosurePlan {
    pub(super) connection_id: ConnectionId,
    pub(super) recommended_connection_id: ConnectionId,
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
    now: SystemTime,
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

            if now.duration_since(changed_at).unwrap_or(Duration::ZERO)
                >= CONNECTION_GOVERNANCE_GRACE_PERIOD
            {
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
        let now = SystemTime::now();

        let peer_ids = {
            let peer_connections = swarm_control.state().peer_connections();
            peer_connections.lock().keys().copied().collect::<Vec<_>>()
        };

        for peer_id in peer_ids {
            let mut selected = swarm_control.collect_selected_connections(peer_id);
            if selected.is_empty() {
                continue;
            }

            SwarmControl::sort_selected_connections(
                ConnectionSelectionStrategy::PreferDirect,
                &mut selected,
            );

            let recommended_connection_id = selected[0].connection_id;
            let recommended_reason = SwarmControl::recommended_reason(
                ConnectionSelectionStrategy::PreferDirect,
                &selected,
            );
            swarm_control.state().update_connection_governance(
                &recommended_connection_id,
                ConnectionGovernanceState::Recommended,
                Some(recommended_reason),
            );

            if selected.len() <= 1 {
                continue;
            }

            let active_stream_counts = selected
                .iter()
                .map(|connection| {
                    (
                        connection.connection_id,
                        swarm_control
                            .state()
                            .active_streams_by_connection(&connection.connection_id)
                            .len(),
                    )
                })
                .collect::<HashMap<_, _>>();

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
                ConnectionSelectionStrategy::PreferDirect,
                &mut selected,
                &active_stream_counts,
                &governance_info,
                now,
            );

            for plan in closure_plan {
                let active_stream_count = active_stream_counts
                    .get(&plan.connection_id)
                    .copied()
                    .unwrap_or(0);
                let reason = format!(
                    "lower-priority-than-connection-{}",
                    plan.recommended_connection_id
                );

                match plan.action {
                    GovernanceAction::MarkDeprecated => {
                        swarm_control.state().update_connection_governance(
                            &plan.connection_id,
                            ConnectionGovernanceState::Deprecated,
                            Some(reason),
                        );
                    }
                    GovernanceAction::MarkClosing => {
                        log::info!(
                            "Marking deprecated idle connection {} for peer {} as closing in favor of recommended connection {} (active_streams={})",
                            plan.connection_id,
                            peer_id,
                            plan.recommended_connection_id,
                            active_stream_count
                        );
                        swarm_control.state().update_connection_governance(
                            &plan.connection_id,
                            ConnectionGovernanceState::Closing,
                            Some(reason),
                        );
                    }
                    GovernanceAction::CloseNow => {
                        log::warn!(
                            "Closing deprecated idle connection {} for peer {} in favor of recommended connection {} (active_streams={})",
                            plan.connection_id,
                            peer_id,
                            plan.recommended_connection_id,
                            active_stream_count
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
    pub(super) fn sort_selected_connections(
        strategy: ConnectionSelectionStrategy,
        selected: &mut [SelectedConnection],
    ) {
        selected.sort_by(|a, b| match strategy {
            ConnectionSelectionStrategy::PreferDirect => a
                .is_relay
                .cmp(&b.is_relay)
                .then(b.active_stream_count.cmp(&a.active_stream_count))
                .then(Self::rtt_key(a.last_rtt).cmp(&Self::rtt_key(b.last_rtt)))
                .then(
                    Self::established_at_key(a.established_at)
                        .cmp(&Self::established_at_key(b.established_at)),
                )
                .then(Self::conn_id_key(a.connection_id).cmp(&Self::conn_id_key(b.connection_id))),
            ConnectionSelectionStrategy::PreferRelay => b
                .is_relay
                .cmp(&a.is_relay)
                .then(b.active_stream_count.cmp(&a.active_stream_count))
                .then(Self::rtt_key(a.last_rtt).cmp(&Self::rtt_key(b.last_rtt)))
                .then(
                    Self::established_at_key(a.established_at)
                        .cmp(&Self::established_at_key(b.established_at)),
                )
                .then(Self::conn_id_key(a.connection_id).cmp(&Self::conn_id_key(b.connection_id))),
            ConnectionSelectionStrategy::PreferLowLatency => Self::rtt_key(a.last_rtt)
                .cmp(&Self::rtt_key(b.last_rtt))
                .then(b.active_stream_count.cmp(&a.active_stream_count))
                .then(a.is_relay.cmp(&b.is_relay))
                .then(
                    Self::established_at_key(a.established_at)
                        .cmp(&Self::established_at_key(b.established_at)),
                )
                .then(Self::conn_id_key(a.connection_id).cmp(&Self::conn_id_key(b.connection_id))),
        });
    }

    pub(super) fn build_connection_closure_plan(
        strategy: ConnectionSelectionStrategy,
        selected: &mut [SelectedConnection],
        active_stream_counts: &HashMap<ConnectionId, usize>,
        governance_info: &HashMap<ConnectionId, ConnectionGovernanceInfo>,
        now: SystemTime,
    ) -> Vec<ConnectionClosurePlan> {
        if selected.len() <= 1 {
            return Vec::new();
        }

        Self::sort_selected_connections(strategy, selected);
        let recommended_connection_id = selected[0].connection_id;

        selected
            .iter()
            .skip(1)
            .filter_map(|connection| {
                let reason = format!(
                    "lower-priority-than-connection-{}",
                    recommended_connection_id
                );
                let active_stream_count = active_stream_counts
                    .get(&connection.connection_id)
                    .copied()
                    .unwrap_or(0);
                let current_governance = governance_info.get(&connection.connection_id);

                let action =
                    next_governance_action(current_governance, &reason, active_stream_count, now)?;

                Some(ConnectionClosurePlan {
                    connection_id: connection.connection_id,
                    recommended_connection_id,
                    action,
                })
            })
            .collect()
    }

    fn conn_id_key(id: ConnectionId) -> u64 {
        let serialized = id.to_string();
        if let Ok(value) = serialized.parse::<u64>() {
            return value;
        }

        let digits: String = serialized.chars().filter(|c| c.is_ascii_digit()).collect();
        digits.parse::<u64>().unwrap_or(u64::MAX)
    }

    fn rtt_key(rtt: Option<Duration>) -> u128 {
        rtt.map(|value| value.as_millis()).unwrap_or(u128::MAX)
    }

    fn established_at_key(established_at: Option<SystemTime>) -> Duration {
        established_at
            .and_then(|ts| ts.duration_since(SystemTime::UNIX_EPOCH).ok())
            .unwrap_or(Duration::MAX)
    }

    fn recommended_reason(
        strategy: ConnectionSelectionStrategy,
        selected: &[SelectedConnection],
    ) -> String {
        let Some(recommended) = selected.first() else {
            return "selected-by-policy".to_string();
        };
        let alternatives = &selected[1..];

        match strategy {
            ConnectionSelectionStrategy::PreferDirect
                if !recommended.is_relay
                    && alternatives.iter().any(|candidate| candidate.is_relay) =>
            {
                return "selected-by-prefer-direct".to_string();
            }
            ConnectionSelectionStrategy::PreferRelay
                if recommended.is_relay
                    && alternatives.iter().any(|candidate| !candidate.is_relay) =>
            {
                return "selected-by-prefer-relay".to_string();
            }
            ConnectionSelectionStrategy::PreferLowLatency
                if recommended.last_rtt.is_some()
                    && alternatives.iter().any(|candidate| {
                        Self::rtt_key(recommended.last_rtt) < Self::rtt_key(candidate.last_rtt)
                    }) =>
            {
                return "selected-by-low-latency".to_string();
            }
            _ => {}
        }

        if recommended.active_stream_count > 0
            && alternatives
                .iter()
                .any(|candidate| recommended.active_stream_count > candidate.active_stream_count)
        {
            return "selected-by-active-streams".to_string();
        }

        if recommended.last_rtt.is_some()
            && alternatives.iter().any(|candidate| {
                Self::rtt_key(recommended.last_rtt) < Self::rtt_key(candidate.last_rtt)
            })
        {
            return "selected-by-lower-rtt".to_string();
        }

        if recommended.established_at.is_some()
            && alternatives.iter().any(|candidate| {
                Self::established_at_key(recommended.established_at)
                    < Self::established_at_key(candidate.established_at)
            })
        {
            return "selected-by-earlier-established".to_string();
        }

        match strategy {
            ConnectionSelectionStrategy::PreferDirect => "selected-by-prefer-direct-ordering",
            ConnectionSelectionStrategy::PreferRelay => "selected-by-prefer-relay-ordering",
            ConnectionSelectionStrategy::PreferLowLatency => "selected-by-low-latency-ordering",
        }
        .to_string()
    }
}
