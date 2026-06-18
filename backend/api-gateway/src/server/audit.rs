use axum::http::StatusCode;

use crate::auth::AdminContext;

use super::admin_route_policy_for_path;
use super::middleware::RequestId;

pub(super) fn action_for_path(path: &str) -> &'static str {
    admin_route_policy_for_path(path)
        .map(|policy| policy.action)
        .unwrap_or("admin.request")
}

pub(super) fn emit_admin_audit(
    request_id: &RequestId,
    admin: &AdminContext,
    action: &'static str,
    target_id: Option<&str>,
    outcome: &'static str,
    status_code: StatusCode,
) {
    tracing::info!(
        event_type = "admin_audit",
        request_id = %request_id.0,
        actor_subject = admin.subject,
        actor_roles = admin.roles.join(","),
        action,
        target_id = target_id.unwrap_or(""),
        outcome,
        status_code = status_code.as_u16(),
        "admin audit event"
    );
}
