use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

pub const MAX_MODERATION_REASON_LEN: usize = 1_000;
pub const MUTE_WARNING_POINTS: u32 = 5;
pub const BAN_WARNING_POINTS: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub id: String,
    pub reporter_id: String,
    pub target_id: String,
    pub reason: String,
    pub status: QueueStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalItem {
    pub id: String,
    pub author_id: String,
    pub target_id: String,
    pub reason: String,
    pub status: QueueStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub id: String,
    pub actor_id: String,
    pub target_user_id: String,
    pub target_id: Option<String>,
    pub reason: String,
    pub points: u32,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModerationMacroAction {
    ResolveReport {
        report_id: String,
        resolution: String,
    },
    ResolveApproval {
        approval_id: String,
        resolution: String,
    },
    IssueWarning {
        target_user_id: String,
        target_id: Option<String>,
        reason: String,
        points: u32,
    },
    ExpireWarning {
        warning_id: String,
        reason: String,
    },
    SetShadowban {
        target_user_id: String,
        shadowbanned: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroExecution {
    pub action_count: usize,
    pub audit_event_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserModerationState {
    pub active_warning_points: u32,
    pub muted: bool,
    pub banned: bool,
    pub shadowbanned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditAction {
    ReportCreated,
    ReportResolved,
    ApprovalQueued,
    ApprovalResolved,
    WarningIssued,
    WarningExpired,
    UserShadowbanSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub id: String,
    pub actor_id: String,
    pub action: AuditAction,
    pub target_id: String,
    pub previous_state: Option<UserModerationState>,
    pub new_state: Option<UserModerationState>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModerationError {
    InvalidReason,
    InvalidPoints,
    ReportNotFound,
    ApprovalNotFound,
    WarningNotFound,
    AlreadyResolved,
    WarningInactive,
    EmptyMacro,
    StorePoisoned,
}

#[derive(Debug, Clone)]
pub struct ModerationService {
    inner: Arc<Mutex<ModerationState>>,
}

#[derive(Debug, Clone, Default)]
struct ModerationState {
    next_report_id: u64,
    next_approval_id: u64,
    next_warning_id: u64,
    next_audit_id: u64,
    reports: HashMap<String, Report>,
    approvals: HashMap<String, ApprovalItem>,
    warnings: HashMap<String, Warning>,
    warning_ids_by_user: HashMap<String, Vec<String>>,
    users: HashMap<String, UserModerationState>,
    audit_events: Vec<AuditEvent>,
}

impl ModerationService {
    pub fn new_in_memory() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ModerationState {
                next_report_id: 1,
                next_approval_id: 1,
                next_warning_id: 1,
                next_audit_id: 1,
                ..ModerationState::default()
            })),
        }
    }

    pub fn report(
        &self,
        reporter_id: &str,
        target_id: &str,
        reason: &str,
    ) -> Result<Report, ModerationError> {
        let reason = clean_reason(reason)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        let report = Report {
            id: format!("report:{}", state.next_report_id),
            reporter_id: reporter_id.to_owned(),
            target_id: target_id.to_owned(),
            reason,
            status: QueueStatus::Open,
        };
        state.next_report_id += 1;
        state.reports.insert(report.id.clone(), report.clone());
        push_audit(
            &mut state,
            reporter_id,
            AuditAction::ReportCreated,
            target_id,
            None,
            None,
            "report created",
        );
        Ok(report)
    }

    pub fn queue_approval(
        &self,
        actor_id: &str,
        author_id: &str,
        target_id: &str,
        reason: &str,
    ) -> Result<ApprovalItem, ModerationError> {
        let reason = clean_reason(reason)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        let item = ApprovalItem {
            id: format!("approval:{}", state.next_approval_id),
            author_id: author_id.to_owned(),
            target_id: target_id.to_owned(),
            reason,
            status: QueueStatus::Open,
        };
        state.next_approval_id += 1;
        state.approvals.insert(item.id.clone(), item.clone());
        push_audit(
            &mut state,
            actor_id,
            AuditAction::ApprovalQueued,
            target_id,
            None,
            None,
            "approval queued",
        );
        Ok(item)
    }

    pub fn resolve_report(
        &self,
        actor_id: &str,
        report_id: &str,
        resolution: &str,
    ) -> Result<Report, ModerationError> {
        let resolution = clean_reason(resolution)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        let report = state
            .reports
            .get_mut(report_id)
            .ok_or(ModerationError::ReportNotFound)?;
        if report.status == QueueStatus::Resolved {
            return Err(ModerationError::AlreadyResolved);
        }
        report.status = QueueStatus::Resolved;
        let resolved = report.clone();
        push_audit(
            &mut state,
            actor_id,
            AuditAction::ReportResolved,
            &resolved.target_id,
            None,
            None,
            &format!("report resolved: {resolution}"),
        );
        Ok(resolved)
    }

    pub fn resolve_approval(
        &self,
        actor_id: &str,
        approval_id: &str,
        resolution: &str,
    ) -> Result<ApprovalItem, ModerationError> {
        let resolution = clean_reason(resolution)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        let approval = state
            .approvals
            .get_mut(approval_id)
            .ok_or(ModerationError::ApprovalNotFound)?;
        if approval.status == QueueStatus::Resolved {
            return Err(ModerationError::AlreadyResolved);
        }
        approval.status = QueueStatus::Resolved;
        let resolved = approval.clone();
        push_audit(
            &mut state,
            actor_id,
            AuditAction::ApprovalResolved,
            &resolved.target_id,
            None,
            None,
            &format!("approval resolved: {resolution}"),
        );
        Ok(resolved)
    }

    pub fn issue_warning(
        &self,
        actor_id: &str,
        target_user_id: &str,
        target_id: Option<&str>,
        reason: &str,
        points: u32,
    ) -> Result<Warning, ModerationError> {
        if points == 0 {
            return Err(ModerationError::InvalidPoints);
        }
        let reason = clean_reason(reason)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        let previous_state = state.users.get(target_user_id).cloned().unwrap_or_default();
        let warning = Warning {
            id: format!("warning:{}", state.next_warning_id),
            actor_id: actor_id.to_owned(),
            target_user_id: target_user_id.to_owned(),
            target_id: target_id.map(ToOwned::to_owned),
            reason,
            points,
            active: true,
        };
        state.next_warning_id += 1;
        state
            .warning_ids_by_user
            .entry(target_user_id.to_owned())
            .or_default()
            .push(warning.id.clone());
        state.warnings.insert(warning.id.clone(), warning.clone());
        recompute_user_state(&mut state, target_user_id);
        let new_state = state.users.get(target_user_id).cloned().unwrap_or_default();
        push_audit(
            &mut state,
            actor_id,
            AuditAction::WarningIssued,
            target_user_id,
            Some(previous_state),
            Some(new_state),
            "warning issued",
        );
        Ok(warning)
    }

    pub fn execute_macro(
        &self,
        actor_id: &str,
        actions: &[ModerationMacroAction],
    ) -> Result<MacroExecution, ModerationError> {
        if actions.is_empty() {
            return Err(ModerationError::EmptyMacro);
        }
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        let mut candidate = state.clone();
        let audit_start = candidate.audit_events.len();

        for action in actions {
            match action {
                ModerationMacroAction::ResolveReport {
                    report_id,
                    resolution,
                } => {
                    resolve_report_in_state(&mut candidate, actor_id, report_id, resolution)?;
                }
                ModerationMacroAction::ResolveApproval {
                    approval_id,
                    resolution,
                } => {
                    resolve_approval_in_state(&mut candidate, actor_id, approval_id, resolution)?;
                }
                ModerationMacroAction::IssueWarning {
                    target_user_id,
                    target_id,
                    reason,
                    points,
                } => {
                    issue_warning_in_state(
                        &mut candidate,
                        actor_id,
                        target_user_id,
                        target_id.as_deref(),
                        reason,
                        *points,
                    )?;
                }
                ModerationMacroAction::ExpireWarning { warning_id, reason } => {
                    expire_warning_in_state(&mut candidate, actor_id, warning_id, reason)?;
                }
                ModerationMacroAction::SetShadowban {
                    target_user_id,
                    shadowbanned,
                } => {
                    set_shadowbanned_in_state(
                        &mut candidate,
                        actor_id,
                        target_user_id,
                        *shadowbanned,
                    );
                }
            }
        }

        let audit_event_count = candidate.audit_events.len().saturating_sub(audit_start);
        *state = candidate;
        Ok(MacroExecution {
            action_count: actions.len(),
            audit_event_count,
        })
    }

    pub fn expire_warning(
        &self,
        actor_id: &str,
        warning_id: &str,
        reason: &str,
    ) -> Result<(Warning, UserModerationState), ModerationError> {
        let reason = clean_reason(reason)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        let target_user_id = {
            let warning = state
                .warnings
                .get(warning_id)
                .ok_or(ModerationError::WarningNotFound)?;
            if !warning.active {
                return Err(ModerationError::WarningInactive);
            }
            warning.target_user_id.clone()
        };
        let previous_state = state
            .users
            .get(&target_user_id)
            .cloned()
            .unwrap_or_default();
        let warning = state
            .warnings
            .get_mut(warning_id)
            .ok_or(ModerationError::WarningNotFound)?;
        warning.active = false;
        let expired = warning.clone();
        recompute_user_state(&mut state, &target_user_id);
        let new_state = state
            .users
            .get(&target_user_id)
            .cloned()
            .unwrap_or_default();
        push_audit(
            &mut state,
            actor_id,
            AuditAction::WarningExpired,
            &target_user_id,
            Some(previous_state),
            Some(new_state.clone()),
            &format!("warning expired: {reason}"),
        );
        Ok((expired, new_state))
    }

    pub fn set_shadowbanned(
        &self,
        actor_id: &str,
        target_user_id: &str,
        shadowbanned: bool,
    ) -> Result<UserModerationState, ModerationError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        let previous_state = state.users.get(target_user_id).cloned().unwrap_or_default();
        let next_state = state.users.entry(target_user_id.to_owned()).or_default();
        next_state.shadowbanned = shadowbanned;
        let new_state = next_state.clone();
        push_audit(
            &mut state,
            actor_id,
            AuditAction::UserShadowbanSet,
            target_user_id,
            Some(previous_state),
            Some(new_state.clone()),
            "shadowban changed",
        );
        Ok(new_state)
    }

    pub fn user_state(&self, user_id: &str) -> Result<UserModerationState, ModerationError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        Ok(state.users.get(user_id).cloned().unwrap_or_default())
    }

    pub fn can_view_author_content(
        &self,
        viewer_id: Option<&str>,
        author_id: &str,
    ) -> Result<bool, ModerationError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        let author_state = state.users.get(author_id).cloned().unwrap_or_default();
        Ok(!author_state.shadowbanned || viewer_id == Some(author_id))
    }

    pub fn shadowbanned_user_ids(&self) -> Result<Vec<String>, ModerationError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        let mut user_ids = state
            .users
            .iter()
            .filter(|(_, user)| user.shadowbanned)
            .map(|(user_id, _)| user_id.clone())
            .collect::<Vec<_>>();
        user_ids.sort();
        Ok(user_ids)
    }

    pub fn open_reports(&self) -> Result<Vec<Report>, ModerationError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        let mut reports = state
            .reports
            .values()
            .filter(|report| report.status == QueueStatus::Open)
            .cloned()
            .collect::<Vec<_>>();
        reports.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(reports)
    }

    pub fn open_approvals(&self) -> Result<Vec<ApprovalItem>, ModerationError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        let mut approvals = state
            .approvals
            .values()
            .filter(|approval| approval.status == QueueStatus::Open)
            .cloned()
            .collect::<Vec<_>>();
        approvals.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(approvals)
    }

    pub fn audit_events(&self) -> Result<Vec<AuditEvent>, ModerationError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| ModerationError::StorePoisoned)?;
        Ok(state.audit_events.clone())
    }
}

impl Default for ModerationService {
    fn default() -> Self {
        Self::new_in_memory()
    }
}

impl fmt::Display for ModerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidReason => formatter.write_str("invalid moderation reason"),
            Self::InvalidPoints => formatter.write_str("invalid warning points"),
            Self::ReportNotFound => formatter.write_str("report not found"),
            Self::ApprovalNotFound => formatter.write_str("approval item not found"),
            Self::WarningNotFound => formatter.write_str("warning not found"),
            Self::AlreadyResolved => formatter.write_str("moderation item is already resolved"),
            Self::WarningInactive => formatter.write_str("warning is already inactive"),
            Self::EmptyMacro => formatter.write_str("moderation macro must contain actions"),
            Self::StorePoisoned => formatter.write_str("moderation store lock is poisoned"),
        }
    }
}

impl std::error::Error for ModerationError {}

fn resolve_report_in_state(
    state: &mut ModerationState,
    actor_id: &str,
    report_id: &str,
    resolution: &str,
) -> Result<Report, ModerationError> {
    let resolution = clean_reason(resolution)?;
    let report = state
        .reports
        .get_mut(report_id)
        .ok_or(ModerationError::ReportNotFound)?;
    if report.status == QueueStatus::Resolved {
        return Err(ModerationError::AlreadyResolved);
    }
    report.status = QueueStatus::Resolved;
    let resolved = report.clone();
    push_audit(
        state,
        actor_id,
        AuditAction::ReportResolved,
        &resolved.target_id,
        None,
        None,
        &format!("report resolved: {resolution}"),
    );
    Ok(resolved)
}

fn resolve_approval_in_state(
    state: &mut ModerationState,
    actor_id: &str,
    approval_id: &str,
    resolution: &str,
) -> Result<ApprovalItem, ModerationError> {
    let resolution = clean_reason(resolution)?;
    let approval = state
        .approvals
        .get_mut(approval_id)
        .ok_or(ModerationError::ApprovalNotFound)?;
    if approval.status == QueueStatus::Resolved {
        return Err(ModerationError::AlreadyResolved);
    }
    approval.status = QueueStatus::Resolved;
    let resolved = approval.clone();
    push_audit(
        state,
        actor_id,
        AuditAction::ApprovalResolved,
        &resolved.target_id,
        None,
        None,
        &format!("approval resolved: {resolution}"),
    );
    Ok(resolved)
}

fn issue_warning_in_state(
    state: &mut ModerationState,
    actor_id: &str,
    target_user_id: &str,
    target_id: Option<&str>,
    reason: &str,
    points: u32,
) -> Result<Warning, ModerationError> {
    if points == 0 {
        return Err(ModerationError::InvalidPoints);
    }
    let reason = clean_reason(reason)?;
    let previous_state = state.users.get(target_user_id).cloned().unwrap_or_default();
    let warning = Warning {
        id: format!("warning:{}", state.next_warning_id),
        actor_id: actor_id.to_owned(),
        target_user_id: target_user_id.to_owned(),
        target_id: target_id.map(ToOwned::to_owned),
        reason,
        points,
        active: true,
    };
    state.next_warning_id += 1;
    state
        .warning_ids_by_user
        .entry(target_user_id.to_owned())
        .or_default()
        .push(warning.id.clone());
    state.warnings.insert(warning.id.clone(), warning.clone());
    recompute_user_state(state, target_user_id);
    let new_state = state.users.get(target_user_id).cloned().unwrap_or_default();
    push_audit(
        state,
        actor_id,
        AuditAction::WarningIssued,
        target_user_id,
        Some(previous_state),
        Some(new_state),
        "warning issued",
    );
    Ok(warning)
}

fn expire_warning_in_state(
    state: &mut ModerationState,
    actor_id: &str,
    warning_id: &str,
    reason: &str,
) -> Result<(Warning, UserModerationState), ModerationError> {
    let reason = clean_reason(reason)?;
    let target_user_id = {
        let warning = state
            .warnings
            .get(warning_id)
            .ok_or(ModerationError::WarningNotFound)?;
        if !warning.active {
            return Err(ModerationError::WarningInactive);
        }
        warning.target_user_id.clone()
    };
    let previous_state = state
        .users
        .get(&target_user_id)
        .cloned()
        .unwrap_or_default();
    let warning = state
        .warnings
        .get_mut(warning_id)
        .ok_or(ModerationError::WarningNotFound)?;
    warning.active = false;
    let expired = warning.clone();
    recompute_user_state(state, &target_user_id);
    let new_state = state
        .users
        .get(&target_user_id)
        .cloned()
        .unwrap_or_default();
    push_audit(
        state,
        actor_id,
        AuditAction::WarningExpired,
        &target_user_id,
        Some(previous_state),
        Some(new_state.clone()),
        &format!("warning expired: {reason}"),
    );
    Ok((expired, new_state))
}

fn set_shadowbanned_in_state(
    state: &mut ModerationState,
    actor_id: &str,
    target_user_id: &str,
    shadowbanned: bool,
) -> UserModerationState {
    let previous_state = state.users.get(target_user_id).cloned().unwrap_or_default();
    let next_state = state.users.entry(target_user_id.to_owned()).or_default();
    next_state.shadowbanned = shadowbanned;
    let new_state = next_state.clone();
    push_audit(
        state,
        actor_id,
        AuditAction::UserShadowbanSet,
        target_user_id,
        Some(previous_state),
        Some(new_state.clone()),
        "shadowban changed",
    );
    new_state
}

fn clean_reason(value: &str) -> Result<String, ModerationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_MODERATION_REASON_LEN || trimmed.contains('\0') {
        Err(ModerationError::InvalidReason)
    } else {
        Ok(trimmed.to_owned())
    }
}

fn recompute_user_state(state: &mut ModerationState, user_id: &str) {
    let shadowbanned = state
        .users
        .get(user_id)
        .is_some_and(|user| user.shadowbanned);
    let active_warning_points = state
        .warning_ids_by_user
        .get(user_id)
        .into_iter()
        .flatten()
        .filter_map(|warning_id| state.warnings.get(warning_id))
        .filter(|warning| warning.active)
        .map(|warning| warning.points)
        .sum();

    state.users.insert(
        user_id.to_owned(),
        UserModerationState {
            active_warning_points,
            muted: active_warning_points >= MUTE_WARNING_POINTS,
            banned: active_warning_points >= BAN_WARNING_POINTS,
            shadowbanned,
        },
    );
}

fn push_audit(
    state: &mut ModerationState,
    actor_id: &str,
    action: AuditAction,
    target_id: &str,
    previous_state: Option<UserModerationState>,
    new_state: Option<UserModerationState>,
    detail: &str,
) {
    let event = AuditEvent {
        id: format!("audit:{}", state.next_audit_id),
        actor_id: actor_id.to_owned(),
        action,
        target_id: target_id.to_owned(),
        previous_state,
        new_state,
        detail: detail.to_owned(),
    };
    state.next_audit_id += 1;
    state.audit_events.push(event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_and_approval_queues_keep_open_items_ordered() {
        let moderation = ModerationService::new_in_memory();

        let second_report = moderation.report("user:2", "post:2", "spam link").unwrap();
        let first_report = moderation
            .report("user:1", "post:1", "personal data")
            .unwrap();
        let approval = moderation
            .queue_approval("system:filters", "user:3", "post:3", "low trust link")
            .unwrap();

        assert_eq!(second_report.id, "report:1");
        assert_eq!(first_report.id, "report:2");
        assert_eq!(approval.id, "approval:1");
        assert_eq!(
            moderation
                .open_reports()
                .unwrap()
                .into_iter()
                .map(|report| report.id)
                .collect::<Vec<_>>(),
            vec!["report:1", "report:2"]
        );
        assert_eq!(moderation.open_approvals().unwrap()[0].id, "approval:1");
    }

    #[test]
    fn resolving_queue_items_closes_them_and_writes_audit() {
        let moderation = ModerationService::new_in_memory();
        let report = moderation.report("user:1", "post:1", "spam").unwrap();
        let approval = moderation
            .queue_approval("system:filters", "user:2", "post:2", "low trust link")
            .unwrap();

        let resolved_report = moderation
            .resolve_report("user:mod", &report.id, "deleted duplicate")
            .unwrap();
        let resolved_approval = moderation
            .resolve_approval("user:mod", &approval.id, "approved")
            .unwrap();

        assert_eq!(resolved_report.status, QueueStatus::Resolved);
        assert_eq!(resolved_approval.status, QueueStatus::Resolved);
        assert!(moderation.open_reports().unwrap().is_empty());
        assert!(moderation.open_approvals().unwrap().is_empty());

        let audit = moderation.audit_events().unwrap();
        assert_eq!(audit.len(), 4);
        assert_eq!(audit[2].action, AuditAction::ReportResolved);
        assert_eq!(audit[2].target_id, "post:1");
        assert_eq!(audit[3].action, AuditAction::ApprovalResolved);
        assert_eq!(audit[3].target_id, "post:2");
    }

    #[test]
    fn resolved_queue_items_cannot_be_resolved_again() {
        let moderation = ModerationService::new_in_memory();
        let report = moderation.report("user:1", "post:1", "spam").unwrap();

        moderation
            .resolve_report("user:mod", &report.id, "handled")
            .unwrap();

        assert!(matches!(
            moderation.resolve_report("user:mod", &report.id, "handled again"),
            Err(ModerationError::AlreadyResolved)
        ));
        assert!(matches!(
            moderation.resolve_approval("user:mod", "approval:missing", "handled"),
            Err(ModerationError::ApprovalNotFound)
        ));
    }

    #[test]
    fn shadowbanned_users_see_their_own_content_but_others_do_not() {
        let moderation = ModerationService::new_in_memory();
        moderation
            .set_shadowbanned("user:mod", "user:spammer", true)
            .unwrap();

        assert!(
            moderation
                .can_view_author_content(Some("user:spammer"), "user:spammer")
                .unwrap()
        );
        assert!(
            !moderation
                .can_view_author_content(Some("user:member"), "user:spammer")
                .unwrap()
        );
        assert!(
            !moderation
                .can_view_author_content(None, "user:spammer")
                .unwrap()
        );
    }

    #[test]
    fn audit_events_capture_previous_and_new_user_state() {
        let moderation = ModerationService::new_in_memory();
        moderation
            .issue_warning("user:mod", "user:target", Some("post:1"), "spam", 3)
            .unwrap();
        moderation
            .set_shadowbanned("user:mod", "user:target", true)
            .unwrap();

        let audit = moderation.audit_events().unwrap();
        assert_eq!(audit.len(), 2);
        assert_eq!(audit[0].action, AuditAction::WarningIssued);
        assert_eq!(
            audit[0].previous_state,
            Some(UserModerationState::default())
        );
        assert_eq!(
            audit[0].new_state,
            Some(UserModerationState {
                active_warning_points: 3,
                muted: false,
                banned: false,
                shadowbanned: false,
            })
        );
        assert_eq!(
            audit[1].previous_state,
            Some(UserModerationState {
                active_warning_points: 3,
                muted: false,
                banned: false,
                shadowbanned: false,
            })
        );
        assert_eq!(
            audit[1].new_state,
            Some(UserModerationState {
                active_warning_points: 3,
                muted: false,
                banned: false,
                shadowbanned: true,
            })
        );
    }

    #[test]
    fn warning_thresholds_trigger_automatic_mutes_and_bans() {
        let moderation = ModerationService::new_in_memory();

        moderation
            .issue_warning("user:mod", "user:target", None, "minor issue", 4)
            .unwrap();
        assert_eq!(
            moderation.user_state("user:target").unwrap(),
            UserModerationState {
                active_warning_points: 4,
                muted: false,
                banned: false,
                shadowbanned: false,
            }
        );

        moderation
            .issue_warning("user:mod", "user:target", None, "spam burst", 1)
            .unwrap();
        assert_eq!(
            moderation.user_state("user:target").unwrap(),
            UserModerationState {
                active_warning_points: MUTE_WARNING_POINTS,
                muted: true,
                banned: false,
                shadowbanned: false,
            }
        );

        moderation
            .issue_warning("user:mod", "user:target", None, "continued abuse", 5)
            .unwrap();
        assert_eq!(
            moderation.user_state("user:target").unwrap(),
            UserModerationState {
                active_warning_points: BAN_WARNING_POINTS,
                muted: true,
                banned: true,
                shadowbanned: false,
            }
        );
    }

    #[test]
    fn expiring_warnings_recomputes_user_state_and_writes_audit() {
        let moderation = ModerationService::new_in_memory();
        let warning = moderation
            .issue_warning("user:mod", "user:target", None, "major abuse", 10)
            .unwrap();
        assert!(moderation.user_state("user:target").unwrap().banned);

        let (expired, state) = moderation
            .expire_warning("user:mod", &warning.id, "points decayed")
            .unwrap();

        assert!(!expired.active);
        assert_eq!(state.active_warning_points, 0);
        assert!(!state.muted);
        assert!(!state.banned);

        let audit = moderation.audit_events().unwrap();
        assert_eq!(audit.len(), 2);
        assert_eq!(audit[1].action, AuditAction::WarningExpired);
        assert_eq!(
            audit[1].previous_state,
            Some(UserModerationState {
                active_warning_points: 10,
                muted: true,
                banned: true,
                shadowbanned: false,
            })
        );
        assert_eq!(audit[1].new_state, Some(UserModerationState::default()));
    }

    #[test]
    fn inactive_warnings_cannot_be_expired_again() {
        let moderation = ModerationService::new_in_memory();
        let warning = moderation
            .issue_warning("user:mod", "user:target", None, "spam", 1)
            .unwrap();
        moderation
            .expire_warning("user:mod", &warning.id, "resolved")
            .unwrap();

        assert!(matches!(
            moderation.expire_warning("user:mod", &warning.id, "again"),
            Err(ModerationError::WarningInactive)
        ));
        assert!(matches!(
            moderation.expire_warning("user:mod", "warning:missing", "missing"),
            Err(ModerationError::WarningNotFound)
        ));
    }

    #[test]
    fn moderation_macro_applies_actions_transactionally() {
        let moderation = ModerationService::new_in_memory();
        let report = moderation.report("user:1", "post:1", "spam").unwrap();
        let approval = moderation
            .queue_approval("system:filters", "user:2", "post:2", "low trust link")
            .unwrap();

        let execution = moderation
            .execute_macro(
                "user:mod",
                &[
                    ModerationMacroAction::ResolveReport {
                        report_id: report.id,
                        resolution: "deleted duplicate".to_owned(),
                    },
                    ModerationMacroAction::ResolveApproval {
                        approval_id: approval.id,
                        resolution: "approved".to_owned(),
                    },
                    ModerationMacroAction::IssueWarning {
                        target_user_id: "user:2".to_owned(),
                        target_id: Some("post:2".to_owned()),
                        reason: "posted spam".to_owned(),
                        points: 5,
                    },
                    ModerationMacroAction::SetShadowban {
                        target_user_id: "user:2".to_owned(),
                        shadowbanned: true,
                    },
                ],
            )
            .unwrap();

        assert_eq!(
            execution,
            MacroExecution {
                action_count: 4,
                audit_event_count: 4,
            }
        );
        assert!(moderation.open_reports().unwrap().is_empty());
        assert!(moderation.open_approvals().unwrap().is_empty());
        assert_eq!(
            moderation.user_state("user:2").unwrap(),
            UserModerationState {
                active_warning_points: 5,
                muted: true,
                banned: false,
                shadowbanned: true,
            }
        );
        assert_eq!(moderation.audit_events().unwrap().len(), 6);
    }

    #[test]
    fn moderation_macro_rolls_back_all_actions_on_failure() {
        let moderation = ModerationService::new_in_memory();
        let report = moderation.report("user:1", "post:1", "spam").unwrap();

        let err = moderation
            .execute_macro(
                "user:mod",
                &[
                    ModerationMacroAction::ResolveReport {
                        report_id: report.id,
                        resolution: "would resolve".to_owned(),
                    },
                    ModerationMacroAction::IssueWarning {
                        target_user_id: "user:2".to_owned(),
                        target_id: None,
                        reason: "invalid zero-point warning".to_owned(),
                        points: 0,
                    },
                ],
            )
            .unwrap_err();

        assert_eq!(err, ModerationError::InvalidPoints);
        assert_eq!(moderation.open_reports().unwrap().len(), 1);
        assert_eq!(
            moderation.user_state("user:2").unwrap(),
            UserModerationState::default()
        );
        assert_eq!(moderation.audit_events().unwrap().len(), 1);
    }

    #[test]
    fn moderation_macro_rejects_empty_action_lists() {
        let moderation = ModerationService::new_in_memory();

        assert_eq!(
            moderation.execute_macro("user:mod", &[]).unwrap_err(),
            ModerationError::EmptyMacro
        );
    }

    #[test]
    fn rejects_invalid_moderation_inputs() {
        let moderation = ModerationService::new_in_memory();

        assert!(matches!(
            moderation.report("user:1", "post:1", "   "),
            Err(ModerationError::InvalidReason)
        ));
        assert!(matches!(
            moderation.issue_warning("user:mod", "user:target", None, "reason", 0),
            Err(ModerationError::InvalidPoints)
        ));
    }
}
