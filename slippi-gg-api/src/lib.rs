use std::borrow::Cow;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::ops::{Deref, DerefMut};
use std::time::Duration;

use serde_json::json;
use ureq::{Agent, AgentBuilder, Resolver};

use dolphin_integrations::Log;

mod graphql;
pub use graphql::{GraphQLBuilder, GraphQLError};

/// Re-export `ureq::Error` for simplicity.
pub type Error = ureq::Error;

/// A DNS resolver that only accepts IPV4 connections.
struct Ipv4Resolver;

impl Resolver for Ipv4Resolver {
    /// Forces IPV4 addresses only.
    fn resolve(&self, netloc: &str) -> io::Result<Vec<SocketAddr>> {
        ToSocketAddrs::to_socket_addrs(netloc).map(|iter| {
            let vec = iter.filter(|s| s.is_ipv4()).collect::<Vec<SocketAddr>>();

            if vec.is_empty() {
                tracing::warn!(
                    target: Log::SlippiOnline,
                    "Failed to get any IPV4 addresses. Does the DNS server support it?"
                );
            }

            vec
        })
    }
}

/// Default timeout that we use on client types. Extracted
/// so that the GraphQLBuilder can also call it.
pub(crate) fn default_timeout() -> Duration {
    Duration::from_millis(5000)
}

/// A wrapper type that simply dereferences to a `ureq::Agent`.
///
/// It's extracted purely for ease of debugging, and for segmenting
/// some initial setup code that would just be cumbersome to do in the
/// core EXI device initialization block.
///
/// Anything that can be called on a `ureq::Agent` can be called on
/// this type. You can also clone this with little cost, and pass it freely
/// to other threads, as it manages itself under the hood with `Arc`.
#[derive(Clone, Debug)]
pub struct APIClient(Agent);

impl APIClient {
    /// Creates and initializes a new APIClient.
    ///
    /// The returned client will only resolve to IPV4 addresses at the moment
    /// due to upstream issues with GCP flex instances and IPV6.
    pub fn new(slippi_semver: &str) -> Self {
        let _build = "unknown";
        let _os = "unknown";

        #[cfg(feature = "mainline")]
        let _build = "mainline";

        #[cfg(feature = "ishiiruka")]
        let _build = "ishiiruka";

        #[cfg(feature = "playback")]
        let _build = "playback";

        #[cfg(target_os = "windows")]
        let _os = "windows";

        #[cfg(target_os = "macos")]
        let _os = "macos";

        #[cfg(target_os = "linux")]
        let _os = "linux";

        // We set `max_idle_connections` to `5` to mimic how CURL was configured in
        // the old C++ logic. This gets cloned and passed down into modules so that
        // the underlying connection pool is shared.
        let http_client = AgentBuilder::new()
            .resolver(Ipv4Resolver)
            .max_idle_connections(5)
            .timeout(default_timeout())
            .user_agent(&format!("SlippiDolphin (v: {slippi_semver}) (b: {_build}) (o: {_os})"))
            .build();

        Self(http_client)
    }

    /// Returns a type that can be used to construct GraphQL requests.
    pub fn graphql<Query>(&self, query: Query) -> GraphQLBuilder
    where
        Query: Into<String>,
    {
        GraphQLBuilder::new(self.clone(), query.into())
    }

    /// Notifies the API server about the status of a match.
    pub fn report_match_status(
        &self,
        uid: &str,
        match_id: &str,
        play_key: &str,
        status: &str
    ) {
        let mutation = r#"
            mutation ($report: OnlineMatchStatusReportInput!) {
                reportOnlineMatchStatus (report: $report)
            }
        "#;

        let variables = json!({
            "report": {
                "matchId": match_id,
                "fbUid": uid,
                "playKey": play_key,
                "status": status,
            }
        });

        match self.graphql(mutation)
            .variables(variables)
            .data_field("/data/reportOnlineMatchStatus")
            .send::<bool>()
        {
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

    /// An asynchronous version of `report_match_status`.
    ///
    /// This spawns a temporary background thread to run the request in. This is not
    /// "async" in typical Rust terms; you (perhaps obviously) do not need to call `.await`
    /// on this.
    pub fn report_match_status_async<StatusString>(
        &self,
        uid: String,
        match_id: String,
        play_key: String,
        status: StatusString
    )
    where
        StatusString: Into<Cow<'static, str>>,
    {
        let api_client = self.clone();
        let status: Cow<'static, str> = status.into();

        let thread = std::thread::Builder::new()
            .name("MatchStatusReq".into())
            .spawn(move || {
                api_client.report_match_status(&uid, &match_id, &play_key, &status);
            });

        if let Err(error) = thread {
            tracing::error!(target: Log::SlippiOnline, ?error, "Unable to spawn request thread");
        }
    }
}

impl Deref for APIClient {
    type Target = Agent;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for APIClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
