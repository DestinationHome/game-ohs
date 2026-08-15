use mongodb::bson::{DateTime, doc};

/// A community score entry shared globally among all users.
///
/// Community scores are scoped by `(account, project, key)`.
/// Unlike counters and data, these are NOT user-specific.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommunityEntry {
    /// The OHS account scope (from the URL path).
    pub account: String,

    /// The OHS project scope (from the URL path).
    pub project: String,

    /// The score key, e.g. `"total_kills"`, `"global_progress"`, etc.
    pub key: String,

    /// The community score value.
    pub score: i64,

    /// When this score was last updated.
    pub updated_at: DateTime,
}

impl CommunityEntry {
    const fn collection_name() -> &'static str {
        "CommunityEntry"
    }

    fn collection(db: &mongodb::Database) -> mongodb::Collection<Self> {
        db.collection(Self::collection_name())
    }

    // ----- Reads --------------------------------------------------------

    /// Get a single community score by key.
    ///
    /// Returns the stored score, or 0 if no entry exists.
    pub async fn get(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        key: &str,
    ) -> mongodb::error::Result<i64> {
        let filter = doc! {
            "account": account,
            "project": project,
            "key": key,
        };

        Ok(Self::collection(db)
            .find_one(filter)
            .await?
            .map(|entry| entry.score)
            .unwrap_or(0))
    }

    // ----- Writes -------------------------------------------------------

    /// Set a community score (upsert).
    pub async fn set(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        key: &str,
        score: i64,
    ) -> mongodb::error::Result<()> {
        let filter = doc! {
            "account": account,
            "project": project,
            "key": key,
        };

        let update = doc! {
            "$set": {
                "score": score,
                "updated_at": DateTime::now(),
            },
            "$setOnInsert": {
                "account": account,
                "project": project,
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

    /// Increment a community score by a delta value (can be negative).
    ///
    /// If the score doesn't exist, it's created with the delta value.
    /// Returns the new score value after increment.
    pub async fn increment(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        key: &str,
        delta: i64,
    ) -> mongodb::error::Result<i64> {
        let filter = doc! {
            "account": account,
            "project": project,
            "key": key,
        };

        let update = doc! {
            "$inc": { "score": delta },
            "$set": { "updated_at": DateTime::now() },
            "$setOnInsert": {
                "account": account,
                "project": project,
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
            .ok_or_else(|| mongodb::error::Error::custom("Failed to increment community score"))?;

        Ok(entry.score)
    }

    // ----- Indexes ------------------------------------------------------

    /// Ensure required indexes exist.
    pub async fn ensure_indexes(db: &mongodb::Database) -> mongodb::error::Result<()> {
        use mongodb::IndexModel;
        use mongodb::bson::doc;

        let collection = Self::collection(db);

        let indexes = vec![
            // Unique compound key for community score identity
            IndexModel::builder()
                .keys(doc! {
                    "account": 1,
                    "project": 1,
                    "key": 1,
                })
                .options(
                    mongodb::options::IndexOptions::builder()
                        .unique(true)
                        .build(),
                )
                .build(),
        ];

        collection.create_indexes(indexes).await?;
        Ok(())
    }
}
