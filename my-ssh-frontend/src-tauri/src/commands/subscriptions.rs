use tauri::State;

use crate::state::AppState;
use crate::subscriptions::{
    AddSubscriptionRequest, RefreshSubscriptionResult, Subscription, SubscriptionScript,
    SubscriptionStore, UpdateSubscriptionRequest,
};

#[tauri::command]
pub async fn list_script_subscriptions(
    state: State<'_, AppState>,
) -> Result<Vec<Subscription>, String> {
    SubscriptionStore::new(&state.app_dir)
        .list()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn add_script_subscription(
    state: State<'_, AppState>,
    subscription: AddSubscriptionRequest,
) -> Result<Subscription, String> {
    SubscriptionStore::new(&state.app_dir)
        .add(subscription)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_script_subscription(
    state: State<'_, AppState>,
    id: String,
    subscription: UpdateSubscriptionRequest,
) -> Result<Subscription, String> {
    SubscriptionStore::new(&state.app_dir)
        .update(&id, subscription)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_script_subscription(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    SubscriptionStore::new(&state.app_dir)
        .remove(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn refresh_script_subscription(
    state: State<'_, AppState>,
    id: String,
) -> Result<RefreshSubscriptionResult, String> {
    SubscriptionStore::new(&state.app_dir)
        .refresh(&id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_cached_subscription_scripts(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<SubscriptionScript>, String> {
    SubscriptionStore::new(&state.app_dir)
        .cached_scripts(&id)
        .map(|(scripts, _warnings)| scripts)
        .map_err(|error| error.to_string())
}
