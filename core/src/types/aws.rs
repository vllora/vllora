use aws_config::{sts::AssumeRoleProvider, Region};
use aws_sdk_bedrock::config::Credentials;

use super::credentials::AwsIAMCredentials;

pub async fn get_user_shared_config(credentials: AwsIAMCredentials) -> aws_config::ConfigLoader {
    let region_name = credentials
        .region
        .unwrap_or(std::env::var("AWS_DEFAULT_REGION").map_or("us-east-1".to_string(), |r| r));
    let region = Region::new(region_name);
    let credentials = Credentials::new(
        credentials.access_key,
        credentials.access_secret,
        None,              // optional session token
        None,              // optional expiration time
        "langdb-provider", // optional provider name
    );
    let shared_config =
        aws_config::defaults(aws_config::BehaviorVersion::latest()).region(region.clone());
    shared_config.credentials_provider(credentials)
}

pub async fn get_shared_config(region: Option<Region>) -> aws_config::ConfigLoader {
    let region = region.clone().unwrap_or(Region::new(
        std::env::var("AWS_DEFAULT_REGION").unwrap_or("ap-southeast-1".into()),
    ));
    let shared_config =
        aws_config::defaults(aws_config::BehaviorVersion::latest()).region(region.clone());

    let config = match std::env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI") {
        Ok(_) => shared_config
            .credentials_provider(aws_config::ecs::EcsCredentialsProvider::builder().build()),
        Err(_) => match std::env::var("AWS_ASSUME_ROLE_ARN") {
            Ok(role) => {
                let provider = AssumeRoleProvider::builder(role)
                    .session_name("textract-session")
                    .build()
                    .await;

                shared_config.credentials_provider(provider)
            }
            Err(_) => aws_config::from_env(),
        },
    };

    config.region(region)
}
