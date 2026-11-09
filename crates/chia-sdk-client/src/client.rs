use std::{
    collections::{HashMap, HashSet},
    fmt,
    net::{IpAddr, SocketAddr},
    ops::Deref,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use chia_protocol::Message;
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::Connector;

use crate::{ClientError, Network, Peer, PeerOptions, connect_peer};

/// Manages connections to multiple full node peers on a network, keyed by IP address.
///
/// The peers and the banned and trusted IP addresses are stored in a [`ClientState`] behind
/// a [`Mutex`]. [`Client`] dereferences to that mutex, so lock it to access them. Clones of a
/// [`Client`] share the same state.
///
/// # Example
///
/// This example is `no_run` because it connects to live mainnet peers.
///
/// ```no_run
/// # #[cfg(feature = "rustls")]
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use std::time::Duration;
///
/// use chia_sdk_client::{Client, Network, PeerOptions, create_rustls_connector, load_ssl_cert};
///
/// let cert = load_ssl_cert("wallet.crt", "wallet.key")?;
/// let connector = create_rustls_connector(&cert)?;
///
/// let network = Network::default_mainnet();
/// let addrs = network.lookup_all(Duration::from_secs(3), 2).await;
/// let client = Client::new("mainnet".to_string(), network, connector);
///
/// for addr in addrs.into_iter().take(3) {
///     if let Err(error) = client.connect(addr, PeerOptions::default()).await {
///         println!("Failed to connect to {addr}: {error}");
///     }
/// }
///
/// for peer in client.lock().await.peers() {
///     println!("Connected to {}", peer.socket_addr());
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Client {
    network_id: String,
    network: Network,
    connector: Connector,
    state: Arc<Mutex<ClientState>>,
}

#[allow(clippy::missing_fields_in_debug)]
impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("network_id", &self.network_id)
            .field("network", &self.network)
            .finish()
    }
}

impl Deref for Client {
    type Target = Mutex<ClientState>;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

/// The connected peers and the banned and trusted IP addresses of a [`Client`].
///
/// Bans don't expire. Trusted IP addresses can't be banned.
///
/// # Example
///
/// ```
/// use std::net::{IpAddr, Ipv4Addr};
///
/// use chia_sdk_client::ClientState;
///
/// let mut state = ClientState::default();
/// let ip_addr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
///
/// assert!(state.ban(ip_addr));
/// assert!(state.is_banned(&ip_addr));
///
/// // Trusting an address also unbans it, and trusted addresses can't be banned.
/// assert!(state.trust(ip_addr));
/// assert!(!state.is_banned(&ip_addr));
/// assert!(!state.ban(ip_addr));
/// ```
#[derive(Debug, Default, Clone)]
pub struct ClientState {
    peers: HashMap<IpAddr, Peer>,
    banned_peers: HashMap<IpAddr, u64>,
    trusted_peers: HashSet<IpAddr>,
}

impl Client {
    /// Creates a client with no peers.
    ///
    /// The `network_id` is the network name sent in the handshake, such as `mainnet` or
    /// `testnet11`, and the `connector` is used for every connection.
    pub fn new(network_id: String, network: Network, connector: Connector) -> Self {
        Self {
            network_id,
            network,
            connector,
            state: Arc::new(Mutex::new(ClientState::default())),
        }
    }

    /// The network name sent in the handshake.
    pub fn network_id(&self) -> &str {
        &self.network_id
    }

    /// The network the client connects to.
    pub fn network(&self) -> &Network {
        &self.network
    }

    /// Connects to a peer with [`connect_peer`], and adds it to the client's peers.
    ///
    /// Returns the receiver for messages from the peer that aren't responses to requests. The
    /// [`Peer`] itself can be found in [`ClientState::peers`].
    ///
    /// If the peer's IP address is banned, the connection is dropped after the handshake and
    /// [`ClientError::BannedPeer`] is returned. If a peer with the same IP address is already
    /// connected, it's replaced.
    pub async fn connect(
        &self,
        socket_addr: SocketAddr,
        options: PeerOptions,
    ) -> Result<mpsc::Receiver<Message>, ClientError> {
        let (peer, receiver) = connect_peer(
            self.network_id.clone(),
            self.connector.clone(),
            socket_addr,
            options,
        )
        .await?;

        let mut state = self.state.lock().await;
        let ip_addr = peer.socket_addr().ip();

        if state.is_banned(&ip_addr) {
            return Err(ClientError::BannedPeer);
        }

        state.peers.insert(peer.socket_addr().ip(), peer);

        Ok(receiver)
    }
}

impl ClientState {
    /// Returns the connected peers, in no particular order.
    pub fn peers(&self) -> impl Iterator<Item = &Peer> {
        self.peers.values()
    }

    /// Removes the peer with the given IP address, and returns whether there was one.
    ///
    /// The connection closes once every clone of the [`Peer`] has been dropped.
    pub fn disconnect(&mut self, ip_addr: &IpAddr) -> bool {
        self.peers.remove(ip_addr).is_some()
    }

    /// Returns whether the IP address is banned.
    pub fn is_banned(&self, ip_addr: &IpAddr) -> bool {
        self.banned_peers.contains_key(ip_addr)
    }

    /// Returns whether the IP address is trusted.
    pub fn is_trusted(&self, ip_addr: &IpAddr) -> bool {
        self.trusted_peers.contains(ip_addr)
    }

    /// Bans the IP address and disconnects its peer, unless the address is trusted.
    ///
    /// Returns `true` if the address wasn't already banned, and `false` if it was already
    /// banned or is trusted.
    pub fn ban(&mut self, ip_addr: IpAddr) -> bool {
        if self.is_trusted(&ip_addr) {
            return false;
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        self.disconnect(&ip_addr);
        self.banned_peers.insert(ip_addr, timestamp).is_none()
    }

    /// Removes the ban on the IP address, and returns whether it was banned.
    pub fn unban(&mut self, ip_addr: IpAddr) -> bool {
        self.banned_peers.remove(&ip_addr).is_some()
    }

    /// Trusts the IP address and removes any ban on it, and returns whether it wasn't already
    /// trusted.
    pub fn trust(&mut self, ip_addr: IpAddr) -> bool {
        let result = self.trusted_peers.insert(ip_addr);
        self.banned_peers.remove(&ip_addr);
        result
    }

    /// Stops trusting the IP address, and returns whether it was trusted.
    pub fn untrust(&mut self, ip_addr: IpAddr) -> bool {
        self.trusted_peers.remove(&ip_addr)
    }
}
