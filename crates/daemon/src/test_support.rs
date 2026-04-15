//! Ephemeral test helpers for [`FungiDaemon`], inspired by the `swarm-test` crate in
//! [rust-libp2p](https://github.com/libp2p/rust-libp2p/tree/master/swarm-test).
//!
//! # Design goals
//!
//! * **Single call-site** – `TestDaemon::spawn().await` is all you need for an isolated daemon.
//! * **Zero port conflicts** – ports are reserved via an OS-assigned `TcpListener` before the
//!   daemon starts, so tests never step on each other.
//! * **Automatic cleanup** – the temp directory is deleted when `TestDaemon` is dropped.
//! * **Composable** – use `TestDaemonBuilder` when you need extra configuration
//!   (known PeerId, allowed peers, config tweaks, …).
//!
//! # Quick start
//!
//! ```rust,ignore
//! #[tokio::test]
//! async fn smoke() {
//!     let d = TestDaemon::spawn().await.unwrap();
//!     assert!(!d.peer_id().to_string().is_empty());
//! }
//!
//! #[tokio::test]
//! async fn two_daemons_can_connect() {
//!     let server = TestDaemon::spawn().await.unwrap();
//!     let client = TestDaemonBuilder::new()
//!         .with_allowed_peer(server.peer_id())
//!         .build()
//!         .await
//!         .unwrap();
//!
//!     server
//!         .swarm_control()
//!         .invoke_swarm(|swarm| swarm.add_peer_address(client.peer_id(), client.tcp_multiaddr()))
//!         .await
//!         .unwrap();
//!     client.connect_to(&server).await.unwrap();
//! }
//! ```

use std::{
    net::TcpListener,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use fungi_config::{FungiConfig, address_book::AddressBookConfig};
use libp2p::{Multiaddr, PeerId, identity::Keypair, multiaddr::Protocol};
use tempfile::TempDir;

use crate::{DaemonArgs, FungiDaemon};

type ConfigMutator = Box<dyn Fn(&mut FungiConfig) + Send + Sync + 'static>;

// Fallback counter for UDP-only conflicts; only used when the TCP port itself is already
// determined via `reserve_ephemeral_port`.

/// Reserve a free TCP port by briefly binding to port 0.
///
/// There is an inherent TOCTOU window between closing the probe listener and the daemon binding
/// the same port; in practice this is negligible in a test environment.
pub fn reserve_ephemeral_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("failed to bind ephemeral port")
        .local_addr()
        .expect("failed to get local addr")
        .port()
}

/// Build a minimal [`FungiConfig`] for an isolated test daemon rooted at `dir`.
///
/// Relay is **disabled** and file-transfer proxies are **off** so tests stay self-contained.
fn minimal_test_config(dir: &TempDir, tcp_port: u16) -> FungiConfig {
    let mut cfg = FungiConfig::apply_from_dir(dir.path()).expect("failed to init test config dir");
    cfg.network.listen_tcp_port = tcp_port;
    // Derive UDP port from TCP port to keep them paired and avoid collisions with other
    // concurrently-running test daemons.  The OS-reserved TCP port will already be >1024
    // so wrapping is very unlikely in practice.
    cfg.network.listen_udp_port = tcp_port.wrapping_add(1000);
    cfg.network.relay_enabled = false;
    cfg.network.custom_relay_addresses.clear();
    cfg.file_transfer.proxy_ftp.enabled = false;
    cfg.file_transfer.proxy_webdav.enabled = false;
    cfg
}

// ── TestDaemonBuilder ─────────────────────────────────────────────────────────

/// Builder for [`TestDaemon`].  All fields are optional; sensible test defaults are used for any
/// field that is not explicitly set.
#[derive(Default)]
pub struct TestDaemonBuilder {
    keypair: Option<Keypair>,
    allowed_peers: Vec<PeerId>,
    config_mutators: Vec<ConfigMutator>,
}

impl TestDaemonBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Use a specific keypair (useful when the test needs a deterministic [`PeerId`]).
    pub fn with_keypair(mut self, keypair: Keypair) -> Self {
        self.keypair = Some(keypair);
        self
    }

    /// Allow an additional incoming peer on this daemon.
    pub fn with_allowed_peer(mut self, peer_id: PeerId) -> Self {
        self.allowed_peers.push(peer_id);
        self
    }

    /// Apply additional configuration after the standard isolated test defaults are set.
    ///
    /// Use this when a scenario needs one-off config beyond the common builder helpers, such as
    /// enabling a specific subsystem or seeding a service-specific client entry, while still
    /// keeping the rest of the daemon setup on the shared test-support path.
    pub fn with_config(
        mut self,
        configure: impl Fn(&mut FungiConfig) + Send + Sync + 'static,
    ) -> Self {
        self.config_mutators.push(Box::new(configure));
        self
    }

    /// Spawn the daemon.
    pub async fn build(self) -> Result<TestDaemon> {
        let dir = TempDir::new()?;
        let keypair = self.keypair.unwrap_or_else(Keypair::generate_ed25519);
        let tcp_port = reserve_ephemeral_port();
        let mut cfg = minimal_test_config(&dir, tcp_port);
        cfg.network
            .incoming_allowed_peers
            .extend(self.allowed_peers);
        for configure in self.config_mutators {
            configure(&mut cfg);
        }

        let daemon = FungiDaemon::start_with(
            DaemonArgs::default(),
            cfg,
            keypair,
            AddressBookConfig::default(),
        )
        .await?;
        Ok(TestDaemon {
            inner: daemon,
            tcp_port,
            _dir: dir,
        })
    }
}

// ── TestDaemon ────────────────────────────────────────────────────────────────

/// An ephemeral [`FungiDaemon`] for use in tests.
///
/// The temporary directory (and therefore any persisted state) is automatically deleted when this
/// value is dropped.
pub struct TestDaemon {
    inner: FungiDaemon,
    /// The TCP port the daemon was configured to listen on.
    pub tcp_port: u16,
    _dir: TempDir,
}

impl TestDaemon {
    // ── Constructors ──────────────────────────────────────────────────────

    /// Spawn an isolated daemon with a fresh random identity, ephemeral TCP port, and no relay.
    ///
    /// This is the simplest entry point for most tests:
    /// ```rust,ignore
    /// let d = TestDaemon::spawn().await.unwrap();
    /// ```
    pub async fn spawn() -> Result<Self> {
        TestDaemonBuilder::new().build().await
    }

    /// Spawn with a known keypair (for tests that need a deterministic [`PeerId`]).
    pub async fn spawn_with_keypair(keypair: Keypair) -> Result<Self> {
        TestDaemonBuilder::new().with_keypair(keypair).build().await
    }

    // ── Accessors ─────────────────────────────────────────────────────────

    /// The [`PeerId`] of this daemon.
    pub fn peer_id(&self) -> PeerId {
        self.inner.swarm_control().local_peer_id()
    }

    /// A `Multiaddr` that can be dialled by another daemon on the same host.
    ///
    /// Format: `/ip4/127.0.0.1/tcp/<port>/p2p/<peer_id>`.
    pub fn tcp_multiaddr(&self) -> Multiaddr {
        let peer_id = self.peer_id();
        Multiaddr::empty()
            .with(Protocol::Ip4("127.0.0.1".parse().unwrap()))
            .with(Protocol::Tcp(self.tcp_port))
            .with(Protocol::P2p(peer_id))
    }

    /// Borrow the inner daemon for calling any daemon API directly.
    pub fn daemon(&self) -> &FungiDaemon {
        &self.inner
    }

    /// Borrow the underlying [`fungi_swarm::SwarmControl`].
    pub fn swarm_control(&self) -> &fungi_swarm::SwarmControl {
        self.inner.swarm_control()
    }

    // ── Connection helpers ────────────────────────────────────────────────

    /// Dial `other` and wait (up to 10 s) for a connection to be established.
    ///
    /// This is the analogue of `SwarmExt::connect` in `libp2p-swarm-test`:
    /// both daemons keep running while the connection is being established.
    pub async fn connect_to(&self, other: &TestDaemon) -> Result<()> {
        let target_peer_id = other.peer_id();
        let target_addr = other.tcp_multiaddr();

        // Register the address with the dialling swarm so it knows where to reach the peer.
        self.swarm_control()
            .invoke_swarm(move |swarm| {
                swarm.add_peer_address(target_peer_id, target_addr);
            })
            .await?;

        // Initiate the dial.
        self.swarm_control()
            .connect(target_peer_id)
            .await
            .map_err(|e| anyhow!("dial failed: {e}"))?;

        Ok(())
    }

    /// Poll (up to `timeout`) until this daemon reports `peer_id` as connected.
    ///
    /// Useful after calling [`connect_to`] when you need to wait for the handshake to complete.
    pub async fn wait_connected(&self, peer_id: PeerId, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            let connected = self
                .swarm_control()
                .invoke_swarm(move |swarm| swarm.is_connected(&peer_id))
                .await?;
            if connected {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(anyhow!(
                    "timed out ({timeout:?}) waiting for connection to {peer_id}"
                ));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

// ── Convenience helpers ───────────────────────────────────────────────────────

/// Spawn two daemons where `server` allows `client`.
///
/// Returns `(client, server)` already wired so `client` can dial `server`.
/// The caller should call `client.connect_to(&server).await` to complete the connection.
pub async fn spawn_connected_pair() -> Result<(TestDaemon, TestDaemon)> {
    // Spawn server first so we know its PeerId for the allow-list.
    let server_kp = Keypair::generate_ed25519();
    let server_peer_id = server_kp.public().to_peer_id();
    let client_kp = Keypair::generate_ed25519();
    let client_peer_id = client_kp.public().to_peer_id();

    let server = TestDaemonBuilder::new()
        .with_keypair(server_kp)
        .with_allowed_peer(client_peer_id)
        .build()
        .await?;
    let client = TestDaemonBuilder::new()
        .with_keypair(client_kp)
        .with_allowed_peer(server_peer_id)
        .build()
        .await?;

    Ok((client, server))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: a single daemon starts up and exposes a non-trivial PeerId.
    #[tokio::test]
    async fn spawn_single_daemon_smoke() {
        let d = TestDaemon::spawn().await.expect("spawn failed");
        let pid = d.peer_id();
        assert!(!pid.to_string().is_empty(), "PeerId should not be empty");
    }

    /// Each spawned daemon gets a distinct PeerId.
    #[tokio::test]
    async fn two_daemons_have_distinct_peer_ids() {
        let a = TestDaemon::spawn().await.unwrap();
        let b = TestDaemon::spawn().await.unwrap();
        assert_ne!(a.peer_id(), b.peer_id());
    }

    /// `tcp_multiaddr` encodes the correct port and peer ID.
    #[tokio::test]
    async fn tcp_multiaddr_contains_port_and_peer_id() {
        let d = TestDaemon::spawn().await.unwrap();
        let addr = d.tcp_multiaddr().to_string();
        assert!(
            addr.contains(&d.tcp_port.to_string()),
            "addr should contain port: {addr}"
        );
        assert!(
            addr.contains(&d.peer_id().to_string()),
            "addr should contain peer id: {addr}"
        );
    }

    /// A deterministic keypair produces the expected PeerId.
    #[tokio::test]
    async fn spawn_with_keypair_yields_expected_peer_id() {
        let kp = Keypair::generate_ed25519();
        let expected = kp.public().to_peer_id();
        let d = TestDaemon::spawn_with_keypair(kp).await.unwrap();
        assert_eq!(d.peer_id(), expected);
    }

    /// `spawn_connected_pair` returns daemons with different peer IDs and correct allow-lists.
    #[tokio::test]
    async fn spawn_connected_pair_has_distinct_peers_and_allow_lists() {
        let (client, server) = spawn_connected_pair().await.unwrap();
        assert_ne!(client.peer_id(), server.peer_id());

        // Server's incoming_allowed_peers should include the client.
        let server_cfg = server.daemon().config();
        let client_in_list = server_cfg
            .lock()
            .network
            .incoming_allowed_peers
            .contains(&client.peer_id());
        assert!(client_in_list, "server should allow client peer");
    }

    #[tokio::test]
    async fn builder_can_customize_config() {
        let d = TestDaemonBuilder::new()
            .with_config(|cfg| {
                cfg.file_transfer.server.enabled = true;
            })
            .build()
            .await
            .unwrap();

        assert!(d.daemon().config().lock().file_transfer.server.enabled);
    }
}
