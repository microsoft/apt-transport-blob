// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
use std::sync::Arc;

use azure_identity::{
    AzureCliCredential, DefaultAzureCredential, DefaultAzureCredentialEnum, EnvironmentCredential,
    ImdsManagedIdentityCredential,
};
use azure_storage::StorageCredentials;
use azure_storage_blobs::{
    blob::operations::GetPropertiesResponse,
    prelude::{BlobClient, ClientBuilder},
};
use log::debug;
use url::Url;

#[derive(Debug)]
pub struct AzureBlob {
    pub account: String,
    pub container_name: String,
    pub blob_name: String,
    blob_client: BlobClient,
}

impl AzureBlob {
    pub fn new_from_url(
        azure_registry: &AzureRegistry,
        url: &Url,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let host = url.host_str().ok_or("No host")?;
        let mut path_segments = url.path_segments().ok_or("No path segments")?;
        let container_name = path_segments.next().ok_or("No container")?;
        let blob_name = path_segments.collect::<Vec<_>>().join("/");
        let account = host.trim_end_matches(".blob.core.windows.net");

        let blob_client = azure_registry.get_blob_client(account, container_name, &blob_name);

        Ok(AzureBlob {
            account: account.to_string(),
            container_name: container_name.to_string(),
            blob_name,
            blob_client,
        })
    }

    pub async fn exists(&self) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(self.blob_client.exists().await?)
    }

    pub async fn properties(&self) -> Result<GetPropertiesResponse, Box<dyn std::error::Error>> {
        Ok(self.blob_client.get_properties().await?)
    }

    pub async fn uri_start_fields(&self) -> Result<(u64, String), Box<dyn std::error::Error>> {
        // Return the size and the last modified time
        let properties = self.properties().await?;
        Ok((
            properties.blob.properties.content_length,
            properties.blob.properties.last_modified.to_string(),
        ))
    }

    pub(crate) async fn download(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(self.blob_client.get_content().await?)
    }
}

pub(crate) struct AzureRegistry {
    credential: Arc<DefaultAzureCredential>,
}

impl AzureRegistry {
    pub fn new() -> Self {
        // Get a credential for Azure
        //
        // Prioritise AzureCli above ManagedIdentity - this makes local operation
        // work a lot faster.
        let sources = vec![
            DefaultAzureCredentialEnum::Environment(EnvironmentCredential::default()),
            DefaultAzureCredentialEnum::AzureCli(AzureCliCredential::new()),
            DefaultAzureCredentialEnum::ManagedIdentity(ImdsManagedIdentityCredential::default()),
        ];
        let credential = DefaultAzureCredential::with_sources(sources);
        AzureRegistry {
            credential: Arc::new(credential),
        }
    }

    pub fn get_blob(&self, url: &Url) -> Result<AzureBlob, Box<dyn std::error::Error>> {
        AzureBlob::new_from_url(self, url)
    }

    pub fn get_blob_client(
        &self,
        account: &str,
        container_name: &str,
        blob_name: &str,
    ) -> BlobClient {
        // Check to see if an AZURE_STORAGE_BEARER_TOKEN is set. This is a token with the
        // storage.azure.com scope. It's prioritised over user credentials.
        let storage_credentials = match std::env::var("AZURE_STORAGE_BEARER_TOKEN") {
            Ok(token) => {
                debug!("Using storage bearer token for accessing {}", account);
                StorageCredentials::bearer_token(token)
            }
            Err(_) => {
                debug!("Using token credentials for accessing {}", account);
                StorageCredentials::token_credential(self.credential.clone())
            }
        };

        // Get the client builder.
        ClientBuilder::new(account, storage_credentials).blob_client(container_name, blob_name)
    }
}
