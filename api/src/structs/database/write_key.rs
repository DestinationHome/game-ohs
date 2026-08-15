/// A unique string that serves as the user's login credential.
///
/// A user must supply this key in order to perform write operations on their account.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct WriteKey([u8; 4]);
impl WriteKey {
    pub fn generate() -> Self {
        Self(rand::random())
    }
}

impl std::fmt::Display for WriteKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}
