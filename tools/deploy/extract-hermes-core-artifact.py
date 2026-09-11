#!/usr/bin/env python3

"""Safely extract and durably commit a Hermes core ingress artifact.

The shell wrapper is the production entry point.  The functions are kept
importable so the repository fixture can exercise the archive boundary without
needing root or writing under /var/lib.
"""

from __future__ import annotations

import gzip
import hashlib
import json
import os
import posixpath
import re
import stat
import sys
import tarfile
import zlib
from pathlib import PurePosixPath
from typing import Callable, Iterable


OFFICIAL_REPOSITORY = "https://github.com/NousResearch/hermes-agent.git"
FIXED_STATE_ROOT = "/var/lib/qintopia-agent-os-deploy"
FIXED_INGRESS_ROOT = os.path.join(FIXED_STATE_ROOT, "hermes-core-ingress")

MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
MAX_METADATA_BYTES = 8 * 1024 * 1024
MAX_EXTENDED_HEADER_BYTES = 1024 * 1024
MAX_EXTENDED_HEADER_TOTAL_BYTES = 8 * 1024 * 1024
MAX_EXTENDED_HEADERS = 1024
MAX_MEMBER_BYTES = 256 * 1024 * 1024
MAX_TOTAL_BYTES = 1 * 1024 * 1024 * 1024
MAX_MEMBERS = 200000
MAX_UNCOMPRESSED_ARCHIVE_BYTES = (
    MAX_TOTAL_BYTES + MAX_MEMBERS * 1024 + 20 * 1024 * 1024
)
READ_CHUNK_BYTES = 1024 * 1024
TAR_BLOCK_BYTES = 512
ZERO_TAR_BLOCK = b"\x00" * TAR_BLOCK_BYTES
TAR_EXTENSION_TYPES = frozenset(
    {
        tarfile.XHDTYPE,
        tarfile.XGLTYPE,
        getattr(tarfile, "SOLARIS_XHDTYPE", b"X"),
        tarfile.GNUTYPE_LONGNAME,
        tarfile.GNUTYPE_LONGLINK,
    }
)

SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
COMMIT_PATTERN = re.compile(r"^[0-9a-f]{40}$")
TAG_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
PATH_COMPONENT_PATTERN = re.compile(r"^[A-Za-z0-9._+@-]+$")

METADATA_FILES = (
    "artifact-manifest.json",
    "build-receipt.json",
    "update-receipt.json",
    "validation-summary.json",
)
TOP_LEVEL_FILES = METADATA_FILES + ("SHA256SUMS",)
TOP_LEVEL_ENTRIES = set(TOP_LEVEL_FILES) | {"core", "runtime"}


class ArtifactError(Exception):
    """An expected, sanitized boundary failure."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


def _fail(code: str) -> None:
    raise ArtifactError(code)


def _validate_expected(expected: dict[str, str]) -> None:
    if not TAG_PATTERN.fullmatch(expected.get("tag", "")):
        _fail("invalid_invocation")
    if not COMMIT_PATTERN.fullmatch(expected.get("commit", "")):
        _fail("invalid_invocation")
    for key in ("archive_sha256", "source_sha256", "identity_sha256", "manifest_sha256"):
        if not SHA256_PATTERN.fullmatch(expected.get(key, "")):
            _fail("invalid_invocation")


def _same_owner(metadata: os.stat_result, owner: tuple[int, int]) -> bool:
    return metadata.st_uid == owner[0] and metadata.st_gid == owner[1]


def _mode(metadata: os.stat_result) -> str:
    return format(stat.S_IMODE(metadata.st_mode), "04o")


def _lstat_regular(file_path: str, owner: tuple[int, int], code: str) -> os.stat_result:
    try:
        metadata = os.lstat(file_path)
    except OSError:
        _fail(code)
    if (
        stat.S_ISLNK(metadata.st_mode)
        or not stat.S_ISREG(metadata.st_mode)
        or metadata.st_nlink != 1
        or not _same_owner(metadata, owner)
    ):
        _fail(code)
    return metadata


def _lstat_directory(directory_path: str, owner: tuple[int, int], code: str) -> os.stat_result:
    try:
        metadata = os.lstat(directory_path)
    except OSError:
        _fail(code)
    if (
        stat.S_ISLNK(metadata.st_mode)
        or not stat.S_ISDIR(metadata.st_mode)
        or not _same_owner(metadata, owner)
    ):
        _fail(code)
    return metadata


def _chown(path_value: str, owner: tuple[int, int]) -> None:
    try:
        os.chown(path_value, owner[0], owner[1], follow_symlinks=False)
    except (AttributeError, TypeError):
        os.lchown(path_value, owner[0], owner[1])
    except OSError:
        _fail("artifact_owner_normalization_failed")


def _chmod(path_value: str, mode: int) -> None:
    try:
        os.chmod(path_value, mode, follow_symlinks=False)
    except (AttributeError, TypeError):
        os.chmod(path_value, mode)
    except OSError:
        _fail("artifact_mode_normalization_failed")


def _fsync_path(path_value: str) -> None:
    flags = os.O_RDONLY
    flags |= getattr(os, "O_NOFOLLOW", 0)
    if os.path.isdir(path_value):
        flags |= getattr(os, "O_DIRECTORY", 0)
    try:
        descriptor = os.open(path_value, flags)
    except OSError:
        _fail("artifact_fsync_failed")
    try:
        os.fsync(descriptor)
    except OSError:
        _fail("artifact_fsync_failed")
    finally:
        os.close(descriptor)


def _fsync_tree(root: str, sync_path: Callable[[str], None]) -> None:
    for current_root, directory_names, file_names in os.walk(root, topdown=False, followlinks=False):
        for name in sorted(file_names):
            sync_path(os.path.join(current_root, name))
        for name in sorted(directory_names):
            sync_path(os.path.join(current_root, name))
    sync_path(root)


def _sha256_file(file_path: str) -> str:
    digest = hashlib.sha256()
    try:
        with open(file_path, "rb") as source:
            while True:
                chunk = source.read(READ_CHUNK_BYTES)
                if not chunk:
                    break
                digest.update(chunk)
    except OSError:
        _fail("archive_unreadable")
    return digest.hexdigest()


def _validate_gzip_stream(file_path: str) -> None:
    total = 0
    try:
        with gzip.open(file_path, "rb") as source:
            while True:
                chunk = source.read(READ_CHUNK_BYTES)
                if not chunk:
                    break
                total += len(chunk)
                if total > MAX_UNCOMPRESSED_ARCHIVE_BYTES:
                    _fail("archive_size_invalid")
    except ArtifactError:
        raise
    except (EOFError, gzip.BadGzipFile, OSError, zlib.error):
        _fail("archive_truncated")


def _prescan_tar_headers(file_path: str) -> None:
    """Validate raw tar framing before tarfile can consume extension bodies."""

    uncompressed_bytes = 0
    member_headers = 0
    extended_headers = 0
    extended_header_bytes = 0
    try:
        with gzip.open(file_path, "rb") as source:

            def read_exact(length: int) -> bytes | None:
                nonlocal uncompressed_bytes
                if length < 0:
                    _fail("archive_header_invalid")
                buffer = bytearray()
                while len(buffer) < length:
                    chunk = source.read(min(READ_CHUNK_BYTES, length - len(buffer)))
                    if not chunk:
                        if not buffer:
                            return None
                        _fail("archive_truncated")
                    uncompressed_bytes += len(chunk)
                    if uncompressed_bytes > MAX_UNCOMPRESSED_ARCHIVE_BYTES:
                        _fail("archive_size_invalid")
                    if len(chunk) > length - len(buffer):
                        _fail("archive_truncated")
                    buffer.extend(chunk)
                return bytes(buffer)

            def discard(length: int) -> None:
                nonlocal uncompressed_bytes
                remaining = length
                while remaining:
                    chunk = source.read(min(READ_CHUNK_BYTES, remaining))
                    if not chunk:
                        _fail("archive_truncated")
                    uncompressed_bytes += len(chunk)
                    if uncompressed_bytes > MAX_UNCOMPRESSED_ARCHIVE_BYTES:
                        _fail("archive_size_invalid")
                    if len(chunk) > remaining:
                        _fail("archive_truncated")
                    remaining -= len(chunk)

            def reject_nonzero_tail() -> None:
                nonlocal uncompressed_bytes
                while True:
                    chunk = source.read(READ_CHUNK_BYTES)
                    if not chunk:
                        return
                    uncompressed_bytes += len(chunk)
                    if uncompressed_bytes > MAX_UNCOMPRESSED_ARCHIVE_BYTES:
                        _fail("archive_size_invalid")
                    if any(chunk):
                        _fail("archive_trailing_data")

            while True:
                header = read_exact(TAR_BLOCK_BYTES)
                if header is None:
                    _fail("archive_truncated")
                if header == ZERO_TAR_BLOCK:
                    second_end = read_exact(TAR_BLOCK_BYTES)
                    if second_end is None:
                        _fail("archive_truncated")
                    if second_end != ZERO_TAR_BLOCK:
                        _fail("archive_header_invalid")
                    reject_nonzero_tail()
                    return

                try:
                    member = tarfile.TarInfo.frombuf(
                        header,
                        "utf-8",
                        "surrogateescape",
                    )
                except (tarfile.HeaderError, OverflowError, UnicodeError, ValueError):
                    _fail("archive_header_invalid")

                if member.type == tarfile.GNUTYPE_SPARSE:
                    _fail("archive_sparse_rejected")
                if member.size < 0:
                    _fail("archive_size_invalid")
                if member.type in TAR_EXTENSION_TYPES:
                    extended_headers += 1
                    extended_header_bytes += member.size
                    if member.size > MAX_EXTENDED_HEADER_BYTES:
                        _fail("archive_extension_size_invalid")
                    if (
                        extended_headers > MAX_EXTENDED_HEADERS
                        or extended_header_bytes > MAX_EXTENDED_HEADER_TOTAL_BYTES
                    ):
                        _fail("archive_extension_total_invalid")
                elif member.size > MAX_MEMBER_BYTES:
                    _fail("archive_size_invalid")
                else:
                    member_headers += 1
                    if member_headers > MAX_MEMBERS:
                        _fail("archive_entry_count_invalid")

                padded_size = ((member.size + TAR_BLOCK_BYTES - 1) // TAR_BLOCK_BYTES) * TAR_BLOCK_BYTES
                discard(padded_size)
    except ArtifactError:
        raise
    except (EOFError, gzip.BadGzipFile, OSError, zlib.error):
        _fail("archive_truncated")


def _bounded_json(file_path: str, owner: tuple[int, int]) -> object:
    metadata = _lstat_regular(file_path, owner, "artifact_metadata_invalid")
    if metadata.st_size <= 0 or metadata.st_size > MAX_METADATA_BYTES:
        _fail("artifact_metadata_invalid")

    def reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
        value: dict[str, object] = {}
        for key, item in pairs:
            if key in value:
                _fail("artifact_metadata_invalid")
            value[key] = item
        return value

    try:
        with open(file_path, encoding="utf-8") as source:
            return json.load(source, object_pairs_hook=reject_duplicate_keys)
    except ArtifactError:
        raise
    except (OSError, ValueError, UnicodeError):
        _fail("artifact_metadata_invalid")


def _canonical_member_path(name: str) -> str:
    if (
        not isinstance(name, str)
        or not name
        or len(name) > 1024
        or "\x00" in name
        or "\\" in name
        or name.startswith("/")
        or re.match(r"^[A-Za-z]:", name)
    ):
        _fail("archive_path_invalid")

    if name.endswith("/"):
        name = name.rstrip("/")
    if not name or posixpath.isabs(name):
        _fail("archive_path_invalid")

    parts = name.split("/")
    if any(part in ("", ".", "..") for part in parts):
        _fail("archive_path_invalid")
    if not all(PATH_COMPONENT_PATTERN.fullmatch(part) for part in parts):
        _fail("archive_path_invalid")
    if parts[0] not in TOP_LEVEL_ENTRIES:
        _fail("archive_layout_invalid")
    if parts[0] not in {"core", "runtime"} and len(parts) != 1:
        _fail("archive_layout_invalid")
    if parts[0] in {"core", "runtime"} and len(parts) == 1:
        return parts[0]
    return str(PurePosixPath(*parts))


def _member_kind(member: tarfile.TarInfo) -> str:
    if member.issym() or member.islnk():
        _fail("archive_link_rejected")
    if member.isdev() or member.isfifo() or member.ischr() or member.isblk():
        _fail("archive_special_file_rejected")
    if member.isdir():
        if member.size != 0:
            _fail("archive_size_invalid")
        return "directory"
    if member.isreg():
        return "file"
    _fail("archive_special_file_rejected")


def _preflight_members(archive: tarfile.TarFile) -> list[tuple[tarfile.TarInfo, str, str]]:
    seen: dict[str, str] = {}
    result: list[tuple[tarfile.TarInfo, str, str]] = []
    total_bytes = 0
    try:
        for member in archive:
            if len(result) >= MAX_MEMBERS:
                _fail("archive_entry_count_invalid")
            relative_path = _canonical_member_path(member.name)
            kind = _member_kind(member)
            if relative_path in seen:
                _fail("archive_duplicate_path")
            seen[relative_path] = kind
            if kind == "file":
                if member.size < 0 or member.size > MAX_MEMBER_BYTES:
                    _fail("archive_size_invalid")
                if (
                    relative_path in TOP_LEVEL_ENTRIES
                    and member.size > MAX_METADATA_BYTES
                ):
                    _fail("archive_size_invalid")
                total_bytes += member.size
                if total_bytes > MAX_TOTAL_BYTES:
                    _fail("archive_size_invalid")
            result.append((member, relative_path, kind))
    except ArtifactError:
        raise
    except (OSError, EOFError, tarfile.TarError):
        _fail("archive_truncated")
    if not result:
        _fail("archive_entry_count_invalid")

    for relative_path, kind in seen.items():
        parts = relative_path.split("/")
        for index in range(1, len(parts)):
            parent = "/".join(parts[:index])
            if seen.get(parent) != "directory":
                _fail("archive_path_conflict")

    actual_top_level = {path.split("/", 1)[0] for path in seen}
    if (
        actual_top_level != TOP_LEVEL_ENTRIES
        or seen.get("core") != "directory"
        or seen.get("runtime") != "directory"
    ):
        _fail("archive_layout_invalid")
    for required in TOP_LEVEL_FILES:
        if seen.get(required) != "file":
            _fail("archive_layout_invalid")
    return result


def _safe_join(root: str, relative_path: str) -> str:
    candidate = os.path.abspath(os.path.join(root, *relative_path.split("/")))
    root_absolute = os.path.abspath(root)
    if candidate != root_absolute and not candidate.startswith(root_absolute + os.sep):
        _fail("archive_path_invalid")
    return candidate


def _create_directory(path_value: str, owner: tuple[int, int]) -> None:
    try:
        os.mkdir(path_value, 0o700)
    except FileExistsError:
        _fail("extract_destination_exists")
    except OSError:
        _fail("extract_directory_failed")
    _chown(path_value, owner)


def _write_member(
    archive: tarfile.TarFile,
    member: tarfile.TarInfo,
    destination: str,
    owner: tuple[int, int],
) -> None:
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    flags |= getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(destination, flags, 0o600)
    except FileExistsError:
        _fail("archive_duplicate_path")
    except OSError:
        _fail("extract_file_failed")

    source = None
    try:
        source = archive.extractfile(member)
        if source is None:
            _fail("archive_truncated")
        remaining = member.size
        while remaining:
            chunk = source.read(min(READ_CHUNK_BYTES, remaining))
            if not chunk:
                _fail("archive_truncated")
            view = memoryview(chunk)
            while view:
                written = os.write(descriptor, view)
                if written <= 0:
                    _fail("extract_file_failed")
                view = view[written:]
            remaining -= len(chunk)
        if source.read(1):
            _fail("archive_size_invalid")
        os.fsync(descriptor)
        _chown(destination, owner)
    except ArtifactError:
        raise
    except (OSError, tarfile.TarError):
        _fail("extract_file_failed")
    finally:
        if source is not None:
            source.close()
        os.close(descriptor)


def _validate_manifest_inventory(
    manifest: object,
    members: Iterable[tuple[tarfile.TarInfo, str, str]],
    expected: dict[str, str],
) -> dict[str, int]:
    if not isinstance(manifest, dict):
        _fail("manifest_schema_invalid")
    for key, expected_value in (
        ("repository", OFFICIAL_REPOSITORY),
        ("tag", expected["tag"]),
        ("commit_sha", expected["commit"]),
        ("source_archive_sha256", expected["source_sha256"]),
        ("artifact_identity_sha256", expected["identity_sha256"]),
        ("artifact_name", f"hermes-core-{expected['commit']}"),
    ):
        if manifest.get(key) != expected_value:
            _fail("artifact_identity_mismatch")

    entries = manifest.get("files")
    if not isinstance(entries, list) or not entries or len(entries) > MAX_MEMBERS:
        _fail("manifest_schema_invalid")

    expected_modes: dict[str, int] = {}
    for entry in entries:
        if not isinstance(entry, dict):
            _fail("manifest_schema_invalid")
        relative_path = entry.get("path")
        if not isinstance(relative_path, str) or not (
            relative_path.startswith("core/")
            or relative_path.startswith("runtime/")
        ):
            _fail("manifest_schema_invalid")
        if _canonical_member_path(relative_path) != relative_path:
            _fail("manifest_schema_invalid")
        if relative_path in expected_modes:
            _fail("manifest_schema_invalid")
        owner_name = entry.get("owner")
        if owner_name != "release-owner":
            _fail("manifest_schema_invalid")
        entry_type = entry.get("type")
        if entry_type == "directory":
            if entry.get("mode") != "0555":
                _fail("manifest_schema_invalid")
            expected_modes[relative_path] = 0o555
        elif entry_type == "file":
            if entry.get("mode") not in ("0444", "0555"):
                _fail("manifest_schema_invalid")
            if not isinstance(entry.get("size_bytes"), int) or entry["size_bytes"] < 0:
                _fail("manifest_schema_invalid")
            if entry["size_bytes"] > MAX_MEMBER_BYTES:
                _fail("manifest_schema_invalid")
            if not SHA256_PATTERN.fullmatch(str(entry.get("sha256", ""))):
                _fail("manifest_schema_invalid")
            expected_modes[relative_path] = int(entry["mode"], 8)
        else:
            _fail("manifest_schema_invalid")

    actual_artifact = {
        relative_path
        for _, relative_path, _ in members
        if relative_path.startswith("core/") or relative_path.startswith("runtime/")
    }
    if actual_artifact != set(expected_modes):
        _fail("manifest_inventory_mismatch")
    return expected_modes


def _normalize_artifact(
    output: str,
    members: list[tuple[tarfile.TarInfo, str, str]],
    expected: dict[str, str],
    owner: tuple[int, int],
    sync_path: Callable[[str], None],
) -> None:
    manifest_path = os.path.join(output, "artifact-manifest.json")
    manifest = _bounded_json(manifest_path, owner)
    if _sha256_file(manifest_path) != expected["manifest_sha256"]:
        _fail("manifest_digest_mismatch")
    artifact_modes = _validate_manifest_inventory(manifest, members, expected)

    expected_modes: dict[str, int] = {
        name: 0o444 for name in TOP_LEVEL_FILES
    }
    expected_modes["core"] = 0o555
    expected_modes["runtime"] = 0o555
    expected_modes.update(artifact_modes)

    for _, relative_path, kind in members:
        path_value = _safe_join(output, relative_path)
        try:
            metadata = os.lstat(path_value)
        except OSError:
            _fail("artifact_entry_missing")
        if stat.S_ISLNK(metadata.st_mode):
            _fail("artifact_symlink_rejected")
        if kind == "directory" and not stat.S_ISDIR(metadata.st_mode):
            _fail("artifact_entry_type_invalid")
        if kind == "file" and (
            not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1
        ):
            _fail("artifact_entry_type_invalid")
        if relative_path not in expected_modes:
            _fail("artifact_layout_invalid")
        _chown(path_value, owner)
        _chmod(path_value, expected_modes[relative_path])

    _chown(output, owner)
    _chmod(output, 0o555)
    _fsync_tree(output, sync_path)


def extract_archive(
    archive_path: str,
    output: str,
    expected: dict[str, str],
    *,
    owner: tuple[int, int] = (0, 0),
    sync_path: Callable[[str], None] = _fsync_path,
) -> None:
    """Verify, safely extract and normalize one archive into ``output``."""

    _validate_expected(expected)
    archive_metadata = _lstat_regular(archive_path, owner, "archive_unreadable")
    if archive_metadata.st_size <= 0 or archive_metadata.st_size > MAX_ARCHIVE_BYTES:
        _fail("archive_size_invalid")
    if _sha256_file(archive_path) != expected["archive_sha256"]:
        _fail("archive_digest_mismatch")
    _validate_gzip_stream(archive_path)
    _prescan_tar_headers(archive_path)

    output = os.path.abspath(output)
    parent = os.path.dirname(output)
    _lstat_directory(parent, owner, "extract_parent_invalid")
    if os.path.lexists(output):
        _fail("extract_destination_exists")
    _create_directory(output, owner)

    try:
        with tarfile.open(archive_path, mode="r:gz") as archive:
            members = _preflight_members(archive)
            directories = [
                (member, relative_path)
                for member, relative_path, kind in members
                if kind == "directory"
            ]
            for _, relative_path in sorted(
                directories, key=lambda item: (item[1].count("/"), item[1])
            ):
                _create_directory(_safe_join(output, relative_path), owner)
            for member, relative_path, kind in members:
                if kind == "file":
                    _write_member(
                        archive,
                        member,
                        _safe_join(output, relative_path),
                        owner,
                    )
            _normalize_artifact(output, members, expected, owner, sync_path)
    except ArtifactError:
        raise
    except (OSError, EOFError, tarfile.TarError):
        _fail("archive_truncated")


def _ensure_commit_directory(path_value: str, owner: tuple[int, int]) -> None:
    _lstat_directory(path_value, owner, "ingress_root_invalid")
    try:
        metadata = os.lstat(path_value)
    except OSError:
        _fail("ingress_root_invalid")
    if stat.S_IMODE(metadata.st_mode) != 0o700:
        _fail("ingress_root_invalid")


def commit_artifact(
    staging: str,
    commit: str,
    *,
    ingress_root: str = FIXED_INGRESS_ROOT,
    owner: tuple[int, int] = (0, 0),
    sync_path: Callable[[str], None] = _fsync_path,
    rename: Callable[[str, str], None] = os.rename,
) -> None:
    """Atomically publish a normalized artifact, fail-closed on uncertainty."""

    if not COMMIT_PATTERN.fullmatch(commit):
        _fail("invalid_invocation")
    staging = os.path.abspath(staging)
    ingress_root = os.path.abspath(ingress_root)
    _lstat_directory(staging, owner, "staging_artifact_invalid")
    _ensure_commit_directory(ingress_root, owner)
    if os.path.dirname(staging) != ingress_root:
        _fail("staging_artifact_invalid")
    if not re.fullmatch(rf"\.staging-{re.escape(commit)}\.[A-Za-z0-9]{{6}}", os.path.basename(staging)):
        _fail("staging_artifact_invalid")
    target = os.path.join(ingress_root, commit)
    if os.path.lexists(target):
        _fail("ingress_destination_exists")

    sync_path(staging)
    renamed = False
    try:
        rename(staging, target)
        renamed = True
    except ArtifactError:
        raise
    except Exception:
        if os.path.lexists(target) and not os.path.lexists(staging):
            _fail("commit_uncertain")
        _fail("rename_failed")

    if not renamed:
        _fail("rename_failed")
    try:
        sync_path(ingress_root)
    except ArtifactError:
        _fail("commit_uncertain")
    except Exception:
        _fail("commit_uncertain")


def _parse_arguments(argv: list[str]) -> tuple[str, dict[str, str]]:
    if not argv:
        _fail("invalid_invocation")
    flags = {"--extract", "--commit-artifact"}
    values = {
        "--archive",
        "--staging",
        "--commit",
        "--expected-archive-sha256",
        "--expected-tag",
        "--expected-commit",
        "--expected-source-sha256",
        "--expected-identity-sha256",
        "--expected-manifest-sha256",
    }
    mode = ""
    parsed: dict[str, str] = {}
    index = 0
    while index < len(argv):
        argument = argv[index]
        if argument in flags:
            if mode or argument == "--extract" and "--commit-artifact" in parsed:
                _fail("invalid_invocation")
            mode = argument
            index += 1
            continue
        if argument not in values or argument in parsed or index + 1 >= len(argv):
            _fail("invalid_invocation")
        value = argv[index + 1]
        if not value or value.startswith("--"):
            _fail("invalid_invocation")
        parsed[argument] = value
        index += 2
    if not mode:
        _fail("invalid_invocation")
    return mode, parsed


def _main(argv: list[str]) -> int:
    if os.geteuid() != 0:
        print("hermes_core_artifact_extraction=blocked", file=sys.stderr)
        print("hermes_core_artifact_error=root_required", file=sys.stderr)
        return 1
    try:
        mode, values = _parse_arguments(argv)
        if mode == "--extract":
            required = {
                "--archive",
                "--staging",
                "--expected-archive-sha256",
                "--expected-tag",
                "--expected-commit",
                "--expected-source-sha256",
                "--expected-identity-sha256",
                "--expected-manifest-sha256",
            }
            if set(values) != required:
                _fail("invalid_invocation")
            expected = {
                "archive_sha256": values["--expected-archive-sha256"],
                "tag": values["--expected-tag"],
                "commit": values["--expected-commit"],
                "source_sha256": values["--expected-source-sha256"],
                "identity_sha256": values["--expected-identity-sha256"],
                "manifest_sha256": values["--expected-manifest-sha256"],
            }
            extract_archive(values["--archive"], values["--staging"], expected)
            print("hermes_core_artifact_extraction=ready")
            print(f"hermes_core_artifact_commit={expected['commit']}")
            return 0

        if set(values) != {"--staging", "--commit"}:
            _fail("invalid_invocation")
        commit_artifact(values["--staging"], values["--commit"])
        print("hermes_core_artifact_commit=ready")
        print(f"hermes_core_artifact_commit_sha={values['--commit']}")
        return 0
    except ArtifactError as error:
        print("hermes_core_artifact=blocked", file=sys.stderr)
        print(f"hermes_core_artifact_error={error.code}", file=sys.stderr)
        return 2 if error.code == "invalid_invocation" else 1
    except Exception:
        print("hermes_core_artifact=blocked", file=sys.stderr)
        print("hermes_core_artifact_error=artifact_boundary_failed", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(_main(sys.argv[1:]))
