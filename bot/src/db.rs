mod after_streams;
mod aliases;
mod commands;
pub(crate) mod models;
mod persisted_set;
mod promotions;
pub(crate) mod schema;
mod words;

use crate::player;

pub use self::{
    after_streams::{AfterStream, AfterStreams},
    aliases::{Alias, Aliases},
    commands::{Command, Commands},
    persisted_set::PersistedSet,
    promotions::{Promotion, Promotions},
    words::{Word, Words},
};

use chrono::Utc;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use futures::{future, Future};
use std::{error, fmt, sync::Arc};
use tokio_threadpool::ThreadPool;

#[derive(Debug)]
pub enum RenameError {
    /// Trying to rename something to a conflicting name.
    Conflict,
    /// Trying to rename something which doesn't exist.
    Missing,
}

impl error::Error for RenameError {}

impl fmt::Display for RenameError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            RenameError::Conflict => "conflict".fmt(fmt),
            RenameError::Missing => "missing".fmt(fmt),
        }
    }
}

embed_migrations!("./migrations");

/// Database abstraction.
#[derive(Clone)]
pub struct Database {
    pub(crate) pool: Arc<Pool<ConnectionManager<SqliteConnection>>>,
    thread_pool: Arc<ThreadPool>,
}

impl Database {
    /// Find posts by users.
    pub fn open(url: &str, thread_pool: Arc<ThreadPool>) -> Result<Database, failure::Error> {
        let manager = ConnectionManager::<SqliteConnection>::new(url);
        let pool = Pool::new(manager)?;

        // Run all migrations.
        embedded_migrations::run_with_output(&pool.get()?, &mut std::io::stdout())?;

        Ok(Database {
            pool: Arc::new(pool),
            thread_pool,
        })
    }

    /// Access settings from the database.
    pub fn settings(&self) -> crate::settings::Settings {
        crate::settings::Settings::new(self.clone())
    }

    /// Find user balance.
    pub fn balance_of(&self, channel: &str, user: &str) -> Result<Option<i32>, failure::Error> {
        use self::schema::balances::dsl;

        let c = self.pool.get()?;

        let b = dsl::balances
            .filter(dsl::channel.eq(channel).and(dsl::user.eq(user)))
            .first::<models::Balance>(&c)
            .optional()?;

        Ok(b.map(|b| b.amount))
    }

    /// Add (or subtract) from the balance for a single user.
    pub fn balance_add(
        &self,
        channel: &str,
        user: &str,
        amount_to_add: i32,
    ) -> impl Future<Item = (), Error = failure::Error> {
        use self::schema::balances::dsl;

        let user = user.to_string();
        let channel = String::from(channel);
        let pool = self.pool.clone();

        self.thread_pool.spawn_handle(future::lazy(move || {
            let c = pool.get()?;

            let filter =
                dsl::balances.filter(dsl::channel.eq(channel.as_str()).and(dsl::user.eq(&user)));

            let b = filter.clone().first::<models::Balance>(&c).optional()?;

            match b {
                None => {
                    let balance = models::Balance {
                        channel: channel.to_string(),
                        user,
                        amount: amount_to_add,
                    };

                    diesel::insert_into(dsl::balances)
                        .values(&balance)
                        .execute(&c)?;
                }
                Some(b) => {
                    let value = b.amount + amount_to_add;
                    diesel::update(filter)
                        .set(dsl::amount.eq(value))
                        .execute(&c)?;
                }
            }

            Ok(())
        }))
    }

    /// Add balance to users.
    pub fn balances_increment<'a>(
        &self,
        channel: &str,
        users: impl IntoIterator<Item = String> + Send + 'static,
        amount_to_add: i32,
    ) -> impl Future<Item = (), Error = failure::Error> {
        use self::schema::balances::dsl;

        let channel = String::from(channel);
        let pool = Arc::clone(&self.pool);

        self.thread_pool.spawn_handle(future::lazy(move || {
            let c = pool.get()?;

            for user in users {
                let user = user.to_lowercase();

                let filter = dsl::balances
                    .filter(dsl::channel.eq(channel.as_str()).and(dsl::user.eq(&user)));

                let b = filter.clone().first::<models::Balance>(&c).optional()?;

                match b {
                    None => {
                        let balance = models::Balance {
                            channel: channel.to_string(),
                            user: user.clone(),
                            amount: amount_to_add,
                        };

                        diesel::insert_into(dsl::balances)
                            .values(&balance)
                            .execute(&c)?;
                    }
                    Some(b) => {
                        let value = b.amount + amount_to_add;
                        diesel::update(filter)
                            .set(dsl::amount.eq(value))
                            .execute(&c)?;
                    }
                }
            }

            Ok(())
        }))
    }
}

impl words::Backend for Database {
    /// List all bad words.
    fn list(&self) -> Result<Vec<models::BadWord>, failure::Error> {
        use self::schema::bad_words::dsl;
        let c = self.pool.get()?;
        Ok(dsl::bad_words.load::<models::BadWord>(&c)?)
    }

    /// Insert a bad word into the database.
    fn edit(&self, word: &str, why: Option<&str>) -> Result<(), failure::Error> {
        use self::schema::bad_words::dsl;

        let c = self.pool.get()?;

        let filter = dsl::bad_words.filter(dsl::word.eq(word));
        let b = filter.clone().first::<models::BadWord>(&c).optional()?;

        match b {
            None => {
                let bad_word = models::BadWord {
                    word: word.to_string(),
                    why: why.map(|s| s.to_string()),
                };

                diesel::insert_into(dsl::bad_words)
                    .values(&bad_word)
                    .execute(&c)?;
            }
            Some(_) => {
                diesel::update(filter)
                    .set(why.map(|w| dsl::why.eq(w)))
                    .execute(&c)?;
            }
        }

        Ok(())
    }

    fn delete(&self, word: &str) -> Result<bool, failure::Error> {
        use self::schema::bad_words::dsl;

        let c = self.pool.get()?;

        let count = diesel::delete(dsl::bad_words.filter(dsl::word.eq(&word))).execute(&c)?;
        Ok(count == 1)
    }
}

impl player::Backend for Database {
    fn list(&self) -> Result<Vec<models::Song>, failure::Error> {
        use self::schema::songs::dsl;
        let c = self.pool.get()?;
        let songs = dsl::songs
            .filter(dsl::deleted.eq(false))
            .order((dsl::promoted_at.desc(), dsl::added_at.asc()))
            .load::<models::Song>(&c)?;
        Ok(songs)
    }

    fn push_back(&self, song: &models::AddSong) -> Result<(), failure::Error> {
        use self::schema::songs::dsl;
        let c = self.pool.get()?;
        diesel::insert_into(dsl::songs).values(song).execute(&c)?;
        Ok(())
    }

    /// Purge the given channel from songs.
    fn song_purge(&self) -> Result<usize, failure::Error> {
        use self::schema::songs::dsl;
        let c = self.pool.get()?;
        Ok(diesel::update(dsl::songs.filter(dsl::deleted.eq(false)))
            .set(dsl::deleted.eq(true))
            .execute(&c)?)
    }

    /// Remove the song at the given location.
    fn remove_song(&self, track_id: &player::TrackId) -> Result<bool, failure::Error> {
        use self::schema::songs::dsl;
        let c = self.pool.get()?;

        let track_id = track_id.to_base62();

        let ids: Vec<i32> = dsl::songs
            .select(dsl::id)
            .filter(dsl::deleted.eq(false).and(dsl::track_id.eq(&track_id)))
            .order(dsl::added_at.desc())
            .limit(1)
            .load(&c)?;

        let count = diesel::update(dsl::songs.filter(dsl::id.eq_any(ids)))
            .set(dsl::deleted.eq(true))
            .execute(&c)?;

        Ok(count == 1)
    }

    /// Promote the song with the given ID.
    fn promote_song(&self, user: &str, track_id: &player::TrackId) -> Result<bool, failure::Error> {
        use self::schema::songs::dsl;
        let c = self.pool.get()?;

        let track_id = track_id.to_base62();

        let ids: Vec<i32> = dsl::songs
            .select(dsl::id)
            .filter(dsl::deleted.eq(false).and(dsl::track_id.eq(&track_id)))
            .order(dsl::added_at.desc())
            .limit(1)
            .load(&c)?;

        let count = diesel::update(dsl::songs.filter(dsl::id.eq_any(ids)))
            .set((
                dsl::promoted_at.eq(Utc::now().naive_utc()),
                dsl::promoted_by.eq(user),
            ))
            .execute(&c)?;

        Ok(count == 1)
    }
}

impl persisted_set::Backend for Database {
    fn list(&self, kind: &str) -> Result<Vec<models::SetValue>, failure::Error> {
        use self::schema::set_values::dsl;
        let c = self.pool.get()?;
        Ok(dsl::set_values
            .filter(dsl::kind.eq(kind))
            .load::<models::SetValue>(&c)?)
    }

    fn insert(&self, channel: &str, kind: &str, value: String) -> Result<(), failure::Error> {
        use self::schema::set_values::dsl;
        let c = self.pool.get()?;

        let value = models::SetValue {
            channel: channel.to_string(),
            kind: kind.to_string(),
            value,
        };

        diesel::insert_into(dsl::set_values)
            .values(value)
            .execute(&c)?;
        Ok(())
    }

    fn remove(&self, channel: &str, kind: &str, value: String) -> Result<bool, failure::Error> {
        use self::schema::set_values::dsl;
        let c = self.pool.get()?;

        let filter = dsl::set_values.filter(
            dsl::channel
                .eq(channel)
                .and(dsl::kind.eq(kind))
                .and(dsl::value.eq(value)),
        );

        let count = diesel::delete(filter).execute(&c)?;
        Ok(count == 1)
    }
}
