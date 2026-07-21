use std::fs;
use std::path::PathBuf;

use app_lib::sync::gitee_snippet::GiteeSnippetRemote;
use app_lib::sync::service::{SyncProvider, SyncService, SyncServiceError};
use app_lib::vault::{AuthType, CreateProfileRequest, Vault};

const TEST_PASSWORD: &str = "live sync test password";
const UPDATED_TEST_PASSWORD: &str = "updated live sync test password";

fn test_directory(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mjjssh-gitee-live-sync-{label}-{}",
        uuid::Uuid::new_v4()
    ))
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
#[ignore = "requires MJJSSH_GITEE_TOKEN and creates then deletes a private Gitee snippet"]
async fn synchronizes_two_local_vaults_and_detects_conflicts() {
    let token = std::env::var("MJJSSH_GITEE_TOKEN")
        .expect("MJJSSH_GITEE_TOKEN must be set for live sync validation");
    let existing = GiteeSnippetRemote::new()
        .expect("create Gitee snippet client")
        .find_sync_vaults(&token)
        .await
        .expect("list existing MJJSSH Gitee snippets");
    assert!(
        existing.is_empty(),
        "live test requires a dedicated token with no MJJSSH cloud sync snippet"
    );

    let first_directory = test_directory("first");
    let second_directory = test_directory("second");
    let replacement_directory = test_directory("replacement");
    let mut created_snippet_id = None;

    let result = async {
        let first = Vault::open(&first_directory)?;
        first.create_profile(&profile("first-host"))?;
        let first_service = SyncService::new(&first, &first_directory)?;
        let initial_status = first_service
            .enable_create(SyncProvider::GiteeSnippet, &token, TEST_PASSWORD.into())
            .await?;
        let snippet_id = initial_status.remote_id.expect("created Gitee snippet ID");
        created_snippet_id = Some(snippet_id.clone());

        let second = Vault::open(&second_directory)?;
        let second_service = SyncService::new(&second, &second_directory)?;
        second_service
            .enable_or_import(SyncProvider::GiteeSnippet, &token, TEST_PASSWORD.into())
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
        first_service.resolve_accept_remote(&token).await?;
        assert_eq!(first.list_profiles()?.len(), 3);
        assert_eq!(
            fs::read_dir(first_directory.join("sync-conflicts"))
                .expect("first conflict backup directory")
                .count(),
            2
        );

        first_service
            .change_password(&token, TEST_PASSWORD.into(), UPDATED_TEST_PASSWORD.into())
            .await?;
        second_service
            .refresh_derived_key(&token, UPDATED_TEST_PASSWORD.into())
            .await?;
        second_service.download(&token).await?;
        assert_eq!(second.list_profiles()?.len(), 3);

        first.create_profile(&profile("first-keep-local"))?;
        second.create_profile(&profile("second-keep-remote"))?;
        second_service.upload(&token).await?;
        assert!(matches!(
            first_service.download(&token).await,
            Err(SyncServiceError::Conflict)
        ));
        first_service.resolve_keep_local(&token).await?;
        assert_eq!(first.list_profiles()?.len(), 4);
        assert_eq!(
            fs::read_dir(first_directory.join("sync-conflicts"))
                .expect("first conflict backup directory")
                .count(),
            4
        );

        second_service.resolve_accept_remote(&token).await?;
        assert_eq!(second.list_profiles()?.len(), 4);
        assert!(second
            .list_profiles()?
            .iter()
            .any(|profile| profile.name == "first-keep-local"));
        assert!(!second
            .list_profiles()?
            .iter()
            .any(|profile| profile.name == "second-keep-remote"));
        assert_eq!(
            fs::read_dir(second_directory.join("sync-conflicts"))
                .expect("second conflict backup directory")
                .count(),
            2
        );

        let replacement = Vault::open(&replacement_directory)?;
        replacement.create_profile(&profile("replacement-host"))?;
        let replacement_service = SyncService::new(&replacement, &replacement_directory)?;
        let replacement_status = replacement_service
            .enable_create(
                SyncProvider::GiteeSnippet,
                &token,
                UPDATED_TEST_PASSWORD.into(),
            )
            .await?;
        assert_eq!(
            replacement_status.remote_id.as_deref(),
            Some(snippet_id.as_str())
        );
        assert_eq!(
            GiteeSnippetRemote::new()?
                .find_sync_vaults(&token)
                .await?
                .len(),
            1
        );

        Ok::<(), SyncServiceError>(())
    }
    .await;

    if let Some(snippet_id) = created_snippet_id {
        let remote = GiteeSnippetRemote::new().unwrap();
        if let Err(error) = remote.delete(&token, &snippet_id).await {
            panic!("live sync validation could not delete its test Gitee snippet: {error}");
        }
    }
    let _ = fs::remove_dir_all(&first_directory);
    let _ = fs::remove_dir_all(&second_directory);
    let _ = fs::remove_dir_all(&replacement_directory);
    result.unwrap();
}
