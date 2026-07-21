use std::fs;
use std::path::PathBuf;

use app_lib::sync::github_gist::GithubGistRemote;
use app_lib::sync::service::{SyncProvider, SyncService, SyncServiceError};
use app_lib::vault::{AuthType, CreateProfileRequest, Vault};

const TEST_PASSWORD: &str = "live sync test password";

fn test_directory(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("mjjssh-live-sync-{label}-{}", uuid::Uuid::new_v4()))
}

fn profile(name: &str) -> CreateProfileRequest {
    CreateProfileRequest {
        name: name.into(),
        host: format!("{name}.example.test"),
        port: Some(22),
        username: "root".into(),
        auth_type: AuthType::Password,
        credential: Some("test-password".into()),
        key_id: None,
        group_name: None,
        icon: None,
        color: None,
        os: None,
        location: None,
    }
}

#[tokio::test]
#[ignore = "requires MJJSSH_GITHUB_GIST_TOKEN and creates then deletes a private GitHub Gist"]
async fn synchronizes_two_local_vaults_and_detects_conflicts() {
    let token = std::env::var("MJJSSH_GITHUB_GIST_TOKEN")
        .expect("MJJSSH_GITHUB_GIST_TOKEN must be set for live sync validation");
    let existing = GithubGistRemote::new()
        .expect("create GitHub Gist client")
        .find_sync_vaults(&token)
        .await
        .expect("list existing MJJSSH GitHub Gists");
    assert!(
        existing.is_empty(),
        "live test requires a dedicated token with no MJJSSH cloud sync Gist"
    );

    let first_directory = test_directory("first");
    let second_directory = test_directory("second");
    let replacement_directory = test_directory("replacement");
    let mut created_gist_id = None;

    let result = async {
        let first = Vault::open(&first_directory)?;
        first.create_profile(&profile("first-host"))?;
        let first_service = SyncService::new(&first, &first_directory)?;
        let initial_status = first_service
            .enable_create(SyncProvider::GithubGist, &token, TEST_PASSWORD.into())
            .await?;
        let gist_id = initial_status.remote_id.expect("created Gist ID");
        created_gist_id = Some(gist_id.clone());

        let second = Vault::open(&second_directory)?;
        let second_service = SyncService::new(&second, &second_directory)?;
        second_service
            .enable_or_import(SyncProvider::GithubGist, &token, TEST_PASSWORD.into())
            .await?;
        assert_eq!(second.list_profiles()?.len(), 1);

        second.create_profile(&profile("second-host"))?;
        second_service.upload(&token).await?;
        first_service.download(&token).await?;
        assert_eq!(first.list_profiles()?.len(), 2);

        first.create_profile(&profile("first-conflict"))?;
        second.create_profile(&profile("second-conflict"))?;
        second_service.upload(&token).await?;
        assert!(matches!(
            first_service.download(&token).await,
            Err(SyncServiceError::Conflict)
        ));

        let replacement = Vault::open(&replacement_directory)?;
        replacement.create_profile(&profile("replacement-host"))?;
        let replacement_service = SyncService::new(&replacement, &replacement_directory)?;
        let replacement_status = replacement_service
            .enable_create(SyncProvider::GithubGist, &token, TEST_PASSWORD.into())
            .await?;
        assert_eq!(
            replacement_status.remote_id.as_deref(),
            Some(gist_id.as_str())
        );
        assert_eq!(
            GithubGistRemote::new()?
                .find_sync_vaults(&token)
                .await?
                .len(),
            1
        );

        Ok::<(), SyncServiceError>(())
    }
    .await;

    if let Some(gist_id) = created_gist_id {
        let remote = GithubGistRemote::new().unwrap();
        if let Err(error) = remote.delete(&token, &gist_id).await {
            panic!("live sync validation could not delete its test Gist: {error}");
        }
    }
    let _ = fs::remove_dir_all(&first_directory);
    let _ = fs::remove_dir_all(&second_directory);
    let _ = fs::remove_dir_all(&replacement_directory);
    result.unwrap();
}
