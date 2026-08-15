use futures::TryStreamExt;
use mongodb::bson::{DateTime, doc};

/// A single counter entry for a user.
///
/// Counters are scoped by `(account, project, user, key)`.
/// Each combination stores exactly one i64 value.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CounterEntry {
    /// The OHS account scope (from the URL path).
    pub account: String,

    /// The OHS project scope (from the URL path).
    pub project: String,

    /// The username that owns this counter.
    pub user: String,

    /// The counter key, e.g. `"credits"`, `"xp"`, etc.
    pub key: String,

    /// The counter value.
    pub value: i64,

    /// When this counter was last updated.
    pub updated_at: DateTime,
}

impl CounterEntry {
    const fn collection_name() -> &'static str {
        "CounterEntry"
    }

    fn collection(db: &mongodb::Database) -> mongodb::Collection<Self> {
        db.collection(Self::collection_name())
    }

    // ----- Reads --------------------------------------------------------

    /// Get a single counter value by key for a user.
    ///
    /// Returns the stored value, or 0 if no entry exists.
    pub async fn get(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        user: &str,
        key: &str,
    ) -> mongodb::error::Result<i64> {
        let filter = doc! {
            "account": account,
            "project": project,
            "user": user,
            "key": key,
        };

        Ok(Self::collection(db)
            .find_one(filter)
            .await?
            .map(|entry| entry.value)
            .unwrap_or(0))
    }

    /// Get multiple counter values by keys for a user.
    ///
    /// Returns a map of `key -> value`. Missing keys default to 0.
    pub async fn get_many(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        user: &str,
        keys: &[String],
    ) -> mongodb::error::Result<serde_json::Map<String, serde_json::Value>> {
        let filter = doc! {
            "account": account,
            "project": project,
            "user": user,
            "key": { "$in": keys },
        };

        let entries: Vec<Self> = Self::collection(db)
            .find(filter)
            .await?
            .try_collect()
            .await?;

        let mut result = serde_json::Map::new();

        // Initialize all requested keys to 0
        for key in keys {
            result.insert(key.clone(), 0.into());
        }

        // Fill in actual values
        for entry in entries {
            result.insert(entry.key, entry.value.into());
        }

        Ok(result)
    }

    /// Get all counters for a user in a specific project.
    ///
    /// Returns a map of `key -> value`.
    pub async fn get_all(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        user: &str,
    ) -> mongodb::error::Result<serde_json::Map<String, serde_json::Value>> {
        let filter = doc! {
            "account": account,
            "project": project,
            "user": user,
        };

        let entries: Vec<Self> = Self::collection(db)
            .find(filter)
            .await?
            .try_collect()
            .await?;

        let mut result = serde_json::Map::new();
        for entry in entries {
            result.insert(entry.key, entry.value.into());
        }

        Ok(result)
    }

    // ----- Writes -------------------------------------------------------

    /// Set a counter value (upsert).
    pub async fn set(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        user: &str,
        key: &str,
        value: i64,
    ) -> mongodb::error::Result<()> {
        let filter = doc! {
            "account": account,
            "project": project,
            "user": user,
            "key": key,
        };

        let update = doc! {
            "$set": {
                "value": value,
                "updated_at": DateTime::now(),
            },
            "$setOnInsert": {
                "account": account,
                "project": project,
                "user": user,
                "key": key,
            }
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

    /// Increment a counter by a delta value (can be negative).
    ///
    /// If the counter doesn't exist, it's created with the delta value.
    /// Returns the new counter value after increment.
    pub async fn increment(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        user: &str,
        key: &str,
        delta: i64,
    ) -> mongodb::error::Result<i64> {
        let filter = doc! {
            "account": account,
            "project": project,
            "user": user,
            "key": key,
        };

        let update = doc! {
            "$inc": { "value": delta },
            "$set": { "updated_at": DateTime::now() },
            "$setOnInsert": {
                "account": account,
                "project": project,
                "user": user,
                "key": key,
            }
        };

        let options = mongodb::options::FindOneAndUpdateOptions::builder()
            .upsert(true)
            .return_document(mongodb::options::ReturnDocument::After)
            .build();

        let entry = Self::collection(db)
            .find_one_and_update(filter, update)
            .with_options(options)
            .await?
            .ok_or_else(|| mongodb::error::Error::custom("Failed to increment counter"))?;

        Ok(entry.value)
    }

    // ----- Indexes ------------------------------------------------------

    /// Ensure required indexes exist.
    pub async fn ensure_indexes(db: &mongodb::Database) -> mongodb::error::Result<()> {
        use mongodb::IndexModel;
        use mongodb::bson::doc;

        let collection = Self::collection(db);

        let indexes = vec![
            // Unique compound key for counter identity
            IndexModel::builder()
                .keys(doc! {
                    "account": 1,
                    "project": 1,
                    "user": 1,
                    "key": 1,
                })
                .options(
                    mongodb::options::IndexOptions::builder()
                        .unique(true)
                        .build(),
                )
                .build(),
            // Prefix index for user counter lookups
            IndexModel::builder()
                .keys(doc! {
                    "account": 1,
                    "project": 1,
                    "user": 1,
                })
                .build(),
        ];

        collection.create_indexes(indexes).await?;
        Ok(())
    }
}
