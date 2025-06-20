// Here we will write the business logic related to the scheduled task in the future 

use crate::model::task;

// Add a scheduled task
pub async fn add_task(user_id: i32, stock_id: i32, action: &str, schedule_time: chrono::DateTime<chrono::Utc>) -> anyhow::Result<task::Model> {
    // TODO: Implement the addition logic
    todo!()
}

// Query all scheduled tasks
pub async fn get_all_tasks() -> anyhow::Result<Vec<task::Model>> {
    // TODO: Implement the query logic
    todo!()
}

// Delete a scheduled task
pub async fn delete_task(id: i32) -> anyhow::Result<()> {
    // TODO: Implement the deletion logic
    todo!()
} 