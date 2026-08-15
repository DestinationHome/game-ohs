use mongodb::bson::{DateTime, doc};
use serde_json::Value;

/// A default value for a data key, scoped by `(account, project, key)`.
///
/// When a player has no stored [`super::data::DataEntry`] for a given key,
/// the system can fall back to the matching `DataEntryDefault` so the game
/// always receives a sensible initial value for brand-new players.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DataEntryDefault {
    /// The OHS account scope (from the URL path).
    pub account: String,

    /// The OHS project scope (from the URL path).
    pub project: String,

    /// The data key this default applies to, e.g. `"inventory"`, `"progress"`.
    pub key: String,

    /// The default value to return when no user-specific entry exists.
    pub value: Value,

    /// When this default was last written.
    pub updated_at: DateTime,
}

impl DataEntryDefault {
    const fn collection_name() -> &'static str {
        "DataEntryDefault"
    }

    fn collection(db: &mongodb::Database) -> mongodb::Collection<Self> {
        db.collection(Self::collection_name())
    }

    // ----- Reads --------------------------------------------------------

    /// Get the default value for a single key.
    ///
    /// Returns the stored default `value`, or `None` if no default is set.
    pub async fn get(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        key: &str,
    ) -> mongodb::error::Result<Option<Value>> {
        let filter = doc! {
            "account": account,
            "project": project,
            "key":     key,
        };

        let db_result = Self::collection(db).find_one(filter).await?;

        if let Some(entry) = db_result {
            return Ok(Some(entry.value));
        }

        // Fallback to static JSON file
        if let Some(map) =
            super::load_fallback_json(account, project, &["defaults.json", "default.json"])
            && let Some(val) = map.get(key)
        {
            if let Err(e) = Self::set(db, account, project, key, val).await {
                tracing::error!(
                    "Failed to cache fallback default to DB for key {}: {}",
                    key,
                    e
                );
            }
            return Ok(Some(val.clone()));
        }

        Ok(None)
    }

    /// Get default values for multiple keys at once.
    ///
    /// Returns a map of `key → value` for every key that has a default.
    #[allow(dead_code, reason = "useful for future API usage external to the game")]
    pub async fn gets(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        keys: &[String],
    ) -> mongodb::error::Result<serde_json::Map<String, Value>> {
        use futures::TryStreamExt;

        let filter = doc! {
            "account": account,
            "project": project,
            "key":     { "$in": keys },
        };

        let entries: Vec<Self> = Self::collection(db)
            .find(filter)
            .await?
            .try_collect()
            .await?;

        let mut map = serde_json::Map::new();
        for entry in entries {
            map.insert(entry.key, entry.value);
        }

        let mut missing_keys = Vec::new();
        for key in keys {
            if !map.contains_key(key) {
                missing_keys.push(key.clone());
            }
        }

        if !missing_keys.is_empty()
            && let Some(fallback_map) =
                super::load_fallback_json(account, project, &["defaults.json", "default.json"])
        {
            for key in missing_keys {
                if let Some(val) = fallback_map.get(&key) {
                    if let Err(e) = Self::set(db, account, project, &key, val).await {
                        tracing::error!(
                            "Failed to cache fallback default to DB for key {}: {}",
                            key,
                            e
                        );
                    }
                    map.insert(key, val.clone());
                }
            }
        }

        Ok(map)
    }

    /// Get all defaults for a given account/project scope.
    ///
    /// Returns a map of `key → value`.
    pub async fn get_all(
        db: &mongodb::Database,
        account: &str,
        project: &str,
    ) -> mongodb::error::Result<serde_json::Map<String, Value>> {
        use futures::TryStreamExt;

        let filter = doc! {
            "account": account,
            "project": project,
        };

        let entries: Vec<Self> = Self::collection(db)
            .find(filter)
            .await?
            .try_collect()
            .await?;

        let mut map = serde_json::Map::new();
        for entry in entries {
            map.insert(entry.key, entry.value);
        }

        if map.is_empty()
            && let Some(fallback_map) =
                super::load_fallback_json(account, project, &["defaults.json", "default.json"])
        {
            for (k, v) in &fallback_map {
                if let Err(e) = Self::set(db, account, project, k, v).await {
                    tracing::error!(
                        "Failed to cache fallback default to DB for key {}: {}",
                        k,
                        e
                    );
                }
            }
            return Ok(fallback_map);
        }

        Ok(map)
    }

    // ----- Writes -------------------------------------------------------

    /// Set (or overwrite) the default value for a key.
    #[allow(dead_code, reason = "useful for future API usage external to the game")]
    pub async fn set(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        key: &str,
        value: &Value,
    ) -> mongodb::error::Result<()> {
        let filter = doc! {
            "account": account,
            "project": project,
            "key":     key,
        };

        let bson_value = mongodb::bson::to_bson(value).unwrap_or(mongodb::bson::Bson::Null);

        let update = doc! {
            "$set": {
                "value":      bson_value,
                "updated_at": DateTime::now(),
            },
            "$setOnInsert": {
                "account": account,
                "project": project,
                "key":     key,
            },
        };

        let options = mongodb::options::UpdateOptions::builder()
            .upsert(true)
            .build();

        Self::collection(db)
            .update_one(filter, update)
            .with_options(options)
            .await
            .map(|_| ())
    }

    /// Remove the default for a specific key.
    #[allow(dead_code, reason = "useful for future API usage external to the game")]
    pub async fn delete(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        key: &str,
    ) -> mongodb::error::Result<bool> {
        let filter = doc! {
            "account": account,
            "project": project,
            "key":     key,
        };

        let result = Self::collection(db).delete_one(filter).await?;

        Ok(result.deleted_count > 0)
    }

    // ----- Indexes ------------------------------------------------------

    /// Create the compound indexes needed for efficient default-value queries.
    ///
    /// Should be called once at application startup.
    pub async fn ensure_indexes(db: &mongodb::Database) -> mongodb::error::Result<()> {
        use mongodb::IndexModel;

        // Unique default per (account, project, key)
        let unique_default = IndexModel::builder()
            .keys(doc! {
                "account": 1,
                "project": 1,
                "key":     1,
            })
            .options(
                mongodb::options::IndexOptions::builder()
                    .unique(true)
                    .build(),
            )
            .build();

        Self::collection(db)
            .create_indexes([unique_default])
            .await
            .map(|_| ())
    }
}
