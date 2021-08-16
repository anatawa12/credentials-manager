use crate::utils::*;
use crate::{RepoSecrets, SecretMap};
use reqwest::{Client, IntoUrl, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use sodiumoxide::crypto::box_::PublicKey;

macro_rules! page_getter_struct {
    ($name: ident, $count: ident, $list: ident, $elem: ty) => {
        #[derive(Deserialize)]
        struct $name {
            $count: usize,
            $list: Vec<$elem>,
        }

        impl PagedData for $name {
            type Item = $elem;

            fn get_total_count(&self) -> usize {
                self.$count
            }

            fn list(self) -> Vec<Self::Item> {
                self.$list
            }
        }
    };
}

pub(crate) async fn set_secrets(
    client: &MyClient,
    secret_map: &SecretMap,
    repo: &str,
    env: &str,
    secrets: &RepoSecrets,
) -> Result<(), Box<dyn std::error::Error>> {
    let existing_secrets = get_secrets_or_make_environment(client, repo, env).await?;

    // if additional secrets are not allowed, remove additional secrets
    if !secrets.additional {
        existing_secrets
            .iter()
            .filter(|s| !secrets.props.contains(*s))
            .map(|n| remove_secret(client, repo, env, n))
            .try_joining_all()
            .await?;
    }

    let key = get_secret_public_key(client, repo, env).await?;

    secrets
        .props
        .iter()
        .map(|n| add_secrets(client, repo, env, &key, n, secret_map.get(n).unwrap()))
        .try_joining_all()
        .await?;

    Ok(())
}

async fn get_secrets_or_make_environment(
    client: &MyClient,
    repo: &str,
    env: &str,
) -> reqwest::Result<HashSet<String>> {
    page_getter_struct!(Secrets, total_count, secrets, Secret);
    #[derive(Deserialize)]
    struct Secret {
        name: String,
    }

    let secrets = match page_getter::<_, _, Secrets>(client, |page| {
        format!(
            "https://api.github.com/repos/{}/environments/{}/secrets?per_page=100&page={}",
            repo, env, page
        )
    })
    .await
    {
        Err(ref e) if matches!(e.status(), Some(StatusCode::OK)) => {
            make_environment(client, repo, env).await?;
            return Ok(HashSet::new());
        }
        Err(e) => return Err(e),
        Ok(vec) => vec,
    };

    Ok(secrets.into_iter().map(|x| x.name).collect())
}

async fn make_environment(client: &MyClient, repo: &str, env: &str) -> reqwest::Result<()> {
    client
        .take()
        .await
        .put(&format!(
            "https://api.github.com/repos/{}/environments/{}",
            repo, env
        ))
        .body("{}")
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

async fn remove_secret(
    client: &MyClient,
    repo: &str,
    env: &str,
    secret: &str,
) -> reqwest::Result<()> {
    client
        .take()
        .await
        .delete(&format!(
            "https://api.github.com/repos/{}/environments/{}/secrets/{}",
            repo, env, secret
        ))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

#[derive(Deserialize)]
struct SecretPublicKey {
    key_id: String,
    #[serde(serialize_with = "key_as_base64", deserialize_with = "key_from_base64")]
    key: PublicKey,
}

async fn get_secret_public_key(client: &MyClient, repo: &str, env: &str) -> reqwest::Result<SecretPublicKey> {
    client.take()
        .await
        .get(&format!("https://api.github.com/repos/{}/environments/{}/secrets/public-key", repo, env))
        .send()
        .await?
        .error_for_status()?
        .json::<SecretPublicKey>()
        .await
}

async fn add_secrets(client: &MyClient, repo: &str, env: &str, key: &SecretPublicKey, name: &str, value: &str) -> reqwest::Result<()> {
    let sealed_data = sodiumoxide::crypto::sealedbox::seal(value.as_bytes(), &key.key);

    #[derive(Serialize)]
    struct ApiRequest<'a> {
        key_id: &'a str,
        #[serde(serialize_with = "as_base64")]
        encrypted_value: Vec<u8>,
    }

    client
        .take()
        .await
        .put(&format!(
            "https://api.github.com/repos/{}/environments/{}/secrets/{}",
            repo, env, name
        ))
        .json(&ApiRequest {
            key_id: &key.key_id,
            encrypted_value: sealed_data,
        })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

// utils

async fn page_getter<F, I, T>(
    client: &MyClient,
    mut url_builder: F,
) -> reqwest::Result<Vec<T::Item>>
where
    F: FnMut(usize) -> I,
    I: IntoUrl,
    T: DeserializeOwned + PagedData,
{
    async fn page_getter_impl<F, I, T>(
        client: &Client,
        mut url_builder: F,
    ) -> reqwest::Result<Option<Vec<T::Item>>>
    where
        F: FnMut(usize) -> I,
        I: IntoUrl,
        T: DeserializeOwned + PagedData,
    {
        let mut total_count: usize = usize::MAX;
        let mut vec = Vec::<T::Item>::new();

        for i in 1..usize::MAX {
            let result: T = client
                .get(url_builder(i))
                .send()
                .await?
                .error_for_status()?
                .json::<T>()
                .await?;

            if total_count == usize::MAX {
                total_count = result.get_total_count()
            }
            if total_count != result.get_total_count() {
                // count changed: retry
                return Ok(None);
            }

            vec.extend(result.list());

            if vec.len() == total_count {
                return Ok(Some(vec));
            }
        }

        panic!("too many pages")
    }
    let client = client.take().await;

    loop {
        if let Some(vec) = page_getter_impl::<&mut F, I, T>(&client, &mut url_builder).await? {
            return Ok(vec);
        }
    }
}

trait PagedData {
    type Item;
    fn get_total_count(&self) -> usize;
    fn list(self) -> Vec<Self::Item>;
}
