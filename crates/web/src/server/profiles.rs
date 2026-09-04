//! Storing a signed-in visitor's progress.
//!
//! One item per user, keyed by the pseudonymous identifier from
//! [`super::auth::derive_user_id`]. The item holds the same [`Progress`] the
//! browser keeps in local storage and nothing else — no email, no name, no
//! provider identifier, no IP address. Read on its own, the table says that
//! somebody typed at a certain speed; it does not say who.
//!
//! DynamoDB rather than a relational database because the access pattern is
//! exactly one key lookup and one write, and running Postgres for that would
//! cost more per month than everything else in the deployment put together.
//!
//! Items carry a TTL that is pushed forward on every write, so an abandoned
//! profile eventually removes itself instead of being kept indefinitely.

use aws_sdk_dynamodb::types::AttributeValue;

use crate::settings::Progress;

/// How long a profile survives without being touched.
pub const INACTIVITY_TTL_SECONDS: u64 = 2 * 365 * 24 * 60 * 60;

/// Where profiles are kept.
pub enum Profiles {
    /// DynamoDB, for a real deployment.
    Dynamo {
        /// The SDK client.
        client: Box<aws_sdk_dynamodb::Client>,
        /// Table name.
        table: String,
    },
    /// In this process only, for local development and tests.
    Memory(std::sync::Mutex<std::collections::BTreeMap<String, Progress>>),
}

/// Something went wrong talking to the store.
#[derive(Debug)]
pub enum ProfileError {
    /// The underlying store failed.
    Store(String),
    /// The stored document could not be read back.
    Corrupt(String),
}

impl core::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProfileError::Store(why) => write!(f, "profile store: {why}"),
            ProfileError::Corrupt(why) => write!(f, "stored profile is unreadable: {why}"),
        }
    }
}

impl std::error::Error for ProfileError {}

impl Profiles {
    /// Builds the store from the environment.
    pub async fn from_env() -> Profiles {
        match std::env::var("PROFILES_TABLE") {
            Ok(table) if !table.is_empty() => {
                let config = aws_config::load_from_env().await;
                tracing::info!(table, "profiles: DynamoDB");
                Profiles::Dynamo {
                    client: Box::new(aws_sdk_dynamodb::Client::new(&config)),
                    table,
                }
            }
            _ => {
                tracing::warn!(
                    "profiles: in-memory (set PROFILES_TABLE to persist signed-in progress)"
                );
                Profiles::in_memory()
            }
        }
    }

    /// An in-memory store, for tests.
    pub fn in_memory() -> Profiles {
        Profiles::Memory(std::sync::Mutex::new(std::collections::BTreeMap::new()))
    }

    /// Reads a user's progress, or `None` if they have never saved any.
    pub async fn load(&self, user: &str) -> Result<Option<Progress>, ProfileError> {
        match self {
            Profiles::Memory(store) => Ok(store.lock().expect("profiles mutex").get(user).cloned()),
            Profiles::Dynamo { client, table } => {
                let response = client
                    .get_item()
                    .table_name(table)
                    .key("user_id", AttributeValue::S(user.to_owned()))
                    .send()
                    .await
                    .map_err(|e| ProfileError::Store(e.to_string()))?;

                let Some(item) = response.item else {
                    return Ok(None);
                };
                let Some(document) = item.get("progress").and_then(|v| v.as_s().ok()) else {
                    return Ok(None);
                };
                serde_json::from_str(document)
                    .map(Some)
                    .map_err(|e| ProfileError::Corrupt(e.to_string()))
            }
        }
    }

    /// Writes a user's progress, replacing whatever was there.
    pub async fn save(&self, user: &str, progress: &Progress) -> Result<(), ProfileError> {
        match self {
            Profiles::Memory(store) => {
                store
                    .lock()
                    .expect("profiles mutex")
                    .insert(user.to_owned(), progress.clone());
                Ok(())
            }
            Profiles::Dynamo { client, table } => {
                let document = serde_json::to_string(progress)
                    .map_err(|e| ProfileError::Corrupt(e.to_string()))?;
                let now = super::session::now_secs();
                client
                    .put_item()
                    .table_name(table)
                    .item("user_id", AttributeValue::S(user.to_owned()))
                    .item("progress", AttributeValue::S(document))
                    .item("updated_at", AttributeValue::N(now.to_string()))
                    .item(
                        "expires_at",
                        AttributeValue::N((now + INACTIVITY_TTL_SECONDS).to_string()),
                    )
                    .send()
                    .await
                    .map_err(|e| ProfileError::Store(e.to_string()))?;
                Ok(())
            }
        }
    }

    /// Removes a user's profile entirely.
    ///
    /// Offered so that erasure is a button rather than an email: a signed-in
    /// visitor can delete everything held about them without asking anybody.
    pub async fn delete(&self, user: &str) -> Result<(), ProfileError> {
        match self {
            Profiles::Memory(store) => {
                store.lock().expect("profiles mutex").remove(user);
                Ok(())
            }
            Profiles::Dynamo { client, table } => {
                client
                    .delete_item()
                    .table_name(table)
                    .key("user_id", AttributeValue::S(user.to_owned()))
                    .send()
                    .await
                    .map_err(|e| ProfileError::Store(e.to_string()))?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typing_core::{goals::Module, stats::Score};

    fn progress(speed: f64) -> Progress {
        let mut progress = Progress::default();
        progress.record(
            Module::Velocity,
            &Score {
                accuracy: 98.0,
                speed,
                fluidness: None,
                touches: 600,
                errors: 0,
                seconds: 60.0,
            },
            1,
        );
        progress
    }

    #[tokio::test]
    async fn an_unknown_user_has_no_profile() {
        let store = Profiles::in_memory();
        assert_eq!(store.load("nobody").await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_profile_round_trips() {
        let store = Profiles::in_memory();
        let saved = progress(55.0);
        store.save("user-a", &saved).await.unwrap();
        assert_eq!(store.load("user-a").await.unwrap(), Some(saved));
    }

    #[tokio::test]
    async fn saving_replaces_rather_than_accumulates() {
        let store = Profiles::in_memory();
        store.save("user-a", &progress(55.0)).await.unwrap();
        store.save("user-a", &progress(70.0)).await.unwrap();
        let loaded = store.load("user-a").await.unwrap().unwrap();
        assert_eq!(loaded.best_for(Module::Velocity).unwrap().speed, 70.0);
    }

    #[tokio::test]
    async fn profiles_do_not_leak_between_users() {
        let store = Profiles::in_memory();
        store.save("user-a", &progress(55.0)).await.unwrap();
        store.save("user-b", &progress(70.0)).await.unwrap();
        assert_eq!(
            store
                .load("user-a")
                .await
                .unwrap()
                .unwrap()
                .best_for(Module::Velocity)
                .unwrap()
                .speed,
            55.0
        );
    }

    #[tokio::test]
    async fn deleting_removes_everything_held() {
        let store = Profiles::in_memory();
        store.save("user-a", &progress(55.0)).await.unwrap();
        store.delete("user-a").await.unwrap();
        assert_eq!(store.load("user-a").await.unwrap(), None);
    }

    #[tokio::test]
    async fn deleting_a_profile_that_is_not_there_is_not_an_error() {
        let store = Profiles::in_memory();
        assert!(store.delete("nobody").await.is_ok());
    }
}
