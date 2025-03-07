//! setbac.tv API helpers.

use crate::{
    api::base::RequestBuilder,
    bus,
    injector::Injector,
    oauth2,
    player::{self, Player},
    prelude::*,
    settings::Settings,
    utils,
};
use chrono::{DateTime, Utc};
use failure::Error;
use reqwest::{header, Client, Method, Url};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

const DEFAULT_API_URL: &str = "https://setbac.tv";

/// A token that comes out of a token workflow.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Token {
    /// The client identifier that generated the token.
    pub client_id: String,
    /// Flow that generated the token.
    pub flow_id: String,
    /// Access token.
    pub access_token: String,
    /// When the token was refreshed.
    pub refreshed_at: DateTime<Utc>,
    /// Expires in seconds.
    pub expires_in: Option<u64>,
    /// Scopes associated with token.
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub struct ConnectionMeta {
    pub id: String,
    pub title: String,
    pub description: String,
    pub hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Connection {
    pub id: String,
    pub title: String,
    pub description: String,
    pub hash: String,
    pub token: Token,
}

impl Connection {
    pub fn as_meta(&self) -> ConnectionMeta {
        ConnectionMeta {
            id: self.id.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            hash: self.hash.clone(),
        }
    }
}

impl Token {
    /// The client id that generated the token.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Get the current access token.
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    /// Return `true` if the token expires within 30 minutes.
    pub fn expires_within(&self, within: Duration) -> Result<bool, Error> {
        let out = match self.expires_in.clone() {
            Some(expires_in) => {
                let expires_in = chrono::Duration::seconds(expires_in as i64);
                let diff = (self.refreshed_at + expires_in) - Utc::now();
                diff < chrono::Duration::from_std(within)?
            }
            None => true,
        };

        Ok(out)
    }

    /// Test that token has all the specified scopes.
    pub fn has_scopes(&self, scopes: &[String]) -> bool {
        use hashbrown::HashSet;

        let mut scopes = scopes
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<String>>();

        for s in &self.scopes {
            scopes.remove(s);
        }

        scopes.is_empty()
    }
}

fn parse_url(url: &str) -> Option<Url> {
    match str::parse(url) {
        Ok(api_url) => Some(api_url),
        Err(e) => {
            log::warn!("bad api url: {}: {}", url, e);
            None
        }
    }
}

struct RemoteBuilder {
    token: oauth2::SyncToken,
    injector: Injector,
    global_bus: Arc<bus::Bus<bus::Global>>,
    player: Option<Player>,
    enabled: bool,
    api_url: Option<Url>,
    secret_key: Option<String>,
}

impl RemoteBuilder {
    fn init(&self, remote: &mut Remote) {
        if self.enabled {
            remote.rx = Some(self.global_bus.add_rx());

            remote.player = match self.player.as_ref() {
                Some(player) => Some(player.clone()),
                None => None,
            };
        } else {
            remote.rx = None;
            remote.player = None;
        }

        remote.setbac = match self.api_url.as_ref() {
            Some(api_url) => {
                let setbac =
                    Setbac::new(self.token.clone(), self.secret_key.clone(), api_url.clone());

                self.injector.update(setbac.clone());
                Some(setbac)
            }
            None => {
                self.injector.clear::<Setbac>();
                None
            }
        };
    }
}

#[derive(Default)]
struct Remote {
    rx: Option<bus::Reader<bus::Global>>,
    player: Option<player::Player>,
    setbac: Option<Setbac>,
}

/// Run update loop shipping information to the remote server.
pub fn run(
    settings: &Settings,
    injector: &Injector,
    token: oauth2::SyncToken,
    global_bus: Arc<bus::Bus<bus::Global>>,
) -> Result<impl Future<Output = Result<(), Error>>, Error> {
    let settings = settings.scoped("remote");

    let (mut api_url_stream, api_url) = settings
        .stream("api-url")
        .or(Some(String::from(DEFAULT_API_URL)))
        .optional()?;

    let (mut secret_key_stream, secret_key) = settings.stream("secret-key").optional()?;

    let (mut enabled_stream, enabled) = settings.stream("enabled").or_with(false)?;

    let (mut player_stream, player) = injector.stream::<Player>();

    let mut remote_builder = RemoteBuilder {
        token,
        injector: injector.clone(),
        global_bus,
        player: None,
        enabled: false,
        api_url: None,
        secret_key,
    };

    remote_builder.enabled = enabled;
    remote_builder.player = player;
    remote_builder.api_url = match api_url.and_then(|s| parse_url(&s)) {
        Some(api_url) => Some(api_url),
        None => None,
    };

    let mut remote = Remote::default();
    remote_builder.init(&mut remote);

    Ok(async move {
        loop {
            futures::select! {
                update = secret_key_stream.select_next_some() => {
                    remote_builder.secret_key = update;
                    remote_builder.init(&mut remote);
                }
                update = player_stream.select_next_some() => {
                    remote_builder.player = update;
                    remote_builder.init(&mut remote);
                }
                update = api_url_stream.select_next_some() => {
                    remote_builder.api_url = match update.and_then(|s| parse_url(&s)) {
                        Some(api_url) => Some(api_url),
                        None => None,
                    };

                    remote_builder.init(&mut remote);
                }
                update = enabled_stream.select_next_some() => {
                    remote_builder.enabled = update;
                    remote_builder.init(&mut remote);
                }
                event = remote.rx.select_next_some() => {
                    /// Only update on switches to current song.
                    match event {
                        bus::Global::SongModified => (),
                        _ => continue,
                    };

                    let setbac = match remote.setbac.as_ref() {
                        Some(setbac) => setbac,
                        None => continue,
                    };

                    let player = match remote.player.as_ref() {
                        Some(player) => player,
                        None => continue,
                    };

                    log::trace!("pushing remote player update");

                    let mut update = PlayerUpdate::default();

                    update.current = player.current().map(|c| c.item.into());

                    for i in player.list() {
                        update.items.push(i.into());
                    }

                    if let Err(e) = setbac.player_update(update).await {
                        log::error!("failed to perform remote player update: {}", e);
                    }
                }
            }
        }
    })
}

pub struct Inner {
    client: Client,
    api_url: Url,
    token: oauth2::SyncToken,
    secret_key: Option<String>,
}

/// API integration.
#[derive(Clone)]
pub struct Setbac {
    inner: Arc<Inner>,
}

impl Setbac {
    /// Create a new API integration.
    pub fn new(token: oauth2::SyncToken, secret_key: Option<String>, api_url: Url) -> Self {
        Setbac {
            inner: Arc::new(Inner {
                client: Client::new(),
                api_url,
                token,
                secret_key,
            }),
        }
    }

    /// Get request against API.
    fn request(&self, method: Method, path: &[&str]) -> RequestBuilder {
        let mut url = self.inner.api_url.clone();
        url.path_segments_mut().expect("bad base").extend(path);

        let mut builder = RequestBuilder::new(self.inner.client.clone(), method, url);

        if let Some(secret_key) = self.inner.secret_key.as_ref() {
            builder = builder.header(header::AUTHORIZATION, &format!("key:{}", secret_key));
        } else {
            builder = builder.token(self.inner.token.clone()).use_oauth2_header();
        }

        builder
    }

    /// Update the channel information.
    pub async fn player_update(&self, request: PlayerUpdate) -> Result<(), Error> {
        let body = serde_json::to_vec(&request)?;

        let req = self
            .request(Method::POST, &["api", "player"])
            .header(header::CONTENT_TYPE, "application/json")
            .body(body);

        let _ = req.execute().await?.ok()?;
        Ok(())
    }

    /// Get the token corresponding to the given flow.
    pub async fn get_connection(&self, id: &str) -> Result<Option<Connection>, Error> {
        let req = self
            .request(Method::GET, &["api", "connections", id])
            .header(header::CONTENT_TYPE, "application/json");

        let token = req.execute().await?.json::<Data<Connection>>()?;
        Ok(token.data)
    }

    /// Get the token corresponding to the given flow.
    pub async fn get_connection_meta(
        &self,
        flow_id: &str,
    ) -> Result<Option<ConnectionMeta>, Error> {
        let req = self
            .request(Method::GET, &["api", "connections", flow_id])
            .query_param("format", "meta")
            .header(header::CONTENT_TYPE, "application/json");

        let token = req.execute().await?.json::<Data<ConnectionMeta>>()?;
        Ok(token.data)
    }

    /// Refresh the token corresponding to the given flow.
    pub async fn refresh_connection(&self, id: &str) -> Result<Option<Connection>, Error> {
        let req = self
            .request(Method::POST, &["api", "connections", id, "refresh"])
            .header(header::CONTENT_TYPE, "application/json");

        let token = req.execute().await?.json::<Data<Connection>>()?;
        Ok(token.data)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Data<T> {
    data: Option<T>,
}

impl<T> Default for Data<T> {
    fn default() -> Self {
        Self { data: None }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PlayerUpdate {
    /// Current song.
    #[serde(default)]
    current: Option<Item>,
    /// Songs.
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Item {
    /// Name of the song.
    name: String,
    /// Artists of the song.
    #[serde(default)]
    artists: Option<String>,
    /// Track ID of the song.
    track_id: String,
    /// URL of the song.
    track_url: String,
    /// User who requested the song.
    #[serde(default)]
    user: Option<String>,
    /// Length of the song.
    duration: String,
}

impl From<Arc<player::Item>> for Item {
    fn from(i: Arc<player::Item>) -> Self {
        Item {
            name: i.track.name(),
            artists: i.track.artists(),
            track_id: i.track_id.to_string(),
            track_url: i.track_id.url(),
            user: i.user.clone(),
            duration: utils::compact_duration(&i.duration),
        }
    }
}
