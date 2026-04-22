mod control;
mod dial_plan;
mod governance;
mod relay;
mod runtime;
#[cfg(test)]
mod tests;
mod types;

use crate::behaviours::FungiBehaviours;
use libp2p::Swarm;

pub use control::{ConnectError, SwarmAsyncCall, SwarmControl};
pub use relay::{get_default_relay_addrs, peer_addr_with_relay};
pub use runtime::FungiSwarm;
pub(crate) use types::ConnectionRecordSliceExt;
pub use types::ConnectionSelectionStrategy;

pub type TSwarm = Swarm<FungiBehaviours>;
