use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};

use crate::strict_json::{parse_strict_bounded_slice, registry_json_limits};

#[derive(Debug, Clone, Copy)]
pub(crate) struct RegistryScanLimits {
    pub max_depth: usize,
    pub max_files: usize,
    pub max_file_bytes: usize,
    pub max_aggregate_bytes: usize,
}

pub(crate) fn collect_registry_files(
    repository_root: &Path,
    registry_root: &Path,
    suffix: &str,
    limits: RegistryScanLimits,
) -> Result<Vec<PathBuf>> {
    if suffix.is_empty() {
        bail!("registry suffix must not be empty");
    }

    let canonical_repository_root = fs::canonicalize(repository_root)
        .with_context(|| format!("canonicalize repository root {}", repository_root.display()))?;
    if !canonical_repository_root.is_dir() {
        bail!("repository root is not a directory");
    }

    let root_metadata = fs::symlink_metadata(registry_root)
        .with_context(|| format!("inspect registry root {}", registry_root.display()))?;
    if root_metadata.file_type().is_symlink() {
        bail!("registry root must not be a symlink");
    }
    if !root_metadata.is_dir() {
        bail!("registry root must be a directory");
    }
    let canonical_registry_root = fs::canonicalize(registry_root)
        .with_context(|| format!("canonicalize registry root {}", registry_root.display()))?;
    ensure_contained(
        &canonical_registry_root,
        &canonical_repository_root,
        "registry root escaped repository root",
    )?;

    let mut state = RegistryScanState {
        files: Vec::new(),
        aggregate_bytes: 0,
    };
    scan_directory(
        &canonical_repository_root,
        &canonical_registry_root,
        &canonical_registry_root,
        suffix,
        limits,
        0,
        &mut state,
    )?;
    state.files.sort();
    Ok(state.files)
}

struct RegistryScanState {
    files: Vec<PathBuf>,
    aggregate_bytes: usize,
}

fn scan_directory(
    canonical_repository_root: &Path,
    canonical_registry_root: &Path,
    directory: &Path,
    suffix: &str,
    limits: RegistryScanLimits,
    depth: usize,
    state: &mut RegistryScanState,
) -> Result<()> {
    if depth > limits.max_depth {
        bail!("registry directory depth exceeds limit");
    }
    ensure_contained(
        directory,
        canonical_repository_root,
        "registry directory escaped repository root",
    )?;
    ensure_contained(
        directory,
        canonical_registry_root,
        "registry directory escaped configured root",
    )?;

    let entries = fs::read_dir(directory)
        .with_context(|| format!("read registry directory {}", directory.display()))?;
    for entry in entries {
        let entry =
            entry.with_context(|| format!("read registry entry under {}", directory.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("read registry file type {}", path.display()))?;
        if file_type.is_symlink() {
            bail!("registry paths must not be symlinks: {}", path.display());
        }

        let canonical_path = fs::canonicalize(&path)
            .with_context(|| format!("canonicalize registry path {}", path.display()))?;
        ensure_contained(
            &canonical_path,
            canonical_repository_root,
            "registry path escaped repository root",
        )?;
        ensure_contained(
            &canonical_path,
            canonical_registry_root,
            "registry path escaped configured root",
        )?;

        if file_type.is_dir() {
            let child_depth = depth
                .checked_add(1)
                .context("registry directory depth overflowed")?;
            scan_directory(
                canonical_repository_root,
                canonical_registry_root,
                &canonical_path,
                suffix,
                limits,
                child_depth,
                state,
            )?;
            continue;
        }
        if !file_type.is_file() {
            bail!("registry contains an unsupported path: {}", path.display());
        }

        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .with_context(|| format!("registry filename must be UTF-8: {}", path.display()))?;
        if !name.ends_with(suffix) {
            continue;
        }
        if state.files.len() >= limits.max_files {
            bail!("registry file count exceeds limit");
        }

        let metadata = entry
            .metadata()
            .with_context(|| format!("read registry metadata {}", path.display()))?;
        let metadata_bytes = usize::try_from(metadata.len())
            .context("registry file byte count does not fit usize")?;
        if metadata_bytes > limits.max_file_bytes {
            bail!("registry file exceeds its byte limit: {}", path.display());
        }
        let bytes = fs::read(&canonical_path)
            .with_context(|| format!("read registry JSON {}", path.display()))?;
        if bytes.len() > limits.max_file_bytes {
            bail!("registry file exceeds its byte limit: {}", path.display());
        }
        let next_aggregate = state
            .aggregate_bytes
            .checked_add(bytes.len())
            .context("registry aggregate byte count overflowed")?;
        if next_aggregate > limits.max_aggregate_bytes {
            bail!("registry aggregate byte limit exceeded");
        }
        parse_strict_bounded_slice(&bytes, registry_json_limits(limits.max_file_bytes))
            .with_context(|| format!("validate registry JSON {}", path.display()))?;

        state.aggregate_bytes = next_aggregate;
        state.files.push(canonical_path);
    }
    Ok(())
}

fn ensure_contained(path: &Path, root: &Path, message: &str) -> Result<()> {
    if !path.starts_with(root) {
        bail!("{message}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_limits() -> RegistryScanLimits {
        RegistryScanLimits {
            max_depth: 2,
            max_files: 4,
            max_file_bytes: 128,
            max_aggregate_bytes: 256,
        }
    }

    fn create_registry() -> (tempfile::TempDir, PathBuf) {
        let repository = tempdir().expect("create temporary repository");
        let registry = repository.path().join("registry");
        fs::create_dir(&registry).expect("create registry directory");
        (repository, registry)
    }

    #[test]
    fn accepts_valid_bounded_registry_in_sorted_order() {
        let (repository, registry) = create_registry();
        fs::write(registry.join("b.mapping.json"), br#"{"version":2}"#)
            .expect("write second mapping");
        fs::write(registry.join("a.mapping.json"), br#"{"version":1}"#)
            .expect("write first mapping");
        fs::write(registry.join("README.md"), "ignored").expect("write ignored file");

        let files =
            collect_registry_files(repository.path(), &registry, ".mapping.json", test_limits())
                .expect("bounded registry is valid");

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].file_name().unwrap(), "a.mapping.json");
        assert_eq!(files[1].file_name().unwrap(), "b.mapping.json");
    }

    #[test]
    fn rejects_duplicate_keys_and_malformed_json() {
        for contents in [br#"{"id":1,"id":2}"#.as_slice(), br#"{"id":1"#.as_slice()] {
            let (repository, registry) = create_registry();
            fs::write(registry.join("bad.mapping.json"), contents).expect("write invalid JSON");
            let error = collect_registry_files(
                repository.path(),
                &registry,
                ".mapping.json",
                test_limits(),
            )
            .expect_err("invalid JSON must fail");
            assert!(format!("{error:#}").contains("validate registry JSON"));
        }
    }

    #[test]
    fn rejects_depth_file_count_and_aggregate_byte_limits() {
        let (repository, registry) = create_registry();
        let deep = registry.join("one/two/three");
        fs::create_dir_all(&deep).expect("create deep registry path");
        fs::write(deep.join("deep.mapping.json"), "{}").expect("write deep mapping");
        assert!(collect_registry_files(
            repository.path(),
            &registry,
            ".mapping.json",
            test_limits(),
        )
        .is_err());

        let (repository, registry) = create_registry();
        for index in 0..=test_limits().max_files {
            fs::write(registry.join(format!("{index}.mapping.json")), "{}")
                .expect("write counted mapping");
        }
        assert!(collect_registry_files(
            repository.path(),
            &registry,
            ".mapping.json",
            test_limits(),
        )
        .is_err());

        let (repository, registry) = create_registry();
        let limits = RegistryScanLimits {
            max_aggregate_bytes: 8,
            ..test_limits()
        };
        fs::write(registry.join("a.mapping.json"), r#"{"a":1}"#)
            .expect("write first aggregate mapping");
        fs::write(registry.join("b.mapping.json"), r#"{"b":2}"#)
            .expect("write second aggregate mapping");
        assert!(
            collect_registry_files(repository.path(), &registry, ".mapping.json", limits,).is_err()
        );
    }

    #[test]
    fn rejects_registry_root_outside_repository() {
        let repository = tempdir().expect("create temporary repository");
        let outside = tempdir().expect("create outside registry");
        let error = collect_registry_files(
            repository.path(),
            outside.path(),
            ".mapping.json",
            test_limits(),
        )
        .expect_err("outside registry must fail");
        assert!(error.to_string().contains("escaped repository root"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_root_and_child_symlinks() {
        use std::os::unix::fs::symlink;

        let repository = tempdir().expect("create temporary repository");
        let actual = repository.path().join("actual");
        fs::create_dir(&actual).expect("create actual registry");
        let root_link = repository.path().join("registry");
        symlink(&actual, &root_link).expect("create root symlink");
        assert!(collect_registry_files(
            repository.path(),
            &root_link,
            ".mapping.json",
            test_limits(),
        )
        .is_err());

        let (repository, registry) = create_registry();
        let target = repository.path().join("target.mapping.json");
        fs::write(&target, "{}").expect("write symlink target");
        symlink(&target, registry.join("linked.mapping.json")).expect("create child symlink");
        assert!(collect_registry_files(
            repository.path(),
            &registry,
            ".mapping.json",
            test_limits(),
        )
        .is_err());
    }
}
