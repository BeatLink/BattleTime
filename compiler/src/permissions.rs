//! The permissions an app lists in `define_app`, and the check that they cover every effect,
//! subscription and system value the app uses, as docs/app-reference.md "Permissions" lists them.
//!
//! The image has no place for permissions yet, so the check is all the compiler does with them.

use neotime_runtime::instruction::{EffectKind, SystemValue};
use neotime_runtime::subscriptions::SubscriptionKind;
use oxc_ast::ast::Expression;
use oxc_span::{GetSpan, Span};

use crate::compiler::Compiler;
use crate::constants::{strip_parentheses, Constant};
use crate::diagnostics::{fail, Compiled};
use crate::scope::Scope;

// ============================================================================
// Permissions
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    HeartRate,
    Motion,
    Notifications,
    Phone,
    PhoneMessages,
    Internet,
    Settings,
    Wake,
    Background,
}

impl Permission {
    const ALL: [Permission; 9] = [
        Permission::HeartRate,
        Permission::Motion,
        Permission::Notifications,
        Permission::Phone,
        Permission::PhoneMessages,
        Permission::Internet,
        Permission::Settings,
        Permission::Wake,
        Permission::Background,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Permission::HeartRate => "heart_rate",
            Permission::Motion => "motion",
            Permission::Notifications => "notifications",
            Permission::Phone => "phone",
            Permission::PhoneMessages => "phone_messages",
            Permission::Internet => "internet",
            Permission::Settings => "settings",
            Permission::Wake => "wake",
            Permission::Background => "background",
        }
    }

    pub fn from_name(name: &str) -> Option<Permission> {
        Permission::ALL
            .into_iter()
            .find(|permission| permission.name() == name)
    }
}

/// The permission an effect needs, if any.
pub fn effect_permission(effect: EffectKind) -> Option<Permission> {
    use EffectKind::*;
    Some(match effect {
        MeasureHeartRate | StartHeartRate | StopHeartRate | SetHeartRateLed => Permission::HeartRate,
        Notify | DismissNotification | MarkNotificationsRead => Permission::Notifications,
        CallControl | MusicControl | FindPhone | RequestMusicState | RequestNavigationState => {
            Permission::Phone
        }
        SendToPhone => Permission::PhoneMessages,
        SetSetting | SetDoNotDisturb | SetBluetooth => Permission::Settings,
        WakeScreen => Permission::Wake,
        _ => return None,
    })
}

/// The permission a subscription's kind needs, apart from `background`.
pub fn subscription_permission(kind: SubscriptionKind) -> Option<Permission> {
    use SubscriptionKind::*;
    Some(match kind {
        HeartRate => Permission::HeartRate,
        Motion | Acceleration => Permission::Motion,
        Notification => Permission::Notifications,
        Call | Music | Navigation => Permission::Phone,
        _ => return None,
    })
}

/// Whether a subscription keeps running after the app closes, which needs `background`.
pub fn runs_in_background(kind: SubscriptionKind, only_while_open: bool) -> bool {
    use SubscriptionKind::*;
    !only_while_open && !matches!(kind, Open | Close | ScreenOn | ScreenOff | Derive)
}

/// The permission reading a system value needs: the phone's player and navigation need `phone`, the kept notifications `notifications`.
pub fn system_value_permission(value: SystemValue) -> Option<Permission> {
    use SystemValue::*;
    match value {
        MusicIsPlaying
        | MusicArtist
        | MusicTrack
        | MusicPosition
        | MusicLength
        | NavigationTurn
        | NavigationInstruction
        | NavigationDistance
        | NavigationProgressPercent => Some(Permission::Phone),
        NotificationCount | NotificationId | NotificationApp | NotificationTitle | NotificationBody
        | NotificationTime => Some(Permission::Notifications),
        _ => None,
    }
}

// ============================================================================
// Checking
// ============================================================================

/// One use of something that needs a permission: what it is and where.
#[derive(Clone, Debug)]
pub struct PermissionUse {
    pub permission: Permission,
    pub what: String,
    pub span: Span,
}

impl<'a> Compiler<'a> {
    /// Records that the app uses something needing a permission, the first time for each permission.
    pub fn note_permission(&mut self, permission: Option<Permission>, what: impl Into<String>, span: Span) {
        let Some(permission) = permission else {
            return;
        };
        if !self
            .permission_uses
            .iter()
            .any(|used| used.permission == permission)
        {
            self.permission_uses.push(PermissionUse {
                permission,
                what: what.into(),
                span,
            });
        }
    }

    /// Records a system value read, for the ones that need a permission.
    pub fn note_system_read(&mut self, value: SystemValue, span: Span) {
        let permission = system_value_permission(value);
        let what = match permission {
            Some(Permission::Notifications) => "reading the kept notifications",
            _ => "reading the phone's player or navigation",
        };
        self.note_permission(permission, what, span);
    }

    /// Checks the app's `permissions` against everything it was found to use.
    pub fn check_permissions(
        &self,
        listed: Option<&'a Expression<'a>>,
        root: &std::rc::Rc<Scope<'a>>,
        app_span: Span,
    ) -> Compiled<()> {
        let mut granted: Vec<Permission> = Vec::new();
        if let Some(listed) = listed {
            let Some(Constant::Array(names)) = self.evaluate_constant(listed, root) else {
                return fail(
                    listed.span(),
                    "write `permissions` as an array of names, such as `[\"wake\", \"background\"]`",
                );
            };
            let span = strip_parentheses(listed).span();
            for name in names.iter() {
                let Constant::Text(name) = name else {
                    return fail(span, "each permission is a name, such as \"wake\"");
                };
                let Some(permission) = Permission::from_name(name) else {
                    let known: Vec<&str> = Permission::ALL
                        .iter()
                        .map(|permission| permission.name())
                        .collect();
                    return fail(
                        span,
                        format!(
                            "there is no permission `{name}`; choose from {}",
                            known.join(", ")
                        ),
                    );
                };
                granted.push(permission);
            }
        }
        for used in &self.permission_uses {
            if !granted.contains(&used.permission) {
                let name = used.permission.name();
                let span = if used.span == Span::default() {
                    app_span
                } else {
                    used.span
                };
                return fail(
                    span,
                    format!(
                        "{} needs the `{name}` permission; add \"{name}\" to define_app's `permissions`",
                        used.what
                    ),
                );
            }
        }
        Ok(())
    }
}
