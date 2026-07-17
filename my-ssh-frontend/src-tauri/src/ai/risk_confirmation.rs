use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

const RISK_CONFIRMATION_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct RiskConfirmation {
    pub id: String,
    pub session_id: String,
    pub command: String,
    pub reason: String,
    created_at: Instant,
}

/// In-memory, one-time confirmations for commands that can make SSH unavailable.
///
/// A confirmation is valid only for its exact normalized command and SSH session.
/// It is intentionally independent from AI task lifecycle so a user can confirm in
/// a subsequent task created by their next chat message.
#[derive(Clone, Default)]
pub struct RiskConfirmationStore {
    confirmations: Arc<Mutex<HashMap<String, RiskConfirmation>>>,
}

impl RiskConfirmationStore {
    pub async fn create(
        &self,
        session_id: String,
        command: String,
        reason: String,
    ) -> RiskConfirmation {
        let confirmation = RiskConfirmation {
            id: uuid::Uuid::new_v4().to_string(),
            session_id,
            command,
            reason,
            created_at: Instant::now(),
        };
        let mut confirmations = self.confirmations.lock().await;
        confirmations.retain(|_, item| item.created_at.elapsed() <= RISK_CONFIRMATION_TTL);
        confirmations.insert(confirmation.id.clone(), confirmation.clone());
        confirmation
    }

    /// Consumes a confirmation only when its command, session, and identifier match.
    pub async fn consume(
        &self,
        confirmation_id: &str,
        session_id: &str,
        command: &str,
    ) -> Option<RiskConfirmation> {
        let mut confirmations = self.confirmations.lock().await;
        let confirmation = confirmations.get(confirmation_id)?;
        if confirmation.created_at.elapsed() > RISK_CONFIRMATION_TTL {
            confirmations.remove(confirmation_id);
            return None;
        }
        if confirmation.session_id != session_id || confirmation.command != command {
            return None;
        }
        confirmations.remove(confirmation_id)
    }

    /// Consumes the command stored for this confirmation without accepting a command
    /// from the UI. The session binding prevents a token from being used elsewhere.
    pub async fn consume_for_session(
        &self,
        confirmation_id: &str,
        session_id: &str,
    ) -> Option<RiskConfirmation> {
        let mut confirmations = self.confirmations.lock().await;
        let confirmation = confirmations.get(confirmation_id)?;
        if confirmation.created_at.elapsed() > RISK_CONFIRMATION_TTL {
            confirmations.remove(confirmation_id);
            return None;
        }
        if confirmation.session_id != session_id {
            return None;
        }
        confirmations.remove(confirmation_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn confirmation_is_bound_to_exact_command_and_session_and_is_one_time() {
        let store = RiskConfirmationStore::default();
        let confirmation = store
            .create(
                "session-a".into(),
                "systemctl restart ssh".into(),
                "may interrupt SSH".into(),
            )
            .await;

        assert!(store
            .consume(&confirmation.id, "session-b", "systemctl restart ssh")
            .await
            .is_none());
        assert!(store
            .consume(&confirmation.id, "session-a", "systemctl restart sshd")
            .await
            .is_none());
        assert!(store
            .consume(&confirmation.id, "session-a", "systemctl restart ssh")
            .await
            .is_some());
        assert!(store
            .consume(&confirmation.id, "session-a", "systemctl restart ssh")
            .await
            .is_none());
    }

    #[tokio::test]
    async fn direct_confirmation_is_session_bound_and_one_time() {
        let store = RiskConfirmationStore::default();
        let confirmation = store
            .create(
                "session-a".into(),
                "systemctl restart ssh".into(),
                "may interrupt SSH".into(),
            )
            .await;

        assert!(store
            .consume_for_session(&confirmation.id, "session-b")
            .await
            .is_none());
        assert!(store
            .consume_for_session(&confirmation.id, "session-a")
            .await
            .is_some());
        assert!(store
            .consume_for_session(&confirmation.id, "session-a")
            .await
            .is_none());
    }
}
