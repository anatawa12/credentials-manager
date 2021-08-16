use reqwest::Client;
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::Semaphore;
use futures::future::{
    TryJoinAll,
    try_join_all,
};
use futures::TryFuture;
use sodiumoxide::crypto::box_::PublicKey;
use serde::{Serializer, Deserializer, Deserialize};

#[derive(Clone)]
pub(crate) struct MyClient {
    client: Client,
    semaphore: Arc<Semaphore>,
}

pub(crate) struct ClientWrap<'a> {
    client: &'a Client,
    // for dropping
    _semaphore: tokio::sync::SemaphorePermit<'a>,
}

impl MyClient {
    pub(crate) fn new(client: Client, permits: usize) -> Self {
        Self {
            client,
            semaphore: Arc::new(Semaphore::new(permits)),
        }
    }

    pub(crate) async fn take(&self) -> ClientWrap<'_> {
        ClientWrap {
            client: &self.client,
            _semaphore: self.semaphore.acquire().await.unwrap(),
        }
    }
}

impl<'a> Deref for ClientWrap<'a> {
    type Target = Client;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

pub trait IterExt : IntoIterator + Sized {
    fn try_joining_all(self) -> TryJoinAll<Self::Item> where Self::Item: TryFuture, {
        try_join_all(self)
    }
}

impl <T : IntoIterator> IterExt for T {}

pub(crate) fn as_base64<S>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer
{
    serializer.serialize_str(&base64::encode(value))
}

pub(crate) fn key_from_base64<'de, D>(deserializer: D) -> Result<PublicKey, D::Error>
    where D: Deserializer<'de>
{
    use serde::de::Error;
    String::deserialize(deserializer)
        .and_then(|string| base64::decode(&string).map_err(|err| Error::custom(err.to_string())))
        .map(|bytes| PublicKey::from_slice(&bytes))
        .and_then(|opt| opt.ok_or_else(|| Error::custom("failed to deserialize public key")))
}
