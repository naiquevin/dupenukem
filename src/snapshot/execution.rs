use super::Action;
use crate::error::AppError;
use log::info;

pub fn execute(actions: Vec<Action>) -> Result<(), AppError> {
    for action in actions {
        let dry_run = true;
        if let Some(msg) = action.log(&dry_run) {
            info!("{}", msg);
        }
    }
    Ok(())
}
