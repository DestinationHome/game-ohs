use mongodb::bson::{DateTime, doc};

/// A single user's score entry on a leaderboard.
///
/// Each leaderboard is identified by `(account, project, key)`.
/// Within a leaderboard, each user has at most one entry —
/// the compound key `(account, project, key, user)` is unique.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LeaderboardEntry {
    /// The OHS account scope (from the URL path).
    pub account: String,

    /// The OHS project scope (from the URL path).
    pub project: String,

    /// The leaderboard key, e.g. `"race_best_time"`.
    pub key: String,

    /// The username of the player who submitted this score.
    pub user: String,

    /// The player's score value.
    pub score: f64,

    /// When the score was last updated.
    pub updated_at: DateTime,
}

impl LeaderboardEntry {
    /// Returns the MongoDB collection name for leaderboard entries.
    const fn collection_name() -> &'static str {
        "LeaderboardEntry"
    }

    /// Returns a handle to the leaderboard collection.
    fn collection(db: &mongodb::Database) -> mongodb::Collection<Self> {
        db.collection(Self::collection_name())
    }

    /// Insert or update a score entry.
    ///
    /// Uses `$max` so the stored score is only replaced when the new score
    /// is higher than the existing one.
    pub async fn upsert_score(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        key: &str,
        user: &str,
        score: f64,
    ) -> mongodb::error::Result<()> {
        let filter = doc! {
            "account": account,
            "project": project,
            "key":     key,
            "user":    user,
        };

        let update = doc! {
            "$max": { "score": score },
            "$set": { "updated_at": DateTime::now() },
            "$setOnInsert": {
                "account": account,
                "project": project,
                "key":     key,
                "user":    user,
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

    /// Fetch a page of entries for a leaderboard, sorted by score descending.
    pub async fn get_by_rank(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        key: &str,
        start: u32,
        limit: u32,
    ) -> mongodb::error::Result<Vec<Self>> {
        use futures::TryStreamExt;

        let filter = doc! {
            "account": account,
            "project": project,
            "key":     key,
        };

        let options = mongodb::options::FindOptions::builder()
            .sort(doc! { "score": -1 })
            .skip(start as u64)
            .limit(limit as i64)
            .build();

        Self::collection(db)
            .find(filter)
            .with_options(options)
            .await?
            .try_collect()
            .await
    }

    /// Fetch entries for specific users on a leaderboard, sorted by score descending.
    pub async fn get_by_users(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        key: &str,
        users: &[String],
        limit: u32,
    ) -> mongodb::error::Result<Vec<Self>> {
        use futures::TryStreamExt;

        let filter = doc! {
            "account": account,
            "project": project,
            "key":     key,
            "user":    { "$in": users },
        };

        let options = mongodb::options::FindOptions::builder()
            .sort(doc! { "score": -1 })
            .limit(limit as i64)
            .build();

        Self::collection(db)
            .find(filter)
            .with_options(options)
            .await?
            .try_collect()
            .await
    }

    /// Fetch the top score/user for each leaderboard key in the project.
    /// Returns a map of key -> (user, score).
    pub async fn get_all_top_scores(
        db: &mongodb::Database,
        account: &str,
        project: &str,
    ) -> mongodb::error::Result<std::collections::HashMap<String, (String, f64)>> {
        use futures::TryStreamExt;

        let pipeline = vec![
            doc! {
                "$match": {
                    "account": account,
                    "project": project,
                }
            },
            doc! {
                "$sort": {
                    "score": -1
                }
            },
            doc! {
                "$group": {
                    "_id": "$key",
                    "user": { "$first": "$user" },
                    "score": { "$first": "$score" }
                }
            },
        ];

        let mut cursor = db
            .collection::<mongodb::bson::Document>(Self::collection_name())
            .aggregate(pipeline)
            .await?;

        let mut results = std::collections::HashMap::new();
        while let Some(doc) = cursor.try_next().await? {
            if let (Some(key), Some(user), Some(score)) = (
                doc.get_str("_id").ok(),
                doc.get_str("user").ok(),
                doc.get_f64("score").ok(),
            ) {
                results.insert(key.to_string(), (user.to_string(), score));
            }
        }

        Ok(results)
    }

    /// Create the compound indexes needed for efficient leaderboard queries.
    ///
    /// Should be called once at application startup.
    pub async fn ensure_indexes(db: &mongodb::Database) -> mongodb::error::Result<()> {
        use mongodb::IndexModel;

        let unique_entry = IndexModel::builder()
            .keys(doc! {
                "account": 1,
                "project": 1,
                "key":     1,
                "user":    1,
            })
            .options(
                mongodb::options::IndexOptions::builder()
                    .unique(true)
                    .build(),
            )
            .build();

        let rank_query = IndexModel::builder()
            .keys(doc! {
                "account": 1,
                "project": 1,
                "key":     1,
                "score":  -1,
            })
            .build();

        Self::collection(db)
            .create_indexes([unique_entry, rank_query])
            .await
            .map(|_| ())
    }
}
