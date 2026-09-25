mod cleanup;
mod common;
mod sessions;
mod step_up;
mod tokens;
mod totp;
mod types;
mod users;

pub(crate) use cleanup::{cleanup_auth_rows, run_periodic_auth_cleanup};
pub(crate) use common::{constant_time_eq, hash_secret, normalize_email};
pub(crate) use sessions::{
    authenticated_session, cleared_session_json, get_session, is_demo_session, logout, logout_all,
    refresh, skip_two_factor_onboarding,
};
pub(crate) use step_up::{require_recent_step_up, start_step_up, verify_step_up};
pub(crate) use totp::{
    disable_two_factor, enable_two_factor, setup_two_factor, verify_tfa, verify_two_factor,
};

pub(crate) fn reject_demo_workspace_mutation(user_id: &str) -> Result<(), crate::ApiError> {
    if user_id == crate::demo::DEMO_USER_ID {
        Err(crate::ApiError::Forbidden(
            "This workspace is read-only".to_owned(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
pub(crate) async fn create_test_session_cookie(state: &crate::AppState, user_id: &str) -> String {
    sessions::create_session(state, user_id)
        .await
        .expect("test session should be created")
        .access_cookie
        .split(';')
        .next()
        .expect("test access cookie should be valid")
        .to_owned()
}

mod password;
pub use password::{bootstrap, recover_admin};
pub(crate) use password::{
    change_password, create_user, list_users, login, require_admin, update_user,
};
