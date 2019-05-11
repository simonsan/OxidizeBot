use crate::{db, template, utils};
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use failure::{format_err, ResultExt as _};
use hashbrown::HashMap;
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Debug, err_derive::Error)]
pub enum BumpError {
    /// Trying to bump something which doesn't exist.
    #[error(display = "promotion missing")]
    Missing,
    /// Database error occurred.
    #[error(display = "database error: {}", _0)]
    Database(failure::Error),
}

/// Local database wrapper.
#[derive(Clone)]
struct Database(db::Database);

impl Database {
    private_database_group_fns!(promotions, Promotion, Key);

    fn edit(
        &self,
        key: &Key,
        frequency: utils::Duration,
        text: &str,
    ) -> Result<(), failure::Error> {
        use db::schema::promotions::dsl;

        let c = self.0.pool.lock();
        let filter =
            dsl::promotions.filter(dsl::channel.eq(&key.channel).and(dsl::name.eq(&key.name)));
        let b = filter
            .clone()
            .first::<db::models::Promotion>(&*c)
            .optional()?;

        let frequency = frequency.num_seconds() as i32;

        match b {
            None => {
                let command = db::models::Promotion {
                    channel: key.channel.to_string(),
                    name: key.name.to_string(),
                    frequency,
                    promoted_at: None,
                    text: text.to_string(),
                    group: None,
                    disabled: false,
                };

                diesel::insert_into(dsl::promotions)
                    .values(&command)
                    .execute(&*c)?;
            }
            Some(_) => {
                let mut set = db::models::UpdatePromotion::default();
                set.text = Some(text);
                set.frequency = Some(frequency);

                diesel::update(filter).set(&set).execute(&*c)?;
            }
        }

        Ok(())
    }

    fn delete(&self, key: &Key) -> Result<bool, failure::Error> {
        use db::schema::promotions::dsl;

        let c = self.0.pool.lock();
        let count = diesel::delete(
            dsl::promotions.filter(dsl::channel.eq(&key.channel).and(dsl::name.eq(&key.name))),
        )
        .execute(&*c)?;
        Ok(count == 1)
    }

    fn rename(&self, from: &Key, to: &Key) -> Result<bool, failure::Error> {
        use db::schema::promotions::dsl;

        let c = self.0.pool.lock();
        let count = diesel::update(
            dsl::promotions.filter(dsl::channel.eq(&from.channel).and(dsl::name.eq(&from.name))),
        )
        .set((dsl::channel.eq(&to.channel), dsl::name.eq(&to.name)))
        .execute(&*c)?;

        Ok(count == 1)
    }

    fn bump_promoted_at(&self, from: &Key, now: &DateTime<Utc>) -> Result<bool, failure::Error> {
        use db::schema::promotions::dsl;

        let c = self.0.pool.lock();
        let count = diesel::update(
            dsl::promotions.filter(dsl::channel.eq(&from.channel).and(dsl::name.eq(&from.name))),
        )
        .set(dsl::promoted_at.eq(now.naive_utc()))
        .execute(&*c)?;

        Ok(count == 1)
    }
}

#[derive(Clone)]
pub struct Promotions {
    inner: Arc<RwLock<HashMap<Key, Arc<Promotion>>>>,
    db: Database,
}

impl Promotions {
    database_group_fns!(Promotion, Key);

    /// Construct a new promos store with a db.
    pub fn load(db: db::Database) -> Result<Promotions, failure::Error> {
        let db = Database(db);

        let mut inner = HashMap::new();

        for promotion in db.list()? {
            let promotion = Promotion::from_db(promotion)?;
            inner.insert(promotion.key.clone(), Arc::new(promotion));
        }

        Ok(Promotions {
            inner: Arc::new(RwLock::new(inner)),
            db,
        })
    }

    /// Insert a word into the bad words list.
    pub fn edit(
        &self,
        channel: &str,
        name: &str,
        frequency: utils::Duration,
        template: template::Template,
    ) -> Result<(), failure::Error> {
        let key = Key::new(channel, name);

        self.db.edit(&key, frequency.clone(), template.source())?;

        let mut inner = self.inner.write();

        db_in_memory_update!(inner, key, |p| {
            p.frequency = frequency;
            p.template = template;
            p.promoted_at = None;
        });

        Ok(())
    }

    /// Remove promotion.
    pub fn delete(&self, channel: &str, name: &str) -> Result<bool, failure::Error> {
        let key = Key::new(channel, name);

        if !self.db.delete(&key)? {
            return Ok(false);
        }

        self.inner.write().remove(&key);
        Ok(true)
    }

    /// Test the given word.
    pub fn get<'a>(&'a self, channel: &str, name: &str) -> Option<Arc<Promotion>> {
        let key = Key::new(channel, name);

        let inner = self.inner.read();

        if let Some(promotion) = inner.get(&key) {
            return Some(Arc::clone(promotion));
        }

        None
    }

    /// Get a list of all enabled promos.
    pub fn list(&self, channel: &str) -> Vec<Arc<Promotion>> {
        let inner = self.inner.read();

        let mut out = Vec::new();

        for c in inner.values() {
            if c.key.channel != channel {
                continue;
            }

            out.push(Arc::clone(c));
        }

        out
    }

    /// Try to rename the promotion.
    pub fn rename(&self, channel: &str, from: &str, to: &str) -> Result<(), db::RenameError> {
        let from = Key::new(channel, from);
        let to = Key::new(channel, to);

        let mut inner = self.inner.write();

        if inner.contains_key(&to) {
            return Err(db::RenameError::Conflict);
        }

        let promotion = match inner.remove(&from) {
            Some(promotion) => promotion,
            None => return Err(db::RenameError::Missing),
        };

        let mut promotion = (*promotion).clone();
        promotion.key = to.clone();

        match self.db.rename(&from, &to) {
            Err(e) => {
                log::error!(
                    "failed to rename promotion `{}` in database: {}",
                    from.name,
                    e
                );
            }
            Ok(false) => {
                log::warn!("promotion {} not renamed in database", from.name);
            }
            Ok(true) => (),
        }

        inner.insert(to, Arc::new(promotion));
        Ok(())
    }

    /// Bump that the given promotion was last promoted right now.
    pub fn bump_promoted_at(&self, promotion: &Promotion) -> Result<(), BumpError> {
        let mut inner = self.inner.write();

        let promotion = match inner.remove(&promotion.key) {
            Some(promotion) => promotion,
            None => return Err(BumpError::Missing),
        };

        let now = Utc::now();

        self.db
            .bump_promoted_at(&promotion.key, &now)
            .map_err(BumpError::Database)?;

        let mut promotion = (*promotion).clone();
        promotion.promoted_at = Some(now);

        inner.insert(promotion.key.clone(), Arc::new(promotion));
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Key {
    pub channel: String,
    pub name: String,
}

impl Key {
    pub fn new(channel: &str, name: &str) -> Self {
        Self {
            channel: channel.to_string(),
            name: name.to_lowercase(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Promotion {
    pub key: Key,
    pub frequency: utils::Duration,
    pub promoted_at: Option<DateTime<Utc>>,
    pub template: template::Template,
    pub group: Option<String>,
    pub disabled: bool,
}

impl Promotion {
    pub fn from_db(promotion: db::models::Promotion) -> Result<Promotion, failure::Error> {
        let template = template::Template::compile(&promotion.text).with_context(|_| {
            format_err!("failed to compile promotion `{:?}` from db", promotion)
        })?;

        let key = Key::new(promotion.channel.as_str(), promotion.name.as_str());
        let frequency = utils::Duration::seconds(promotion.frequency as u64);
        let promoted_at = promotion
            .promoted_at
            .map(|d| DateTime::<Utc>::from_utc(d, Utc));

        Ok(Promotion {
            key,
            frequency,
            promoted_at,
            template,
            group: promotion.group,
            disabled: promotion.disabled,
        })
    }

    /// Render the given promotion.
    pub fn render<T>(&self, data: &T) -> Result<String, failure::Error>
    where
        T: serde::Serialize,
    {
        Ok(self.template.render_to_string(data)?)
    }
}
