use mongodb::bson::Bson;

use super::{account_id::AccountId, write_key::WriteKey};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub struct AccountEntry {
    /// See: [`AccountId`](AccountId)
    pub id: AccountId,

    /// See: [`WriteKey`](WriteKey)
    pub write_key: WriteKey,

    /// The user's display name in-game.
    pub username: String,

    /// A list of account usernames that are friends with this account.
    ///
    /// Home might periodically update this list.
    pub buddylist: Vec<String>,

    /// Age of the user's account, in years, e.g. 18 for 18 years old.
    /// TODO: validate this
    pub age: Option<u32>,

    /// Gender of the user's account, e.g. "male" or "female".
    /// TODO: make this an enum
    pub gender: Option<String>,

    /// The user's region, e.g. "en-US", "fr-FR", etc.
    /// TODO: make this an enum or use a crate for locale identifiers
    pub region: Option<String>,

    /// The user's territory, e.g. "SCEA", "SCEE", etc.
    /// TODO: make this an enum
    pub territory: Option<String>,
}

impl AccountEntry {
    pub async fn new(username: &str) -> Self {
        Self {
            id: AccountId::generate(),
            write_key: WriteKey::generate(),

            username: username.to_string(),
            buddylist: Vec::new(),
            age: None,
            gender: None,
            region: None,
            territory: None,
        }
    }

    /// Returns the MongoDB collection name for accounts.
    const fn collection_name() -> &'static str {
        "AccountEntry"
    }

    /// Returns a handle to the account collection.
    fn collection(db: &mongodb::Database) -> mongodb::Collection<Self> {
        db.collection(Self::collection_name())
    }

    /// Read an account from the database by a specific field.
    ///
    /// Typically used with `("username", username)` or `("id", id)`.
    pub async fn read_by(
        db: &mongodb::Database,
        (param, value): (String, Bson),
    ) -> mongodb::error::Result<Option<Self>> {
        Self::collection(db)
            .find_one(mongodb::bson::doc! { param: value })
            .await
    }

    /// Insert or update this account in the database.
    pub async fn upsert(&self, db: &mongodb::Database) -> mongodb::error::Result<()> {
        let options = mongodb::options::ReplaceOptions::builder()
            .upsert(true)
            .build();

        Self::collection(db)
            .replace_one(mongodb::bson::doc! { "id": self.id }, self)
            .with_options(options)
            .await
            .map(|_| ())
    }

    // ----- Indexes ------------------------------------------------------

    /// Ensure required indexes exist.
    pub async fn ensure_indexes(db: &mongodb::Database) -> mongodb::error::Result<()> {
        use mongodb::IndexModel;
        use mongodb::bson::doc;

        let collection = Self::collection(db);

        let indexes = vec![
            // Unique compound key for user identity
            IndexModel::builder()
                .keys(doc! {
                    "id": 1,
                    "username": 1,
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
