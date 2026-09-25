"""Opt-in Linux filesystem simulation for the private managed policy path.

Run only in a disposable, network-disabled Linux container as root. This checks
real ownership, symlinks and modes; it does not install a production Profile.
"""
import importlib.util
import json
import os
from pathlib import Path
import sys


assert os.environ.get("ANAN_POLICY_SIMULATE") == "1", "explicit_simulation_required"
assert sys.platform == "linux" and os.getuid() == 0, "disposable_linux_root_required"
root = Path("/anan-managed-policy-simulation")
assert not root.exists(), "fresh_simulation_directory_required"
root.mkdir(mode=0o755)
directory = root / "managed"
directory.mkdir(mode=0o750)
config = directory / "config.yaml"
config.write_text("skills:\n  inline_shell: false\n")
config.chmod(0o640)
spec = importlib.util.spec_from_file_location("pms_policy", Path(__file__).parents[1] / "production.py")
production = importlib.util.module_from_spec(spec)
spec.loader.exec_module(production)
production.check_managed_files(directory)
denied = []


def rejects(name, path=directory):
    try:
        production.check_managed_files(path)
    except ValueError as error:
        assert str(error) == production.ERROR
        denied.append(name)
    else:
        raise AssertionError("unsafe_policy_accepted: " + name)


root.chmod(0o777)
rejects("writable_parent")
root.chmod(0o755)
directory.chmod(0o755)
rejects("public_policy_directory")
directory.chmod(0o750)
config.chmod(0o644)
rejects("public_policy_file")
config.chmod(0o660)
rejects("group_writable_policy")
config.chmod(0o640)
link = root / "linked-managed"
link.symlink_to(directory, target_is_directory=True)
rejects("directory_symlink", link)
saved = directory / "saved.yaml"
config.rename(saved)
config.symlink_to(saved)
rejects("file_symlink")
config.unlink()
saved.rename(config)
hardlink = directory / "second.yaml"
os.link(config, hardlink)
rejects("hardlinked_policy")
hardlink.unlink()
config.rename(saved)
config.mkdir()
rejects("non_regular_policy")
config.rmdir()
saved.rename(config)
production.check_managed_files(directory)
print(json.dumps({"environment": "disposable Linux filesystem simulation",
                  "valid_private_root_policy": True, "denied": denied,
                  "production_acceptance": False}))
