use crate::config::Config;
use crate::scheduler::task_scheduler::{Task, TaskExecutor};
use crate::utils::mail::send_mail_with_config;
use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

// Mail task executor, supports asynchronous sending of mails.
pub struct MailTaskExecutor {
    pub config: Config,
}

#[async_trait]
impl TaskExecutor for MailTaskExecutor {
    async fn execute(
        &self,
        task: &Task,
        _cancellation: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let data = task
            .data
            .as_ref()
            .ok_or("Mail task is missing data field")?;
        let to = data
            .get("to")
            .and_then(Value::as_str)
            .ok_or("Missing to field")?;
        let subject = data
            .get("subject")
            .and_then(Value::as_str)
            .unwrap_or("(No theme)");
        let body = data.get("body").and_then(Value::as_str).unwrap_or("");
        send_mail_with_config(self.config.clone(), to, subject, body).await?;
        Ok(())
    }
    fn get_name(&self) -> &str {
        "mail_sender"
    }
}
