use std::borrow::Cow;
use std::net::UdpSocket;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};

use rusty_enet::{Event, Host, HostSettings, Packet, PacketKind};
use rusty_enet::error::{HostNewError, NoAvailablePeers};
use thiserror::Error;

use dolphin_integrations::Log;
use slippi_gg_api::APIClient;
use slippi_shared_types::{AtomicState, OnceValue, OnlinePlayMode};
use slippi_user::UserManager;

use crate::NetplayState;
use crate::context::{MatchContext, Player, PlayerRank, Stage};

const MM_HOST_DEV: &str = "mm2.slippi.gg";
const MM_HOST_PROD: &str = "mm.slippi.gg";
const MM_PORT: u16 = 43113;

const CREATE_TICKET: &str = "create-ticket";
const CREATE_TICKET_RESP: &str = "create-ticket-resp";
const GET_TICKET_RESP: &str = "get-ticket-resp";

/// Various settings used by the matchmaking server for pairing players up.
#[derive(Clone, Debug)]
pub struct MatchSearchSettings {
    pub mode: OnlinePlayMode,
    pub connect_code: String
}

/// The core matchmaking operation.
///
/// This should always be called on a background thread. If it successfully finds a match
/// before erroring out (or before the player cancels the flow) then it will spawn another
/// thread for netplay communication and "link" the main thread to it via the provided 
/// channel receiver endpoint.
pub fn run(
    state: AtomicState<NetplayState>,
    match_context: OnceValue<MatchContext>,
    error_message: OnceValue<Cow<'static, str>>,
    scm_ver: String,
    api_client: APIClient,
    user_manager: UserManager,
    search: MatchSearchSettings
) {
    let mm_host = match scm_ver.contains("dev") {
        true => MM_HOST_DEV,
        false => MM_HOST_PROD
    };

    let mut host = None;
    let mut context = MatchContext::default();

    loop {
        match state.get() {
            NetplayState::Initializing => {
                match submit_ticket(mm_host, &user_manager, &search, &scm_ver) {
                    Ok(enet_host) => {
                        host = Some(enet_host);
                        state.set(NetplayState::Matchmaking);
                    },

                    Err(error) => {
                        tracing::error!(target: Log::SlippiOnline, ?error, "Matchmaking init failure");
                        set_init_error(error_message, error);
                        state.set(NetplayState::ErrorEncountered);
                        return;
                    }
                }
            },

            NetplayState::Matchmaking => {
                // This is unlikely to ever happen and mostly exists as a sanity check.
                if host.is_none() {
                    tracing::error!(target: Log::SlippiOnline, "Missing enet host in matchmaking");
                    error_message.set("Missing host".into());
                    state.set(NetplayState::ErrorEncountered);
                    return;
                }

                match check_ticket(host.as_mut().unwrap(), &user_manager) {
                    Ok(Some(ctx)) => {
                        context = ctx;
                        state.set(NetplayState::OpponentConnecting);
                    },

                    Ok(None) => {
                        tracing::info!(target: Log::SlippiOnline, "No match assigned yet");
                    },

                    Err(error) => {
                        tracing::error!(target: Log::SlippiOnline, ?error, "Matchmaking failure");
                        set_matchmake_error(error_message, error);
                        state.set(NetplayState::ErrorEncountered);
                        return;
                    }
                };
            },

            _ => { break; }
        }
    }

    if let Some(host) = host.take() {
        terminate_connection(host);
    }

    // If ranked, report to the backend that we are attempting to connect to this match.
    if context.id.contains("mode.ranked") {
        report_connection_attempt(&api_client, &user_manager, &context.id);
    }

    // If we get here, we've got a valid match and we're good to go.
    // Store the context in the provided slot, and spin up the Netplay thread.
    //
    // This thread will die off now and any resources can wither away.
    match_context.set(context);

    // Spin up netplay thread
}

/// Reports a connection attempt. This should only be called in Ranked.
fn report_connection_attempt(api_client: &APIClient, user_manager: &UserManager, match_id: &str) {
    let (uid, play_key) = user_manager.get(|user| (user.uid.clone(), user.play_key.clone()));
    let status = "connecting";

    match api_client.report_match_status(&uid, &match_id, &play_key, status) {
        Ok(value) if value => {
            tracing::info!(
                target: Log::SlippiOnline,
                "Executed status report request: {status}"
            );
        },

        Ok(value) => {
            tracing::error!(
                target: Log::SlippiOnline,
                ?value,
                "Failed status report request: {status}"
            );
        },

        Err(error) => {
            tracing::error!(
                target: Log::SlippiOnline,
                ?error,
                "Error executing status report request: {status}"
            );
        }
    }
}

/// Attempts to terminate the connection by gracefully disconnecting peers. If peers
/// do not appear to disconnect, this will force disconnects after around 3000ms.
fn terminate_connection(mut host: Host<UdpSocket>) {
    for peer in host.peers_mut() {
        peer.disconnect(0);
    }

    let timeout = 3000;
    let mut slept = 0;

    while slept <= timeout {
        // If we receive a Disconnect, then we can bail early and let the `Drop` impl
        // on `Host` handle cleaning up resources.
        if let Ok(Some(Event::Disconnect { peer: _, data: _ })) = host.service() {
            return;
        }

        std::thread::sleep(std::time::Duration::from_millis(250));
        slept += 250;
    }

    // If we didn't receive a Disconnect event, then we need to force disconnect
    // everything. When the `host` is dropped at the end of this function it will
    // trigger `enet_destroy` behind the scenes.
    for peer in host.peers_mut() {
        peer.reset();
    }
}

#[derive(Debug, Error)]
enum ReceiveError {
    #[error(transparent)]
    HostRead(std::io::Error),

    #[error(transparent)]
    Deserialize(serde_json::Error),

    #[error("Matchmaking server disconnected")]
    Disconnect,

    #[error("No response from matchmaking server")]
    Timeout,

    #[error(transparent)]
    Utf8Read(std::str::Utf8Error)
}

/// Repeatedly checks the inner socket for new data. We will attempt to deserialize any data
/// received to our expected type.
///
/// This attempts to replicate the timeout handling of the C++ version, albeit against what
/// appears to be a newer/different enet API. For the way this is called, it's not a
/// significant burden to just chunk the timeout checking manually 
/// (e.g 5000ms in 250ms chunks, etc).
fn receive<T>(host: &mut Host<UdpSocket>, mut timeout_ms: i32) -> Result<T, ReceiveError>
where
    T: serde::de::DeserializeOwned,
{
    let host_service_timeout_ms = 250;

    // Make sure loop runs at least once
    if timeout_ms < host_service_timeout_ms {
        timeout_ms = host_service_timeout_ms;
    }

    // This is not a perfect way to timeout but hopefully it's close enough?
    let max_attempts = timeout_ms / host_service_timeout_ms;
    
    let mut attempt = 0;

    while attempt < max_attempts {
        if let Some(event) = host.service().map_err(ReceiveError::HostRead)? {
            if let Event::Disconnect { .. } = event {
                return Err(ReceiveError::Disconnect);
            }

            if let Event::Receive { peer: _, channel_id: _, packet } = event {
                let message = str::from_utf8(packet.data()).map_err(ReceiveError::Utf8Read)?;
                let data = serde_json::from_str(message).map_err(ReceiveError::Deserialize)?;
                return Ok(data);
            }
        }

        attempt += 1;
        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    Err(ReceiveError::Timeout)
}

#[derive(Debug, Error)]
enum ConnectError {
    #[error(transparent)]
    ServerLookup(std::io::Error),

    #[error("Failed to determine socket addrs for matchmaking server url")]
    NoValidServerAddr,

    #[error(transparent)]
    SocketBind(std::io::Error),

    #[error(transparent)]
    SocketPortCheck(std::io::Error),

    #[error(transparent)]
    HostNew(HostNewError<UdpSocket>),

    #[error(transparent)]
    NoAvailablePeers(NoAvailablePeers),

    #[error(transparent)]
    HostRead(std::io::Error),

    #[error("Failed to connect to matchmaking server")]
    UnableToConnect
}

/// Creates a new enet host client, connected to the matchmaking server and ready for
/// further usage.
fn connect_to_mm(mm_host: &str) -> Result<(SocketAddr, Host<UdpSocket>, u16), ConnectError> {
    // There's no sense in doing anything further if we can't resolve the socket addr 
    // for the matchmaking server.
    let mm_socket_addr = (mm_host, MM_PORT)
        .to_socket_addrs()
        .map_err(ConnectError::ServerLookup)?
        .next()
        .ok_or_else(|| ConnectError::NoValidServerAddr)?;

    let addr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));

    let get_dolphin_custom_netplay_port: fn() -> Option<u16> = || { None };

    // Generate a list of addresses & ports to try. 
    // 
    // We are explicitly trying a slew of client addresses because we are trying to utilize
    // our connection to the matchmaking service in order to hole punch. Whichever port works
    // will end up being the port we listen on when we start our server.
    let addr_attempts = match get_dolphin_custom_netplay_port() {
        Some(port) => vec![SocketAddr::new(addr, port)],
        
        None => (0..15).into_iter().map(|i| {
            // 41000 + (generator() % 10000);
            SocketAddr::new(addr, 41000 + i)
        }).collect()
    };
    
    tracing::info!(target: Log::SlippiOnline, ?addr_attempts);

    let socket = UdpSocket::bind(&*addr_attempts).map_err(ConnectError::SocketBind)?;
    let port = socket.local_addr().map_err(ConnectError::SocketPortCheck)?.port();

    let mut host = Host::new(socket, HostSettings {
        peer_limit: 1,
        channel_limit: 3,
        incoming_bandwidth_limit: None,
        outgoing_bandwidth_limit: None,
        ..Default::default()
    }).map_err(ConnectError::HostNew)?;

    host.connect(mm_socket_addr, 3, 0).map_err(ConnectError::NoAvailablePeers)?;

    // Listen to the host for a short period so we can make sure we're properly connected.
    let mut attempt = 0;
    let max_attempts = 20;
    loop {
        match host.service().map_err(ConnectError::HostRead)? {
            Some(Event::Connect { .. }) => {
                // is_mm_connected = true;
                return Ok((mm_socket_addr, host, port));
            },

            Some(event) => {
                tracing::warn!(
                    target: Log::SlippiOnline,
                    ?event,
                    "Received unexpected event in client initialization"
                );
            },

            None => {}
        }

        if attempt == max_attempts {
            return Err(ConnectError::UnableToConnect);
        }

        attempt += 1;
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Determine local IP address. We can attempt to connect to our opponent via
/// local IP address if we have the same external IP address. The following
/// scenarios can cause us to have the same external IP address:
///
/// - we are connected to the same LAN
/// - we are connected to the same VPN node
/// - we are behind the same CGNAT
fn determine_lan_addr(mm_addr: SocketAddr, port: u16) -> Result<String, std::io::Error> {
    let get_dolphin_custom_lan_ip: fn() -> Option<String> = || { None };

    match get_dolphin_custom_lan_ip() {
        Some(addr) => {
            tracing::warn!(target: Log::SlippiOnline, "Overwriting LAN IP with custom address");
            Ok(format!("{addr}:{port}"))
        },

        None => {
            let socket = UdpSocket::bind("0.0.0.0:0")?;
            socket.connect(mm_addr)?;

            let local_addr = socket.local_addr()?.ip();
            Ok(format!("{local_addr}:{port}"))
        }
    }
}

/// Any errors that can occur during the ticket submission process.
#[derive(Debug, Error)]
enum SubmitTicketError {
    #[error(transparent)]
    Connect(ConnectError),

    #[error(transparent)]
    LanAddrLookup(std::io::Error),

    #[error(transparent)]
    InvalidBody(serde_json::Error),

    #[error(transparent)]
    Receive(ReceiveError),

    #[error("Invalid response type from server: {0}")]
    InvalidResponse(String),

    #[error("Error from server: {0}")]
    Server(String)
}

fn set_init_error(error_message: OnceValue<Cow<'static, str>>, error: SubmitTicketError) {
    error_message.set(match error {
        SubmitTicketError::Connect(error) => match error {
            ConnectError::ServerLookup(_) => "Failed to find mm server".into(),
            ConnectError::NoValidServerAddr => "Failed to route to mm server".into(),
            
            ConnectError::HostNew(_) |
            ConnectError::SocketBind(_) |
            ConnectError::SocketPortCheck(_) => "Failed to create mm client".into(),

            ConnectError::NoAvailablePeers(_) |
            ConnectError::HostRead(_) |
            ConnectError::UnableToConnect => "Failed to start connection to mm server".into()
        },

        SubmitTicketError::LanAddrLookup(_) => "Unable to determine IP addr".into(),
        SubmitTicketError::InvalidBody(_) => "Failed to submit to mm queue".into(),
        SubmitTicketError::Receive(_) => "Failed to join mm queue".into(),
        SubmitTicketError::InvalidResponse(_) => "Invalid response from mm queue".into(),
        SubmitTicketError::Server(error) => error.into()
    });
}

/// The response payload format we expect from successful ticket submission.
#[derive(Debug, serde::Deserialize)]
struct SubmitTicketResponse {
    #[serde(alias = "type")]
    kind: String,

    error: Option<String>
}

/// Submits a match ticket to the matchmaking server.
fn submit_ticket(
    mm_host: &str,
    user_manager: &UserManager,
    search: &MatchSearchSettings,
    app_version: &str
) -> Result<Host<UdpSocket>, SubmitTicketError> {
    let (mm_socket_addr, mut host, selected_network_port) = connect_to_mm(mm_host)
        .map_err(SubmitTicketError::Connect)?;
    
    let lan_addr = determine_lan_addr(mm_socket_addr, selected_network_port)
        .map_err(SubmitTicketError::LanAddrLookup)?;

    let (uid, play_key, connect_code, display_name) = user_manager.get(|user| {
        (user.uid.clone(), user.play_key.clone(), user.connect_code.clone(), user.display_name.clone())
    });

    let request = serde_json::json!({
        "type": CREATE_TICKET,
        "user": {
            "uid": uid,
            "playKey": play_key,
            "connectCode": connect_code,
            "displayName": display_name
        },
        "search": {
            "mode": search.mode,
            "connectCode": search.connect_code
        },
        "appVersion": app_version,
        "ipAddressLan": lan_addr
    });

    let request_body = serde_json::to_string(&request)
        .map_err(SubmitTicketError::InvalidBody)?;

    tracing::info!(target: Log::SlippiOnline, ticket_request = ?request_body);

    let packet = Packet::new(request_body.as_bytes(), PacketKind::Reliable);
    let channel_id = 0;
    host.broadcast(channel_id, &packet);

    let response: SubmitTicketResponse = receive(&mut host, 5000)
        .map_err(SubmitTicketError::Receive)?;

    tracing::info!(target: Log::SlippiOnline, ticket_response = ?response);

    if response.kind != CREATE_TICKET_RESP {
        return Err(SubmitTicketError::InvalidResponse(response.kind));
    }

    if let Some(error) = response.error {
        return Err(SubmitTicketError::Server(error));
    }

    Ok(host)
}

#[derive(Debug, Error)]
enum CheckTicketError {
    #[error(transparent)]
    Receive(ReceiveError),

    #[error("Invalid response type from server: {0}")]
    InvalidResponse(String),

    #[error("Error from server: {0}")]
    Server(String),

    #[error(transparent)]
    InvalidAddr(std::net::AddrParseError)
}

fn set_matchmake_error(error_message: OnceValue<Cow<'static, str>>, error: CheckTicketError) {
    error_message.set(match error {
        CheckTicketError::Receive(ReceiveError::Disconnect) => "Lost connection to the mm server".into(),
        CheckTicketError::Receive(_) => "Failed to receive mm status".into(),
        CheckTicketError::InvalidResponse(_) => "Invalid response when getting mm status".into(),
        CheckTicketError::Server(error) => error.into(),
        CheckTicketError::InvalidAddr(_) => "Invalid response from mm".into()
    });
}
	
#[derive(Debug, serde::Deserialize)]
struct PlayerInfo {
    #[serde(alias = "isLocalPlayer")]
    is_local: bool,

    uid: String,

    #[serde(alias = "displayName")]
    display_name: String,

    #[serde(alias = "connectCode")]
    connect_code: String,

    port: usize,

    #[serde(alias = "ipAddress")]
    ip_address: String,

    #[serde(alias = "ipAddressLan")]
    ip_address_lan: Option<String>,

    #[serde(alias = "isBot")]
    is_bot: bool,

    #[serde(alias = "chatMessages")]
    chat_messages: Vec<String>,

    #[serde(default)]
    rank: PlayerRank
}

#[derive(Debug, serde::Deserialize)]
struct TicketResponse {
    #[serde(alias = "type")]
    pub kind: String,
    
    #[serde(alias = "latestVersion", default)]
    pub latest_version: String,

    #[serde(alias = "matchId", default)]
    pub match_id: String,

    pub error: Option<String>,

    #[serde(default)]
    pub players: Vec<PlayerInfo>,

    #[serde(default)]
    pub stages: Vec<u16>,

    #[serde(alias = "isHost")]
    pub is_host: bool
}

/// Checks for a matchmaking response. If one is available, this will then
/// handle extracting information and returning it as `MatchContext`.
///
/// If this returns `None`, it just means there's no response available yet.
fn check_ticket(
    host: &mut Host<UdpSocket>,
    user_manager: &UserManager
) -> Result<Option<MatchContext>, CheckTicketError> {
    let response = receive::<TicketResponse>(host, 2000);

    // A timeout isn't an error to raise here; it just means we don't have an
    // assigned match yet and should check back in a short bit.
    if let Err(ReceiveError::Timeout) = response {
        return Ok(None);
    }

    let mut response = response.map_err(CheckTicketError::Receive)?;
    
    tracing::info!(target: Log::SlippiOnline, mm_response = ?response);

    if response.kind != GET_TICKET_RESP {
        return Err(CheckTicketError::InvalidResponse(response.kind));
    }

    if let Some(error) = response.error {
        // Update version number when the mm server tells us our version is outdated
        // Force latest version for people whose file updates dont work
        //
        // (@TODO: Is this even still necessary...?)
        if response.latest_version != "" {
            let latest_version = std::mem::take(&mut response.latest_version);
            user_manager.overwrite_latest_version(latest_version);
        }

        return Err(CheckTicketError::Server(error));
    }

    tracing::warn!(target: Log::SlippiOnline, match_id = ?response.match_id);

    let mut context = MatchContext::default();
    context.id = response.match_id;
    context.is_host = response.is_host;

    // This is a socket address that will never actually be used; the API guarantees that we'll
    // overwrite this value after we find the `is_local` player. It's just slightly nicer
    // ergonomics-wise than dealing with an `Option` here.
    let mut local_external_ip: SocketAddr = ([0; 4], 0).into();

    for player in response.players.iter_mut() {
        if player.is_local {
            local_external_ip = player.ip_address.parse().map_err(CheckTicketError::InvalidAddr)?;
            context.local_player_index = (player.port - 1) as usize;
        }

        let mut chat_messages = std::mem::take(&mut player.chat_messages);
        if chat_messages.len() != 16 {
            chat_messages = slippi_user::chat::default();
        }

        context.players.push(Player {
            uid: std::mem::take(&mut player.uid),
            display_name: std::mem::take(&mut player.display_name),
            connect_code: std::mem::take(&mut player.connect_code),
            chat_messages,
            rank: player.rank,
            is_bot: player.is_bot,
            port: player.port
        });
    }
    
    // Loop a second time to ensure we have the correct IPs.
    //
    // Note that this is translated more or less verbatim from the older C++ code; I assume
    // that there's an important reason for doing it this way and the extra loop isn't the end
    // of the world.
    for player in response.players.into_iter() {
        if (player.port - 1) as usize == context.local_player_index {
            continue;
        }

        let addr: SocketAddr = player
            .ip_address
            .as_str()
            .parse()
            .map_err(CheckTicketError::InvalidAddr)?;

        // @TODO: Under what circumstances could `addr` _match_ `local_external_ip`? Something
        // about this logic feels weird to me - like there's a very small window where an address
        // could not be pushed to the remote_addrs?
        if addr.ip() != local_external_ip.ip() || player.ip_address_lan.is_none() {
            context.remote_addrs.push(addr);
            continue;
        }

        // If external IPs are the same, try using LAN IPs
        // TODO: Instead of using one or the other, it might be better to try both
        if let Some(lan_addr) = player.ip_address_lan {
            let addr: SocketAddr = lan_addr
                .as_str()
                .parse()
                .map_err(CheckTicketError::InvalidAddr)?;

            context.remote_addrs.push(addr);
        }
    }

    for value in response.stages.into_iter() {
        if let Some(stage) = Stage::from(value) {
            context.stages.push(stage);
        } else {
            tracing::warn!(target: Log::SlippiOnline, "Received unknown stage value: {}", value);
        }
    }
    
    // Shouldn't happen, but here just in case.
    if context.stages.is_empty() {
        context.stages = vec![
            Stage::PokemonStadium,
            Stage::YoshisStory,
            Stage::Dreamland,
            Stage::Battlefield,
            Stage::FinalDestination
        ];

        // If singles, FoD should be allowed.
        if context.players.len() == 2 {
            context.stages.push(Stage::FountainOfDreams);
        }
    }

    Ok(Some(context))
}
