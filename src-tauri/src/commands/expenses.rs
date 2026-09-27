use super::{blocking, Service};
use crate::domain::expense::{Expense, ExpenseEdit};
use crate::error::Result;

#[tauri::command]
pub(super) async fn list_expenses(s: Service<'_>) -> Result<Vec<Expense>> {
    blocking(s, |s| s.expenses()).await
}
#[tauri::command]
pub(super) async fn get_expense(s: Service<'_>, id: String) -> Result<Expense> {
    blocking(s, move |s| s.expense(&id)).await
}
#[tauri::command]
pub(super) async fn create_expense(s: Service<'_>) -> Result<Expense> {
    blocking(s, |s| s.create_expense()).await
}
#[tauri::command]
pub(super) async fn edit_expense(s: Service<'_>, edit: ExpenseEdit) -> Result<Expense> {
    blocking(s, move |s| s.edit_expense(edit)).await
}
#[tauri::command]
pub(super) async fn queue_extraction(s: Service<'_>, id: String) -> Result<()> {
    blocking(s, move |s| s.queue_extraction(&id)).await
}
