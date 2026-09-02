use anyhow::{bail, Context, Result};
use serde_json::Value;

pub(crate) const RECIPE_METADATA_KEY: &str = "space_execution_recipe";
pub(crate) const QIWE_TEXT_TEMPLATE_V1: &str = "qiwe_text_template_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegisteredRecipe {
    QiweTextTemplateV1,
}

impl RegisteredRecipe {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::QiweTextTemplateV1 => QIWE_TEXT_TEMPLATE_V1,
        }
    }
}

pub(crate) fn from_capability_metadata(metadata: &Value) -> Result<RegisteredRecipe> {
    let recipe = metadata
        .get(RECIPE_METADATA_KEY)
        .and_then(Value::as_str)
        .context("deterministic capability has no registered execution recipe")?;
    match recipe {
        QIWE_TEXT_TEMPLATE_V1 => Ok(RegisteredRecipe::QiweTextTemplateV1),
        _ => bail!("deterministic capability execution recipe is not registered"),
    }
}

pub(crate) fn is_registered_metadata(metadata: &Value) -> bool {
    from_capability_metadata(metadata).is_ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn recipe_dispatch_is_metadata_driven_and_closed() {
        let metadata = json!({RECIPE_METADATA_KEY: QIWE_TEXT_TEMPLATE_V1});
        assert_eq!(
            from_capability_metadata(&metadata).expect("registered recipe"),
            RegisteredRecipe::QiweTextTemplateV1
        );
        assert!(!is_registered_metadata(&json!({})));
        assert!(!is_registered_metadata(
            &json!({RECIPE_METADATA_KEY: "arbitrary_http"})
        ));
    }
}
