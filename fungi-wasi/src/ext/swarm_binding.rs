use super::swarm;

pub struct SwarmBinding {}

impl swarm::Host for SwarmBinding {
    fn peer_id(&mut self) -> String {
        "todo".to_string()
    }
}
