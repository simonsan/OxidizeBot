//! Stream currency configuration.
use crate::{api, db::Database};
pub use crate::{db::models::Balance, utils::Duration};
use failure::Error;
use hashbrown::HashSet;
use std::sync::Arc;

mod builtin;
mod mysql;

/// Balance of a single user.
#[derive(Default)]
pub struct BalanceOf {
    pub balance: i64,
    pub watch_time: i64,
}

impl BalanceOf {
    /// Get the current watch time for the specified balance as a duration.
    pub fn watch_time(&self) -> Duration {
        if self.watch_time < 0 {
            return Duration::default();
        }

        Duration::seconds(self.watch_time as u64)
    }
}

/// Helper struct to construct a currency.
pub struct CurrencyBuilder {
    pub ty: BackendType,
    pub enabled: bool,
    pub command_enabled: bool,
    pub name: Option<Arc<String>>,
    pub db: Option<Database>,
    pub twitch: api::Twitch,
    pub mysql_url: Option<String>,
    pub mysql_schema: mysql::Schema,
}

impl CurrencyBuilder {
    /// Construct a new currency builder.
    pub fn new(twitch: api::Twitch, mysql_schema: mysql::Schema) -> Self {
        Self {
            ty: Default::default(),
            enabled: Default::default(),
            command_enabled: Default::default(),
            name: Default::default(),
            db: None,
            twitch,
            mysql_url: None,
            mysql_schema,
        }
    }

    /// Build a new currency.
    pub fn build(&self) -> Option<Currency> {
        use self::mysql::Schema;

        if !self.enabled {
            return None;
        }

        let backend = match self.ty {
            BackendType::BuiltIn => {
                let db = self.db.as_ref()?;
                let backend = self::builtin::Backend::new(db.clone());
                Backend::BuiltIn(backend)
            }
            BackendType::Mysql => {
                let channel = String::from("");
                let url = self.mysql_url.clone()?;
                let schema = self.mysql_schema.clone();

                let backend = match self::mysql::Backend::connect(channel, url, schema) {
                    Ok(backend) => backend,
                    Err(e) => {
                        log_err!(e, "failed to establish connection");
                        return None;
                    }
                };

                Backend::MySql(backend)
            }
            BackendType::Honkos => {
                let channel = String::from("");
                let url = self.mysql_url.clone()?;
                let schema = Schema {
                    table: String::from("honkos"),
                    user_column: String::from("username"),
                    balance_column: String::from("honko_balance"),
                };

                let backend = match self::mysql::Backend::connect(channel, url, schema) {
                    Ok(backend) => backend,
                    Err(e) => {
                        log_err!(e, "failed to establish connection");
                        return None;
                    }
                };

                Backend::MySql(backend)
            }
        };

        let name = Arc::new(self.name.as_ref()?.to_string());
        let twitch = self.twitch.clone();
        let command_enabled = self.command_enabled;

        Some(Currency {
            name,
            command_enabled,
            inner: Arc::new(Inner { backend, twitch }),
        })
    }
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub enum BackendType {
    #[serde(rename = "builtin")]
    BuiltIn,
    #[serde(rename = "mysql")]
    Mysql,
    #[serde(rename = "honkos")]
    Honkos,
}

impl Default for BackendType {
    fn default() -> Self {
        BackendType::BuiltIn
    }
}

enum Backend {
    BuiltIn(self::builtin::Backend),
    MySql(self::mysql::Backend),
}

impl Backend {
    /// Add (or subtract) from the balance for a single user.
    pub async fn balance_transfer(
        &self,
        channel: &str,
        giver: &str,
        taker: &str,
        amount: i64,
        override_balance: bool,
    ) -> Result<(), BalanceTransferError> {
        use self::Backend::*;

        match *self {
            BuiltIn(ref backend) => {
                backend
                    .balance_transfer(channel, giver, taker, amount, override_balance)
                    .await
            }
            MySql(ref backend) => {
                backend
                    .balance_transfer(channel, giver, taker, amount, override_balance)
                    .await
            }
        }
    }

    /// Get balances for all users.
    pub async fn export_balances(&self) -> Result<Vec<Balance>, Error> {
        use self::Backend::*;

        match *self {
            BuiltIn(ref backend) => backend.export_balances().await,
            MySql(ref backend) => backend.export_balances().await,
        }
    }

    /// Import balances for all users.
    pub async fn import_balances(&self, balances: Vec<Balance>) -> Result<(), Error> {
        use self::Backend::*;

        match *self {
            BuiltIn(ref backend) => backend.import_balances(balances).await,
            MySql(ref backend) => backend.import_balances(balances).await,
        }
    }

    /// Find user balance.
    pub async fn balance_of(&self, channel: &str, user: &str) -> Result<Option<BalanceOf>, Error> {
        use self::Backend::*;

        match *self {
            BuiltIn(ref backend) => backend.balance_of(channel, user).await,
            MySql(ref backend) => backend.balance_of(channel, user).await,
        }
    }

    /// Add (or subtract) from the balance for a single user.
    pub async fn balance_add(&self, channel: &str, user: &str, amount: i64) -> Result<(), Error> {
        use self::Backend::*;

        match *self {
            BuiltIn(ref backend) => backend.balance_add(channel, user, amount).await,
            MySql(ref backend) => backend.balance_add(channel, user, amount).await,
        }
    }

    /// Add balance to users.
    pub async fn balances_increment<I>(
        &self,
        channel: &str,
        users: I,
        amount: i64,
        watch_time: i64,
    ) -> Result<(), Error>
    where
        I: IntoIterator<Item = String> + Send + 'static,
        I::IntoIter: Send + 'static,
    {
        use self::Backend::*;

        match *self {
            BuiltIn(ref backend) => {
                backend
                    .balances_increment(channel, users, amount, watch_time)
                    .await
            }
            MySql(ref backend) => backend.balances_increment(channel, users, amount).await,
        }
    }
}

struct Inner {
    backend: Backend,
    twitch: api::Twitch,
}

/// The currency being used.
#[derive(Clone)]
pub struct Currency {
    pub name: Arc<String>,
    pub command_enabled: bool,
    inner: Arc<Inner>,
}

impl Currency {
    /// Reward all users.
    pub async fn add_channel_all(
        &self,
        channel: &str,
        reward: i64,
        watch_time: i64,
    ) -> Result<usize, failure::Error> {
        let chatters = self.inner.twitch.chatters(channel).await?;

        let mut users = HashSet::new();
        users.extend(chatters.viewers);
        users.extend(chatters.moderators);
        users.extend(chatters.broadcaster);

        let len = users.len();

        self.inner
            .backend
            .balances_increment(channel, users, reward, watch_time)
            .await?;

        Ok(len)
    }

    /// Add (or subtract) from the balance for a single user.
    pub async fn balance_transfer(
        &self,
        channel: &str,
        giver: &str,
        taker: &str,
        amount: i64,
        override_balance: bool,
    ) -> Result<(), BalanceTransferError> {
        self.inner
            .backend
            .balance_transfer(channel, giver, taker, amount, override_balance)
            .await
    }

    /// Get balances for all users.
    pub async fn export_balances(&self) -> Result<Vec<Balance>, Error> {
        self.inner.backend.export_balances().await
    }

    /// Import balances for all users.
    pub async fn import_balances(&self, balances: Vec<Balance>) -> Result<(), Error> {
        self.inner.backend.import_balances(balances).await
    }

    /// Find user balance.
    pub async fn balance_of(&self, channel: &str, user: &str) -> Result<Option<BalanceOf>, Error> {
        self.inner.backend.balance_of(channel, user).await
    }

    /// Add (or subtract) from the balance for a single user.
    pub async fn balance_add(&self, channel: &str, user: &str, amount: i64) -> Result<(), Error> {
        self.inner.backend.balance_add(channel, user, amount).await
    }

    /// Add balance to users.
    pub async fn balances_increment<I>(
        &self,
        channel: &str,
        users: I,
        amount: i64,
        watch_time: i64,
    ) -> Result<(), Error>
    where
        I: IntoIterator<Item = String> + Send + 'static,
        I::IntoIter: Send + 'static,
    {
        self.inner
            .backend
            .balances_increment(channel, users, amount, watch_time)
            .await
    }
}

#[derive(Debug, err_derive::Error)]
pub enum BalanceTransferError {
    #[error(display = "missing balance for transfer")]
    NoBalance,
    #[error(display = "other error: {}", _0)]
    Other(Error),
}

impl From<Error> for BalanceTransferError {
    fn from(value: Error) -> Self {
        BalanceTransferError::Other(value)
    }
}

impl From<diesel::result::Error> for BalanceTransferError {
    fn from(value: diesel::result::Error) -> Self {
        BalanceTransferError::Other(value.into())
    }
}

impl From<std::num::TryFromIntError> for BalanceTransferError {
    fn from(value: std::num::TryFromIntError) -> Self {
        BalanceTransferError::Other(value.into())
    }
}

impl From<mysql_async::error::Error> for BalanceTransferError {
    fn from(value: mysql_async::error::Error) -> Self {
        BalanceTransferError::Other(value.into())
    }
}
