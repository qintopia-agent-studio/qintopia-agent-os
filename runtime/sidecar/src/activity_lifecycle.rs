#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityPhase {
    Pre,
    In,
    Post,
}

impl ActivityPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pre => "pre_event",
            Self::In => "in_event",
            Self::Post => "post_event",
        }
    }

    pub const fn route(self) -> &'static str {
        match self {
            Self::Pre => "promotion_preparation",
            Self::In => "live_support",
            Self::Post => "activity_recap",
        }
    }

    pub const fn root_work_item_type(self) -> &'static str {
        match self {
            Self::Pre => "activity_promotion_request",
            Self::In => "activity_live_support_request",
            Self::Post => "activity_recap_request",
        }
    }

    pub const fn workflow_type(self) -> &'static str {
        match self {
            Self::Pre => "activity_promotion",
            Self::In => "activity_live_support",
            Self::Post => "activity_recap",
        }
    }

    pub const fn needs_visual(self) -> bool {
        !matches!(self, Self::In)
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "pre_event" => Some(Self::Pre),
            "in_event" => Some(Self::In),
            "post_event" => Some(Self::Post),
            _ => None,
        }
    }

    pub fn parse_or_pre_event(value: &str) -> Option<Self> {
        if value.trim().is_empty() {
            Some(Self::Pre)
        } else {
            Self::parse(value)
        }
    }

    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Pre, Self::Pre | Self::In | Self::Post)
                | (Self::In, Self::In | Self::Post)
                | (Self::Post, Self::Post)
        )
    }
}

pub fn initial_phase_for_signal(signal_type: &str) -> Option<ActivityPhase> {
    (signal_type == "活动/聚会").then_some(ActivityPhase::Pre)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_routes_are_fixed_and_visual_scope_is_bounded() {
        assert_eq!(ActivityPhase::Pre.route(), "promotion_preparation");
        assert_eq!(ActivityPhase::In.route(), "live_support");
        assert_eq!(ActivityPhase::Post.route(), "activity_recap");
        assert!(ActivityPhase::Pre.needs_visual());
        assert!(!ActivityPhase::In.needs_visual());
        assert!(ActivityPhase::Post.needs_visual());
    }

    #[test]
    fn phase_transitions_are_forward_only() {
        assert!(ActivityPhase::Pre.can_transition_to(ActivityPhase::In));
        assert!(ActivityPhase::Pre.can_transition_to(ActivityPhase::Post));
        assert!(ActivityPhase::In.can_transition_to(ActivityPhase::Post));
        assert!(ActivityPhase::Post.can_transition_to(ActivityPhase::Post));
        assert!(!ActivityPhase::In.can_transition_to(ActivityPhase::Pre));
        assert!(!ActivityPhase::Post.can_transition_to(ActivityPhase::In));
    }

    #[test]
    fn only_activity_signals_receive_an_initial_phase() {
        assert_eq!(
            initial_phase_for_signal("活动/聚会"),
            Some(ActivityPhase::Pre)
        );
        assert_eq!(initial_phase_for_signal("服务/设施"), None);
        assert_eq!(
            ActivityPhase::parse_or_pre_event(""),
            Some(ActivityPhase::Pre)
        );
        assert_eq!(ActivityPhase::parse("unexpected"), None);
    }
}
