//! Single source of truth for the QiWe image-send claim SQL branches.
//!
//! `qiwe_image_send_state` issues four separate SQL statements
//! (claim / preview / lock-claim / lock-callback) that all embed the same
//! "which upstream image workflow produced this artifact" branch. Those
//! branches were historically copy-pasted; this module defines them once as
//! data and renders the exact SQL fragments every statement shares.
//!
//! The rendered fragments are byte-for-byte identical to the SQL that was
//! inline before this refactor (asserted by the snapshot unit tests below),
//! so the production claim semantics do not change. Adding a third claimable
//! workflow (e.g. the erhua morning brief) only requires appending one entry
//! to `CLAIMABLE_IMAGE_SOURCES` — all four statements pick it up.

use std::fmt::Write as _;

/// One upstream image workflow whose approved artifacts may be claimed by the
/// QiWe image-send adapter.
pub struct ClaimableImageSource {
    /// `artifacts.created_by_agent` value identifying the workflow's artifacts.
    pub created_by_agent: &'static str,
    /// `work_items.work_item_type` of the upstream (source) request.
    pub source_work_item_type: &'static str,
    /// `work_items.capability_key` of the upstream (source) request.
    pub source_capability_key: &'static str,
    /// `work_items.target_agent` of the upstream (source) request.
    pub source_target_agent: &'static str,
    /// When `Some`, artifacts must additionally carry
    /// `metadata->>'workflow_type' = <value>` (workflows that share one agent).
    pub artifact_workflow_type: Option<&'static str>,
    /// `review_policy` the send request must use for this workflow.
    pub review_policy: &'static str,
}

/// Claimable image workflows, in render order (huabaosi, xiaoman).
pub const CLAIMABLE_IMAGE_SOURCES: &[ClaimableImageSource] = &[
    ClaimableImageSource {
        created_by_agent: "huabaosi",
        source_work_item_type: "image_generation_request",
        source_capability_key: "huabaosi.generate_image_asset",
        source_target_agent: "huabaosi",
        artifact_workflow_type: None,
        review_policy: "human_final_confirmation",
    },
    ClaimableImageSource {
        created_by_agent: "xiaoman",
        source_work_item_type: "daily_case_report_request",
        source_capability_key: "xiaoman.daily_case_report_auto_publish",
        source_target_agent: "xiaoman",
        artifact_workflow_type: Some("daily_case_report"),
        review_policy: "automatic_publish",
    },
];

fn write_source_branch(out: &mut String, source: &ClaimableImageSource, unit: &str) {
    let _ = writeln!(out, "{unit}(");
    let _ = writeln!(
        out,
        "{unit}    artifact.created_by_agent = '{created_by_agent}'",
        created_by_agent = source.created_by_agent
    );
    if let Some(artifact_workflow_type) = source.artifact_workflow_type {
        let _ = writeln!(
            out,
            "{unit}    AND artifact.metadata->>'workflow_type' = '{artifact_workflow_type}'"
        );
    }
    let _ = writeln!(
        out,
        "{unit}    AND source_request.work_item_type = '{source_work_item_type}'",
        source_work_item_type = source.source_work_item_type
    );
    let _ = writeln!(
        out,
        "{unit}    AND source_request.capability_key = '{source_capability_key}'",
        source_capability_key = source.source_capability_key
    );
    let _ = writeln!(
        out,
        "{unit}    AND source_request.target_agent = '{source_target_agent}'",
        source_target_agent = source.source_target_agent
    );
    let _ = writeln!(out, "{unit}    AND source_request.status = 'completed'");
    let _ = write!(out, "{unit})");
}

/// Renders the `AND (... OR ...)` branch that joins `source_request` to the
/// upstream workflow which produced the artifact. The returned string keeps
/// no trailing newline; the JOIN/ON/JOIN-prefix lines stay in each query.
pub fn claimable_image_source_join_condition(indent: &str) -> String {
    let unit = format!("{indent}    ");
    let mut out = String::new();
    let _ = writeln!(out, "{indent}AND (");
    for (index, source) in CLAIMABLE_IMAGE_SOURCES.iter().enumerate() {
        if index > 0 {
            let _ = writeln!(out);
            let _ = writeln!(out, "{unit}OR");
        }
        write_source_branch(&mut out, source, &unit);
    }
    let _ = writeln!(out);
    let _ = write!(out, "{indent})");
    out
}

fn write_review_policy_branch(
    out: &mut String,
    source: &ClaimableImageSource,
    unit: &str,
    or_prefixed: bool,
) {
    if or_prefixed {
        let _ = writeln!(out, "{unit}OR (");
    } else {
        let _ = writeln!(out, "{unit}(");
    }
    let _ = writeln!(
        out,
        "{unit}    artifact.created_by_agent = '{created_by_agent}'",
        created_by_agent = source.created_by_agent
    );
    let _ = writeln!(
        out,
        "{unit}    AND request.review_policy = '{review_policy}'",
        review_policy = source.review_policy
    );
    if let Some(artifact_workflow_type) = source.artifact_workflow_type {
        let _ = writeln!(
            out,
            "{unit}    AND request.payload->>'workflow_type' = '{artifact_workflow_type}'"
        );
        let _ = writeln!(
            out,
            "{unit}    AND request.payload->>'requires_human_final_confirmation' = 'false'"
        );
    }
    let _ = write!(out, "{unit})");
}

/// Renders the `AND ((agent + review_policy) OR ...)` branch pairing each
/// workflow's agent with the review policy the send request must use.
/// Returned string keeps no trailing newline.
pub fn claimable_image_review_policy_condition(indent: &str) -> String {
    let unit = format!("{indent}    ");
    let mut out = String::new();
    let _ = writeln!(out, "{indent}AND (");
    for (index, source) in CLAIMABLE_IMAGE_SOURCES.iter().enumerate() {
        if index > 0 {
            let _ = writeln!(out);
        }
        write_review_policy_branch(&mut out, source, &unit, index > 0);
    }
    let _ = writeln!(out);
    let _ = write!(out, "{indent})");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Byte-for-byte snapshot of the JOIN branch previously inline at all four
    // claim sites (claim_ready_work_item / preview_ready_work_item /
    // lock_current_claim / lock_callback_policy).
    const JOIN_BRANCH_SNAPSHOT: &str = r#"AND (
    (
        artifact.created_by_agent = 'huabaosi'
        AND source_request.work_item_type = 'image_generation_request'
        AND source_request.capability_key = 'huabaosi.generate_image_asset'
        AND source_request.target_agent = 'huabaosi'
        AND source_request.status = 'completed'
    )
    OR
    (
        artifact.created_by_agent = 'xiaoman'
        AND artifact.metadata->>'workflow_type' = 'daily_case_report'
        AND source_request.work_item_type = 'daily_case_report_request'
        AND source_request.capability_key = 'xiaoman.daily_case_report_auto_publish'
        AND source_request.target_agent = 'xiaoman'
        AND source_request.status = 'completed'
    )
)"#;

    // Byte-for-byte snapshot of the agent + review_policy branch previously
    // inline at all four claim sites (twice in lock_callback_policy).
    const REVIEW_POLICY_BRANCH_SNAPSHOT: &str = r#"AND (
    (
        artifact.created_by_agent = 'huabaosi'
        AND request.review_policy = 'human_final_confirmation'
    )
    OR (
        artifact.created_by_agent = 'xiaoman'
        AND request.review_policy = 'automatic_publish'
        AND request.payload->>'workflow_type' = 'daily_case_report'
        AND request.payload->>'requires_human_final_confirmation' = 'false'
    )
)"#;

    #[test]
    fn join_branch_matches_pre_refactor_sql_byte_for_byte() {
        assert_eq!(
            claimable_image_source_join_condition(""),
            JOIN_BRANCH_SNAPSHOT
        );
    }

    #[test]
    fn review_policy_branch_matches_pre_refactor_sql_byte_for_byte() {
        assert_eq!(
            claimable_image_review_policy_condition(""),
            REVIEW_POLICY_BRANCH_SNAPSHOT
        );
    }

    #[test]
    fn indentation_shifts_every_line() {
        let join = claimable_image_source_join_condition("            ");
        assert!(join.starts_with("            AND (\n"));
        assert!(join.contains("\n                (\n"));
        assert!(join.contains("\n                OR\n"));
        assert!(join.ends_with("\n            )"));

        let policy = claimable_image_review_policy_condition("          ");
        assert!(policy.starts_with("          AND (\n"));
        assert!(policy.contains("\n              OR (\n"));
        assert!(policy.ends_with("\n          )"));
    }

    #[test]
    fn every_claimable_source_is_rendered_in_both_branches() {
        let join = claimable_image_source_join_condition("");
        let policy = claimable_image_review_policy_condition("");
        for source in CLAIMABLE_IMAGE_SOURCES {
            let agent = format!("artifact.created_by_agent = '{}'", source.created_by_agent);
            assert_eq!(join.matches(&agent).count(), 1, "join branch: {agent}");
            assert_eq!(policy.matches(&agent).count(), 1, "policy branch: {agent}");
            assert!(join.contains(source.source_capability_key));
            assert!(policy.contains(source.review_policy));
        }
    }
}
