mod set_secrets;
mod utils;

use crate::utils::*;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    sodiumoxide::init().unwrap();
    eprintln!("loading secrets");
    let secret_map: SecretMap = load_secret_map().await?;
    eprintln!("loading config");
    let config: ConfigRepos = serde_yaml::from_slice(&std::fs::read("config.yml")?)?;

    // first verify secrets
    check_secrets(&secret_map, &config);

    let token = secret_map.get("ACTIONS_PAT").expect("ACTIONS_PAT env variable");
    let mut default_headers = HeaderMap::new();
    default_headers.insert("accept", header_from_str("application/vnd.github.v3+json"));
    default_headers.insert(
        "authorization",
        header_from_str(&format!("token {}", token)),
    );
    let client = Client::builder()
        .user_agent("credentials-manager")
        .default_headers(default_headers)
        .build()
        .unwrap();
    let client = MyClient::new(client, 8);

    // last, set secrets via api
    for (repo, repo_envs) in config {
        for (env, secrets) in repo_envs {
            set_secrets::set_secrets(&client, &secret_map, &repo, &env, &secrets).await?;
        }
    }

    Ok(())
}

async fn load_secret_map() -> std::io::Result<SecretMap> {
    if let Ok(secrets) = env::var("INPUT_SECRETS") {
        Ok(serde_json::from_str(&secrets)?)
    } else {
        Ok(serde_json::from_slice(&tokio::fs::read(".credentials.json").await?)?)
    }
}

fn header_from_str(str: &str) -> HeaderValue {
    HeaderValue::from_str(str).unwrap()
}

fn check_secrets(secrets: &SecretMap, config: &ConfigRepos) {
    let not_founds = config
        .values()
        .flat_map(|e| e.values())
        .flat_map(|e| &e.props)
        .filter(|value| !secrets.contains_key(*value))
        .collect::<HashSet<_>>();

    if not_founds.is_empty() {
        return;
    }

    eprint!("secrets ");
    for x in not_founds {
        eprint!("{}, ", x);
    }
    eprintln!("are not found");
    panic!("some secret not found")
}

// secret name -> value
type SecretMap = HashMap<String, String>;

// repository name(user/name) -> repository
type ConfigRepos = HashMap<String, Repository>;
// environment -> secrets
type Repository = HashMap<String, RepoSecrets>;

#[derive(Deserialize)]
struct RepoSecrets {
    props: HashSet<String>,
    #[serde(default)]
    additional: bool,
}
