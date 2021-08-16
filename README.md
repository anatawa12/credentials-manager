# Credentials manager

The tool to send credentials to environments of repositories.

## how to use for you

1. fork this repository
2. rewrite config.yml
3. set your secrets to secrets of environment named 'secrets'. 
   secret named `ACTIONS_PAT` will be used to set repositories secrets so required.
4. run Send Values(send-values.yml) via workflow_dispatch

## format of send-values.yml

```typescript
// the key is repository name lile 'anatawa12/credentials-manager'
type ConfigYaml = Record<string, RepositoryConfig>;
// the key is environment name in your repository
type RepositoryConfig = Record<String, RepoSecrets>;

interface RepoSecrets {
    // the list of secrets. this will be the name of secret of 
    // both this repository and target repository.
    props: string[],
    // true if you want to allow secrets which does not listed on props
    // if false, this tool will remove secrets.
    additional: boolean,
}
```
