"""Private host credentials, outside skill mounts and environment passthrough.

This is a file input boundary, not proof of process/tool isolation.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import stat


KEYS = frozenset({"GREENPMS_API_TOKEN", "QINTOPIA_FOUNDATION_TOKEN",
                  "QINTOPIA_FOUNDATION_HOST_TOKEN"})
MAX_BYTES = 16384


def _object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("private_credentials_required")
        result[key] = value
    return result


def load(path: str, *, profile_home: str, environ=None) -> dict[str, str]:
    """Read a bounded owner-only file through pinned, non-symlink directory FDs.

    Never reflect filesystem errors, file contents or credential values to callers.
    Parent directories may belong to root or this user, but must not be writable
    by other users. Runtime files are not written, repaired, cached or exported.
    """
    env = os.environ if environ is None else environ
    fd = None
    try:
        candidate = Path(path)
        home = Path(profile_home)
        if (not candidate.is_absolute() or not home.is_absolute()
                or ".." in candidate.parts or any(key in env for key in KEYS)):
            raise ValueError
        # The opened path below cannot contain symlinks; resolve the Profile too
        # so a symlinked Profile cannot disguise a skill-mountable secret file.
        if candidate.is_relative_to(home.resolve()):
            raise ValueError
        fd = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
        for index, component in enumerate(candidate.parts[1:]):
            final = index == len(candidate.parts) - 2
            flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
            if not final:
                flags |= os.O_DIRECTORY
            child = os.open(component, flags, dir_fd=fd)
            os.close(fd)
            fd = child
            info = os.fstat(fd)
            if final:
                if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid()
                        or stat.S_IMODE(info.st_mode) != 0o600 or info.st_nlink != 1
                        or info.st_size > MAX_BYTES):
                    raise ValueError
            elif info.st_uid not in {0, os.getuid()} or info.st_mode & 0o022:
                raise ValueError
        raw = os.read(fd, MAX_BYTES + 1)
        if len(raw) > MAX_BYTES:
            raise ValueError
        value = json.loads(raw, object_pairs_hook=_object)
        if not isinstance(value, dict) or set(value) != KEYS:
            raise ValueError
        if any(not isinstance(token, str) or not 16 <= len(token) <= 512
               or not token.isascii() or any(c.isspace() or ord(c) < 33 or ord(c) == 127 for c in token)
               for token in value.values()):
            raise ValueError
        if any(not 32 <= len(value[key]) <= 256 for key in KEYS if key != "GREENPMS_API_TOKEN"):
            raise ValueError
        if len(set(value.values())) != len(KEYS):
            raise ValueError
        return value
    except (OSError, ValueError, TypeError, RecursionError):
        raise ValueError("private_credentials_required") from None
    finally:
        if fd is not None:
            os.close(fd)
