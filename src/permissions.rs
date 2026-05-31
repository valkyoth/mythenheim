use std::{
    collections::{HashMap, HashSet},
    fmt,
    str::FromStr,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Capability(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityParseError;

impl Capability {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parse_static(value: &'static str) -> Self {
        value
            .parse()
            .expect("static capability strings must be valid")
    }
}

impl FromStr for Capability {
    type Err = CapabilityParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if is_valid_capability(value) {
            Ok(Self(value.to_owned()))
        } else {
            Err(CapabilityParseError)
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Display for CapabilityParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid capability string")
    }
}

impl std::error::Error for CapabilityParseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustLevel {
    NewSeed = 0,
    Wanderer = 1,
    Citizen = 2,
    Elder = 3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustMetrics {
    pub topics_read: u32,
    pub minutes_read: u32,
    pub posts_created: u32,
    pub days_visited: u32,
    pub helpful_flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedRole {
    pub category_id: String,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorPermissions {
    pub actor_id: String,
    pub trust_level: TrustLevel,
    pub global_roles: Vec<Role>,
    pub category_roles: Vec<ScopedRole>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionContext {
    pub actor_id: String,
    pub owner_id: Option<String>,
    pub category_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionError {
    InvalidCapability(CapabilityParseError),
    RoleNotFound,
    EscalatingRoleAssignment { missing: Vec<Capability> },
    StorePoisoned,
}

impl TrustLevel {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::NewSeed,
            1 => Self::Wanderer,
            2 => Self::Citizen,
            _ => Self::Elder,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TrustMetrics {
    pub fn trust_level(&self) -> TrustLevel {
        if self.topics_read >= 200
            && self.minutes_read >= 1_200
            && self.posts_created >= 100
            && self.days_visited >= 50
            && self.helpful_flags >= 20
        {
            TrustLevel::Elder
        } else if self.topics_read >= 50
            && self.minutes_read >= 240
            && self.posts_created >= 15
            && self.days_visited >= 10
        {
            TrustLevel::Citizen
        } else if self.topics_read >= 5 && self.minutes_read >= 10 {
            TrustLevel::Wanderer
        } else {
            TrustLevel::NewSeed
        }
    }
}

impl Role {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        capabilities: impl IntoIterator<Item = Capability>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            capabilities: capabilities.into_iter().collect(),
        }
    }
}

impl ActorPermissions {
    pub fn effective_capabilities(&self, category_id: Option<&str>) -> HashSet<Capability> {
        let mut capabilities = trust_capabilities(self.trust_level);
        for role in &self.global_roles {
            capabilities.extend(role.capabilities.iter().cloned());
        }
        if let Some(category_id) = category_id {
            for scoped in &self.category_roles {
                if scoped.category_id == category_id {
                    capabilities.extend(scoped.role.capabilities.iter().cloned());
                }
            }
        }
        capabilities
    }

    pub fn allows(&self, required: &Capability, context: &PermissionContext) -> bool {
        if context.actor_id != self.actor_id {
            return false;
        }
        let capabilities = self.effective_capabilities(context.category_id.as_deref());
        if capabilities.contains(required) {
            if required.as_str().ends_with(".own") {
                return context.owner_id.as_deref() == Some(self.actor_id.as_str());
            }
            return true;
        }

        if let Some(any_capability) = own_to_any(required)
            && capabilities.contains(&any_capability)
        {
            return true;
        }

        false
    }

    pub fn can_assign_role(&self, role: &Role) -> Result<(), PermissionError> {
        let capabilities = self.effective_capabilities(None);
        let mut missing = role
            .capabilities
            .iter()
            .filter(|capability| !capabilities.contains(*capability))
            .cloned()
            .collect::<Vec<_>>();
        missing.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        if missing.is_empty() {
            Ok(())
        } else {
            Err(PermissionError::EscalatingRoleAssignment { missing })
        }
    }
}

#[derive(Debug, Clone)]
pub struct PermissionService {
    inner: Arc<Mutex<PermissionState>>,
}

#[derive(Debug, Default)]
struct PermissionState {
    roles: HashMap<String, Role>,
    default_role_ids: Vec<String>,
    global_assignments: HashMap<String, HashSet<String>>,
    category_assignments: HashMap<(String, String), HashSet<String>>,
}

impl PermissionService {
    pub fn new_in_memory(default_roles: impl IntoIterator<Item = Role>) -> Self {
        let mut state = PermissionState::default();
        for role in default_roles {
            state.default_role_ids.push(role.id.clone());
            state.roles.insert(role.id.clone(), role);
        }
        Self {
            inner: Arc::new(Mutex::new(state)),
        }
    }

    pub fn define_role(&self, role: Role) -> Result<(), PermissionError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| PermissionError::StorePoisoned)?;
        state.roles.insert(role.id.clone(), role);
        Ok(())
    }

    pub fn actor_permissions(
        &self,
        user_id: &str,
        trust_level: TrustLevel,
    ) -> Result<ActorPermissions, PermissionError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| PermissionError::StorePoisoned)?;
        Ok(actor_permissions_from_state(&state, user_id, trust_level))
    }

    pub fn assign_global_role(
        &self,
        actor_id: &str,
        actor_trust_level: TrustLevel,
        target_user_id: &str,
        role_id: &str,
    ) -> Result<(), PermissionError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| PermissionError::StorePoisoned)?;
        let role = state
            .roles
            .get(role_id)
            .cloned()
            .ok_or(PermissionError::RoleNotFound)?;
        let actor = actor_permissions_from_state(&state, actor_id, actor_trust_level);
        actor.can_assign_role(&role)?;
        state
            .global_assignments
            .entry(target_user_id.to_owned())
            .or_default()
            .insert(role_id.to_owned());
        Ok(())
    }

    pub fn grant_global_role_for_bootstrap(
        &self,
        target_user_id: &str,
        role_id: &str,
    ) -> Result<(), PermissionError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| PermissionError::StorePoisoned)?;
        if !state.roles.contains_key(role_id) {
            return Err(PermissionError::RoleNotFound);
        }
        state
            .global_assignments
            .entry(target_user_id.to_owned())
            .or_default()
            .insert(role_id.to_owned());
        Ok(())
    }

    pub fn assign_category_role(
        &self,
        actor_id: &str,
        actor_trust_level: TrustLevel,
        target_user_id: &str,
        category_id: &str,
        role_id: &str,
    ) -> Result<(), PermissionError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| PermissionError::StorePoisoned)?;
        let role = state
            .roles
            .get(role_id)
            .cloned()
            .ok_or(PermissionError::RoleNotFound)?;
        let actor = actor_permissions_from_state(&state, actor_id, actor_trust_level);
        actor.can_assign_role(&role)?;
        state
            .category_assignments
            .entry((target_user_id.to_owned(), category_id.to_owned()))
            .or_default()
            .insert(role_id.to_owned());
        Ok(())
    }

    pub fn grant_category_role_for_bootstrap(
        &self,
        target_user_id: &str,
        category_id: &str,
        role_id: &str,
    ) -> Result<(), PermissionError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| PermissionError::StorePoisoned)?;
        if !state.roles.contains_key(role_id) {
            return Err(PermissionError::RoleNotFound);
        }
        state
            .category_assignments
            .entry((target_user_id.to_owned(), category_id.to_owned()))
            .or_default()
            .insert(role_id.to_owned());
        Ok(())
    }
}

impl Default for PermissionService {
    fn default() -> Self {
        Self::new_in_memory([])
    }
}

impl fmt::Display for PermissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCapability(err) => write!(formatter, "{err}"),
            Self::RoleNotFound => formatter.write_str("role not found"),
            Self::EscalatingRoleAssignment { missing } => {
                write!(
                    formatter,
                    "actor lacks {} role capability(s)",
                    missing.len()
                )
            }
            Self::StorePoisoned => formatter.write_str("permission store lock is poisoned"),
        }
    }
}

impl std::error::Error for PermissionError {}

fn is_valid_capability(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(first) = parts.next() else {
        return false;
    };

    if !is_valid_segment(first) {
        return false;
    }

    let mut segment_count = 1;
    for part in parts {
        segment_count += 1;
        if !is_valid_segment(part) {
            return false;
        }
    }

    segment_count >= 2
}

fn own_to_any(capability: &Capability) -> Option<Capability> {
    let value = capability.as_str();
    let prefix = value.strip_suffix(".own")?;
    Some(Capability::parse_static(match prefix {
        "post.edit" => "post.edit.any",
        "post.delete" => "post.delete.any",
        "topic.edit" => "topic.edit.any",
        "topic.delete" => "topic.delete.any",
        _ => return None,
    }))
}

fn trust_capabilities(trust_level: TrustLevel) -> HashSet<Capability> {
    let mut capabilities = HashSet::new();
    capabilities.insert(Capability::parse_static("category.read.public"));

    if trust_level >= TrustLevel::Wanderer {
        capabilities.insert(Capability::parse_static("topic.create"));
        capabilities.insert(Capability::parse_static("post.reply"));
    }
    if trust_level >= TrustLevel::Citizen {
        capabilities.insert(Capability::parse_static("post.edit.own"));
        capabilities.insert(Capability::parse_static("post.delete.own"));
        capabilities.insert(Capability::parse_static("reaction.create"));
    }
    if trust_level >= TrustLevel::Elder {
        capabilities.insert(Capability::parse_static("flag.trusted"));
        capabilities.insert(Capability::parse_static("category.read.veteran"));
    }

    capabilities
}

fn actor_permissions_from_state(
    state: &PermissionState,
    user_id: &str,
    trust_level: TrustLevel,
) -> ActorPermissions {
    let mut seen_global = HashSet::new();
    let mut global_roles = Vec::new();

    for role_id in &state.default_role_ids {
        if seen_global.insert(role_id.clone())
            && let Some(role) = state.roles.get(role_id)
        {
            global_roles.push(role.clone());
        }
    }
    if let Some(role_ids) = state.global_assignments.get(user_id) {
        let mut sorted_role_ids = role_ids.iter().collect::<Vec<_>>();
        sorted_role_ids.sort();
        for role_id in sorted_role_ids {
            if seen_global.insert(role_id.clone())
                && let Some(role) = state.roles.get(role_id)
            {
                global_roles.push(role.clone());
            }
        }
    }

    let mut category_roles = Vec::new();
    let mut scoped_keys = state
        .category_assignments
        .iter()
        .filter(|((assigned_user_id, _), _)| assigned_user_id == user_id)
        .collect::<Vec<_>>();
    scoped_keys.sort_by(|((_, left_category), _), ((_, right_category), _)| {
        left_category.cmp(right_category)
    });
    for ((_, category_id), role_ids) in scoped_keys {
        let mut sorted_role_ids = role_ids.iter().collect::<Vec<_>>();
        sorted_role_ids.sort();
        for role_id in sorted_role_ids {
            if let Some(role) = state.roles.get(role_id) {
                category_roles.push(ScopedRole {
                    category_id: category_id.clone(),
                    role: role.clone(),
                });
            }
        }
    }

    ActorPermissions {
        actor_id: user_id.to_owned(),
        trust_level,
        global_roles,
        category_roles,
    }
}

fn is_valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_granular_capability() {
        let capability: Capability = "post.delete.own".parse().unwrap();

        assert_eq!(capability.as_str(), "post.delete.own");
    }

    #[test]
    fn rejects_wildcards_and_uppercase() {
        assert!("post.*".parse::<Capability>().is_err());
        assert!("System.Settings.Write".parse::<Capability>().is_err());
    }

    #[test]
    fn rejects_single_segment() {
        assert!("admin".parse::<Capability>().is_err());
    }

    #[test]
    fn resolves_trust_levels_deterministically() {
        assert_eq!(
            TrustMetrics {
                topics_read: 4,
                minutes_read: 100,
                posts_created: 100,
                days_visited: 100,
                helpful_flags: 100,
            }
            .trust_level(),
            TrustLevel::NewSeed
        );
        assert_eq!(
            TrustMetrics {
                topics_read: 5,
                minutes_read: 10,
                posts_created: 0,
                days_visited: 0,
                helpful_flags: 0,
            }
            .trust_level(),
            TrustLevel::Wanderer
        );
        assert_eq!(
            TrustMetrics {
                topics_read: 50,
                minutes_read: 240,
                posts_created: 15,
                days_visited: 10,
                helpful_flags: 0,
            }
            .trust_level(),
            TrustLevel::Citizen
        );
        assert_eq!(
            TrustMetrics {
                topics_read: 200,
                minutes_read: 1_200,
                posts_created: 100,
                days_visited: 50,
                helpful_flags: 20,
            }
            .trust_level(),
            TrustLevel::Elder
        );
    }

    #[test]
    fn ownership_capabilities_require_matching_owner() {
        let actor = ActorPermissions {
            actor_id: "user:1".to_owned(),
            trust_level: TrustLevel::Citizen,
            global_roles: vec![],
            category_roles: vec![],
        };
        let required = Capability::parse_static("post.edit.own");

        assert!(actor.allows(
            &required,
            &PermissionContext {
                actor_id: "user:1".to_owned(),
                owner_id: Some("user:1".to_owned()),
                category_id: Some("category:1".to_owned()),
            }
        ));
        assert!(!actor.allows(
            &required,
            &PermissionContext {
                actor_id: "user:1".to_owned(),
                owner_id: Some("user:2".to_owned()),
                category_id: Some("category:1".to_owned()),
            }
        ));
    }

    #[test]
    fn any_capability_satisfies_own_action() {
        let role = Role::new(
            "role:moderator",
            "Moderator",
            [Capability::parse_static("post.edit.any")],
        );
        let actor = ActorPermissions {
            actor_id: "user:1".to_owned(),
            trust_level: TrustLevel::NewSeed,
            global_roles: vec![role],
            category_roles: vec![],
        };

        assert!(actor.allows(
            &Capability::parse_static("post.edit.own"),
            &PermissionContext {
                actor_id: "user:1".to_owned(),
                owner_id: Some("user:2".to_owned()),
                category_id: Some("category:1".to_owned()),
            }
        ));
    }

    #[test]
    fn category_scoped_roles_do_not_leak() {
        let role = Role::new(
            "role:category-moderator",
            "Category Moderator",
            [Capability::parse_static("topic.delete.any")],
        );
        let actor = ActorPermissions {
            actor_id: "user:1".to_owned(),
            trust_level: TrustLevel::NewSeed,
            global_roles: vec![],
            category_roles: vec![ScopedRole {
                category_id: "category:1".to_owned(),
                role,
            }],
        };
        let required = Capability::parse_static("topic.delete.any");

        assert!(actor.allows(
            &required,
            &PermissionContext {
                actor_id: "user:1".to_owned(),
                owner_id: Some("user:2".to_owned()),
                category_id: Some("category:1".to_owned()),
            }
        ));
        assert!(!actor.allows(
            &required,
            &PermissionContext {
                actor_id: "user:1".to_owned(),
                owner_id: Some("user:2".to_owned()),
                category_id: Some("category:2".to_owned()),
            }
        ));
    }

    #[test]
    fn role_assignment_cannot_escalate() {
        let actor = ActorPermissions {
            actor_id: "user:1".to_owned(),
            trust_level: TrustLevel::NewSeed,
            global_roles: vec![Role::new(
                "role:limited-admin",
                "Limited Admin",
                [Capability::parse_static("user.warn")],
            )],
            category_roles: vec![],
        };

        assert!(
            actor
                .can_assign_role(&Role::new(
                    "role:warning",
                    "Warning Staff",
                    [Capability::parse_static("user.warn")]
                ))
                .is_ok()
        );

        let err = actor
            .can_assign_role(&Role::new(
                "role:admin",
                "Admin",
                [
                    Capability::parse_static("user.warn"),
                    Capability::parse_static("system.settings.write"),
                ],
            ))
            .unwrap_err();
        assert!(matches!(
            err,
            PermissionError::EscalatingRoleAssignment { missing } if missing == vec![Capability::parse_static("system.settings.write")]
        ));
    }

    #[test]
    fn permission_service_resolves_default_and_assigned_roles() {
        let service = PermissionService::new_in_memory([Role::new(
            "role:member",
            "Member",
            [Capability::parse_static("post.reply")],
        )]);
        service
            .define_role(Role::new(
                "role:moderator",
                "Moderator",
                [Capability::parse_static("post.edit.any")],
            ))
            .unwrap();
        service
            .define_role(Role::new(
                "role:moderator-granter",
                "Moderator Granter",
                [Capability::parse_static("post.edit.any")],
            ))
            .unwrap();
        service
            .grant_global_role_for_bootstrap("user:admin", "role:moderator-granter")
            .unwrap();
        service
            .assign_global_role(
                "user:admin",
                TrustLevel::NewSeed,
                "user:2",
                "role:moderator",
            )
            .unwrap();

        let actor = service
            .actor_permissions("user:2", TrustLevel::NewSeed)
            .unwrap();
        let capabilities = actor.effective_capabilities(None);

        assert!(capabilities.contains(&Capability::parse_static("post.reply")));
        assert!(capabilities.contains(&Capability::parse_static("post.edit.any")));
    }

    #[test]
    fn permission_service_blocks_escalating_assignment() {
        let service = PermissionService::new_in_memory([]);
        service
            .define_role(Role::new(
                "role:limited",
                "Limited",
                [Capability::parse_static("user.warn")],
            ))
            .unwrap();
        service
            .define_role(Role::new(
                "role:admin",
                "Admin",
                [Capability::parse_static("system.settings.write")],
            ))
            .unwrap();
        service
            .assign_global_role(
                "user:root",
                TrustLevel::NewSeed,
                "user:actor",
                "role:limited",
            )
            .unwrap_err();
        service
            .grant_global_role_for_bootstrap("user:actor", "role:limited")
            .unwrap();

        let err = service
            .assign_global_role(
                "user:actor",
                TrustLevel::NewSeed,
                "user:target",
                "role:admin",
            )
            .unwrap_err();

        assert!(matches!(
            err,
            PermissionError::EscalatingRoleAssignment { missing } if missing == vec![Capability::parse_static("system.settings.write")]
        ));
    }

    #[test]
    fn permission_service_resolves_category_assignments_without_leaking() {
        let service = PermissionService::new_in_memory([]);
        service
            .define_role(Role::new(
                "role:category-moderator",
                "Category Moderator",
                [Capability::parse_static("topic.delete.any")],
            ))
            .unwrap();
        service
            .grant_global_role_for_bootstrap("user:admin", "role:category-moderator")
            .unwrap();
        service
            .assign_category_role(
                "user:admin",
                TrustLevel::NewSeed,
                "user:mod",
                "category:1",
                "role:category-moderator",
            )
            .unwrap();

        let actor = service
            .actor_permissions("user:mod", TrustLevel::NewSeed)
            .unwrap();
        let required = Capability::parse_static("topic.delete.any");

        assert!(actor.allows(
            &required,
            &PermissionContext {
                actor_id: "user:mod".to_owned(),
                owner_id: Some("user:other".to_owned()),
                category_id: Some("category:1".to_owned()),
            }
        ));
        assert!(!actor.allows(
            &required,
            &PermissionContext {
                actor_id: "user:mod".to_owned(),
                owner_id: Some("user:other".to_owned()),
                category_id: Some("category:2".to_owned()),
            }
        ));
    }
}
