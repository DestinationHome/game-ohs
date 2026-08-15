use mongodb::bson::{DateTime, doc};
use serde_json::Value;

/// Global data for a project, not scoped to any specific user.
///
/// Each `(account, project)` combination has one document containing
/// a JSON object with all global key-value pairs.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GlobalEntry {
    /// The OHS account scope (from the URL path).
    pub account: String,

    /// The OHS project scope (from the URL path).
    pub project: String,

    /// The global data as a JSON object (e.g., `{"totalDrinks": 100, "totalCredits": 5000}`).
    pub data: Value,

    /// When this entry was last updated.
    pub updated_at: DateTime,
}

impl GlobalEntry {
    const fn collection_name() -> &'static str {
        "GlobalEntry"
    }

    fn collection(db: &mongodb::Database) -> mongodb::Collection<Self> {
        db.collection(Self::collection_name())
    }

    // ----- Reads --------------------------------------------------------

    /// Set global data for a project (upsert).
    pub async fn set(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        data: &Value,
    ) -> mongodb::error::Result<()> {
        let filter = doc! {
            "account": account,
            "project": project,
        };

        let bson_data = mongodb::bson::to_bson(data).unwrap_or(mongodb::bson::Bson::Null);

        let update = doc! {
            "$set": {
                "data": bson_data,
                "updated_at": DateTime::now(),
            },
            "$setOnInsert": {
                "account": account,
                "project": project,
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

    /// Get the entire global data object for a project.
    ///
    /// Returns an empty object if no globals exist.
    pub async fn get_all(
        db: &mongodb::Database,
        account: &str,
        project: &str,
    ) -> mongodb::error::Result<Value> {
        let filter = doc! {
            "account": account,
            "project": project,
        };

        let db_result = Self::collection(db).find_one(filter).await?;

        if let Some(entry) = db_result {
            return Ok(entry.data);
        }

        // Fallback to static JSON file
        if let Some(map) = super::load_fallback_json(account, project, &["globals.json"]) {
            let data = Value::Object(map);
            if let Err(e) = Self::set(db, account, project, &data).await {
                tracing::error!("Failed to cache fallback globals to DB: {}", e);
            }
            return Ok(data);
        }

        Ok(Value::Object(serde_json::Map::new()))
    }

    /// Get a specific global value by key.
    ///
    /// Returns None if the key doesn't exist.
    pub async fn get(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        key: &str,
    ) -> mongodb::error::Result<Option<Value>> {
        let data = Self::get_all(db, account, project).await?;

        Ok(data.as_object().and_then(|obj| obj.get(key)).cloned())
    }

    /// Get multiple global values by keys.
    ///
    /// Returns a filtered object containing only the requested keys.
    pub async fn get_many(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        keys: &[String],
    ) -> mongodb::error::Result<Value> {
        let data = Self::get_all(db, account, project).await?;

        data.as_object().map_or_else(
            || Ok(Value::Object(serde_json::Map::new())),
            |obj| {
                let filtered: serde_json::Map<String, Value> = obj
                    .iter()
                    .filter(|(k, _)| keys.contains(k))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                Ok(Value::Object(filtered))
            },
        )
    }

    // ----- Indexes ------------------------------------------------------

    /// Ensure required indexes exist.
    pub async fn ensure_indexes(db: &mongodb::Database) -> mongodb::error::Result<()> {
        use mongodb::IndexModel;
        use mongodb::bson::doc;

        let collection = Self::collection(db);

        let indexes = vec![
            // Unique compound key for global data identity
            IndexModel::builder()
                .keys(doc! {
                    "account": 1,
                    "project": 1,
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
