use std::{
    env,
    error::Error,
    fmt::{self, Display},
    future::Future,
};

use axum::async_trait;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

pub type AnyError = Box<dyn std::error::Error + Send + Sync>;

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[async_trait]
pub trait KVTrait {
    async fn get<B>(&self, key: &str) -> Result<B, AnyError>
    where
        B: serde::Serialize,
        B: serde::de::DeserializeOwned;
    async fn set<B>(&self, key: &str, value: &B, expire: u64) -> Result<(), AnyError>
    where
        B: Sync,
        B: serde::Serialize,
        B: serde::de::DeserializeOwned;
    async fn del(&self, key: &str) -> Result<(), AnyError>;
}

#[derive(Debug)]
pub struct NotFoundError {}
impl Display for NotFoundError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Not found")
    }
}
impl Error for NotFoundError {}
pub fn not_found_error() -> Result<(), NotFoundError> {
    Err(NotFoundError {})
}

pub fn normailze_key(key: &str) -> String {
    let key = key
        .to_string()
        .replace('/', "-")
        .replace('\\', "-")
        .replace(':', "-")
        .replace('*', "-")
        .replace('?', "-")
        .replace('\"', "-")
        .replace('<', "-")
        .replace('>', "-")
        .replace('|', "-")
        .replace('.', "-")
        .replace('@', "-")
        .replace('_', "-");
    let prrefix = env::var("TOKI_KV_PREFIX").unwrap_or_else(|_| "".into());
    return format!("{}{}", prrefix, key);
}

#[derive(Debug, Clone)]
pub struct KVFilesystem {
    path: String,
}
#[derive(Serialize, Deserialize)]
pub struct KVFilesystemJsonData<T>
where
    T: Serialize,
{
    data: T,
    expire: u64,
}

impl KVFilesystem {
    pub fn new(path: &str) -> KVFilesystem {
        KVFilesystem {
            path: path.to_string(),
        }
    }
}

#[async_trait]
impl KVTrait for KVFilesystem {
    async fn get<B>(&self, key: &str) -> Result<B, AnyError>
    where
        B: serde::Serialize,
        B: serde::de::DeserializeOwned,
    {
        let path = format!("{}/{}.json", self.path, key);
        let contents = tokio::fs::read_to_string(path).await;
        match contents {
            Ok(contents) => {
                let json: KVFilesystemJsonData<B> = serde_json::from_str(&contents)?;
                if json.expire > 0 && json.expire < now() {
                    not_found_error()?;
                }
                Ok(json.data)
            }
            Err(_) => Err(Box::new(NotFoundError {})),
        }
    }
    async fn set<B>(&self, key: &str, value: &B, expire: u64) -> Result<(), AnyError>
    where
        B: Sync,
        B: serde::Serialize,
        B: serde::de::DeserializeOwned,
    {
        let path = format!("{}/{}.json", self.path, key);
        let data = KVFilesystemJsonData {
            data: value,
            expire: expire + now(),
        };
        let contents = serde_json::to_string(&data)?;
        tokio::fs::write(path, contents).await?;
        Ok(())
    }
    async fn del(&self, key: &str) -> Result<(), AnyError> {
        let path = format!("{}/{}.json", self.path, key);
        tokio::fs::remove_file(path).await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct KVRedis {
    redis: redis::Client,
}
impl KVRedis {
    pub fn new(redis: redis::Client) -> KVRedis {
        KVRedis { redis }
    }
}
#[async_trait]
impl KVTrait for KVRedis {
    async fn get<B>(&self, key: &str) -> Result<B, AnyError>
    where
        B: serde::Serialize,
        B: serde::de::DeserializeOwned,
    {
        let mut con = self.redis.get_async_connection().await?;
        let value: redis::Value = con.get(key).await?;
        let res: B;
        match value {
            redis::Value::Data(data) => {
                res = serde_json::from_slice(&data)?;
                Ok(res)
            }
            _ => Err(Box::new(NotFoundError {})),
        }
    }
    async fn set<B>(&self, key: &str, value: &B, expire: u64) -> Result<(), AnyError>
    where
        B: Sync,
        B: serde::Serialize,
        B: serde::de::DeserializeOwned,
    {
        let mut con = self.redis.get_async_connection().await?;
        let data = serde_json::to_string(value)?;
        con.set_ex(key, data, expire as usize).await?;
        Ok(())
    }
    async fn del(&self, key: &str) -> Result<(), AnyError> {
        let mut con = self.redis.get_async_connection().await?;
        con.del(key).await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum KVManager {
    KVFilesystem(KVFilesystem),
    KVRedis(KVRedis),
}
impl KVManager {
    pub fn new(conn: String) -> Result<KVManager, AnyError> {
        if conn.starts_with("file:") {
            return Ok(KVManager::KVFilesystem(KVFilesystem::new(
                conn.strip_prefix("file:").unwrap(),
            )));
        }
        if conn.starts_with("redis:") || conn.starts_with("redis+unix:") {
            let redis = redis::Client::open(conn)?;
            return Ok(KVManager::KVRedis(KVRedis::new(redis)));
        }
        panic!("unsupported kv connection");
    }
    #[tracing::instrument(skip(self))]
    pub async fn get<B>(&self, key: &str) -> Result<B, AnyError>
    where
        B: serde::Serialize,
        B: serde::de::DeserializeOwned,
    {
        match self {
            KVManager::KVFilesystem(kv) => kv.get(&normailze_key(key)).await,
            KVManager::KVRedis(kv) => kv.get(&normailze_key(key)).await,
        }
    }
    pub async fn get_some<B>(&self, key: &str) -> Result<Option<B>, AnyError>
    where
        B: serde::Serialize,
        B: serde::de::DeserializeOwned,
    {
        let res = self.get::<B>(key).await;
        match res {
            Ok(d) => Ok(Some(d)),
            Err(e) => {
                if e.is::<NotFoundError>() {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }
    pub async fn get_or<B>(&self, key: &str, default: B) -> Result<B, AnyError>
    where
        B: serde::Serialize,
        B: serde::de::DeserializeOwned,
    {
        let res = self.get::<B>(key).await;
        match res {
            Ok(d) => Ok(d),
            Err(e) => {
                if e.is::<NotFoundError>() {
                    Ok(default)
                } else {
                    Err(e)
                }
            }
        }
    }
    #[tracing::instrument(skip(self, value, expire))]
    pub async fn set<B>(&self, key: &str, value: &B, expire: u64) -> Result<(), AnyError>
    where
        B: Sync,
        B: serde::Serialize,
        B: serde::de::DeserializeOwned,
    {
        match self {
            KVManager::KVFilesystem(kv) => kv.set(&normailze_key(key), value, expire).await,
            KVManager::KVRedis(kv) => kv.set(&normailze_key(key), value, expire).await,
        }
    }
    #[tracing::instrument(skip(self))]
    pub async fn del(&self, key: &str) -> Result<(), AnyError> {
        match self {
            KVManager::KVFilesystem(kv) => kv.del(&normailze_key(key)).await,
            KVManager::KVRedis(kv) => kv.del(&normailze_key(key)).await,
        }
    }

    pub async fn get_or_init<B, F>(
        &self,
        key: &str,
        init: impl FnOnce() -> F,
        expire: u64,
    ) -> Result<KvGetOrInitResult<B>, AnyError>
    where
        F: Future<Output = Result<B, AnyError>>,
        B: serde::Serialize,
        B: serde::de::DeserializeOwned,
        B: Clone,
        B: Sync,
    {
        let value = self.get_some(key).await?;

        match value {
            Some(v) => Ok(KvGetOrInitResult {
                value: v,
                hit: true,
            }),
            None => {
                let value = init().await?;
                self.set(key, &value, expire).await?;
                Ok(KvGetOrInitResult { value, hit: false })
            }
        }
    }
}

pub struct KvGetOrInitResult<B> {
    pub value: B,
    pub hit: bool,
}
