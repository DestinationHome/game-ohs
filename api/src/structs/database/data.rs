use mongodb::bson::{DateTime, doc};
use serde_json::Value;

use crate::structs::database::data_defaults::DataEntryDefault;

/// A single key-value entry in a user's data store.
///
/// Data is scoped by `(account, project, user, key)`.  Each combination
/// stores exactly one JSON `value`.  This replaces the old deeply-nested
/// `user.data[project][env][key]` approach with a flat, indexed collection.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DataEntry {
    /// The OHS account scope (from the URL path).
    pub account: String,

    /// The OHS project scope (from the URL path).
    pub project: String,

    /// The username that owns this data.
    pub user: String,

    /// The data key, e.g. `"inventory"`, `"progress"`, etc.
    pub key: String,

    /// The stored value (arbitrary JSON).
    pub value: Value,

    /// When this entry was last written.
    pub updated_at: DateTime,
}

impl DataEntry {
    const fn collection_name() -> &'static str {
        "DataEntry"
    }

    fn collection(db: &mongodb::Database) -> mongodb::Collection<Self> {
        db.collection(Self::collection_name())
    }

    // ----- Reads --------------------------------------------------------

    /// Get a single value by key for a user.
    ///
    /// Returns the stored `value`, or `None` if no entry exists.
    pub async fn get(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        user: &str,
        key: &str,
    ) -> mongodb::error::Result<Option<Value>> {
        let filter = doc! {
            "account": account,
            "project": project,
            "user":    user,
            "key":     key,
        };

        let result = Self::collection(db)
            .find_one(filter)
            .await?
            .map(|e| e.value);

        // If no user-specific entry exists, check for a default value.
        if result.is_none() {
            DataEntryDefault::get(db, account, project, key).await
        } else {
            Ok(result)
        }
    }

    /// Get multiple values by keys for a user.
    ///
    /// Returns a map of `key → value` for every key that exists.
    pub async fn gets(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        user: &str,
        keys: &[String],
    ) -> mongodb::error::Result<serde_json::Map<String, Value>> {
        use futures::TryStreamExt;

        let filter = doc! {
            "account": account,
            "project": project,
            "user":    user,
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

        // For any keys that don't have a user-specific value, check for defaults.
        for key in keys {
            if !map.contains_key(key)
                && let Some(default) = DataEntryDefault::get(db, account, project, key).await?
            {
                map.insert(key.clone(), default);
            }
        }

        Ok(map)
    }

    /// Get all data entries for a user in a given scope.
    ///
    /// Returns a map of `key → value` for every key the user has.
    pub async fn get_all(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        user: &str,
    ) -> mongodb::error::Result<serde_json::Map<String, Value>> {
        use futures::TryStreamExt;

        let filter = doc! {
            "account": account,
            "project": project,
            "user":    user,
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

        // For any keys that don't have a user-specific value, check for defaults.
        let defaults = DataEntryDefault::get_all(db, account, project).await?;
        for (key, default) in defaults {
            if !map.contains_key(&key) {
                map.insert(key, default);
            }
        }

        Ok(map)
    }

    /// Get specific keys for multiple users at once.
    ///
    /// Returns a map of `user → { key → value }`.
    pub async fn get_many(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        users: &[String],
        keys: Option<&[String]>,
    ) -> mongodb::error::Result<serde_json::Map<String, Value>> {
        use futures::TryStreamExt;

        let mut filter = doc! {
            "account": account,
            "project": project,
            "user":    { "$in": users },
        };

        if let Some(keys) = keys {
            filter.insert("key", doc! { "$in": keys });
        }

        let entries: Vec<Self> = Self::collection(db)
            .find(filter)
            .await?
            .try_collect()
            .await?;

        // Group by user
        let mut result = serde_json::Map::new();
        for entry in entries {
            let user_map = result
                .entry(&entry.user)
                .or_insert_with(|| Value::Object(serde_json::Map::new()));

            // Insert the entry's key/value into the user's map.
            if let Value::Object(m) = user_map {
                m.insert(entry.key, entry.value);

                // For any missing keys for this user, check for defaults.
                if let Some(keys) = keys {
                    for key in keys {
                        if !m.contains_key(key)
                            && let Some(default) =
                                DataEntryDefault::get(db, account, project, key).await?
                        {
                            m.insert(key.clone(), default);
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    // ----- Writes -------------------------------------------------------

    /// Set a value for a user, overwriting any existing value.
    pub async fn set(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        user: &str,
        key: &str,
        value: &Value,
    ) -> mongodb::error::Result<()> {
        let filter = doc! {
            "account": account,
            "project": project,
            "user":    user,
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
                "user":    user,
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

    /// Set a value only if the key doesn't exist or the current value is
    /// empty (null, `{}`, or `[]`).
    ///
    /// Returns `true` if the value was written, `false` if a non-empty
    /// value already existed.
    pub async fn set_if_empty(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        user: &str,
        key: &str,
        value: &Value,
    ) -> mongodb::error::Result<bool> {
        // Only match if no entry exists, or the existing value is
        // null / empty object / empty array.
        let filter = doc! {
            "account": account,
            "project": project,
            "user":    user,
            "key":     key,
            "$or": [
                { "value": { "$exists": false } },
                { "value": mongodb::bson::Bson::Null },
                { "value": {} },
                { "value": [] },
            ],
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
                "user":    user,
                "key":     key,
            },
        };

        let options = mongodb::options::UpdateOptions::builder()
            .upsert(true)
            .build();

        let result = Self::collection(db)
            .update_one(filter, update)
            .with_options(options)
            .await?;

        // If nothing matched and nothing was upserted, a non-empty value exists.
        Ok(result.modified_count > 0 || result.upserted_id.is_some())
    }

    /// Clear a single value for a user.
    pub async fn clear_entry(
        db: &mongodb::Database,
        account: &str,
        project: &str,
        user: &str,
        key: &str,
    ) -> mongodb::error::Result<()> {
        let filter = doc! {
            "account": account,
            "project": project,
            "user":    user,
            "key":     key,
        };

        Self::collection(db).delete_one(filter).await.map(|_| ())
    }

    // ----- Indexes ------------------------------------------------------

    /// Create the compound indexes needed for efficient data queries.
    ///
    /// Should be called once at application startup.
    pub async fn ensure_indexes(db: &mongodb::Database) -> mongodb::error::Result<()> {
        use mongodb::IndexModel;

        // Unique entry per (account, project, user, key)
        let unique_entry = IndexModel::builder()
            .keys(doc! {
                "account": 1,
                "project": 1,
                "user":    1,
                "key":     1,
            })
            .options(
                mongodb::options::IndexOptions::builder()
                    .unique(true)
                    .build(),
            )
            .build();

        // Fast lookup of all keys for a user in a scope
        let user_scope = IndexModel::builder()
            .keys(doc! {
                "account": 1,
                "project": 1,
                "user":    1,
            })
            .build();

        Self::collection(db)
            .create_indexes([unique_entry, user_scope])
            .await
            .map(|_| ())
    }
}
