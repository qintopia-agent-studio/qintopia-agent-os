#!/usr/bin/env bash
set -euo pipefail
umask 077
[[ $# -eq 1 ]] || exit 2
mode="$1"
case "$mode" in prepare|activate|quiesce|verify-closed) ;; *) exit 2;; esac
[[ "$(id -u)" -eq 0 ]] || exit 75
trap 'exit 75' ERR
state=/var/lib/qintopia-agent-os-deploy
release_root=/home/ubuntu/qintopia-agent-os-releases
script_path="$(readlink -f -- "${BASH_SOURCE[0]}")"
[[ "$script_path" =~ ^/home/ubuntu/qintopia-agent-os-releases/([0-9a-f]{40})/deploy/runner/foundation-broker-lifecycle\.sh$ ]] || exit 75
release="$release_root/${BASH_REMATCH[1]}"
check_inherited_lock() {
  python3 - "$state/deploy.lock" <<'PY'
import fcntl
import os
import stat
import sys

lock_path = sys.argv[1]
path = os.lstat(lock_path)
held = os.fstat(9)
if (not stat.S_ISREG(path.st_mode) or not stat.S_ISREG(held.st_mode) or
        path.st_uid != 0 or path.st_nlink != 1 or
        (path.st_dev, path.st_ino) != (held.st_dev, held.st_ino)):
    raise SystemExit("inherited deploy lock identity changed")
probe = os.open(lock_path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
try:
    independent = os.fstat(probe)
    if (independent.st_dev, independent.st_ino) != (held.st_dev, held.st_ino):
        raise SystemExit("deploy lock path changed during verification")
    try:
        fcntl.flock(probe, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        pass
    else:
        fcntl.flock(probe, fcntl.LOCK_UN)
        raise SystemExit("deploy lock was not held before the helper")
finally:
    os.close(probe)
try:
    fcntl.flock(9, fcntl.LOCK_EX | fcntl.LOCK_NB)
except BlockingIOError:
    raise SystemExit("deploy lock FD 9 is not the inherited owner") from None
PY
}

check_maintenance_hold() {
  python3 - "$state" <<'PY'
import json
import os
import re
import stat
import sys
from pathlib import Path

state = Path(sys.argv[1])
hold = state / "recovery/hold"
info = hold.lstat()
if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or
        info.st_nlink != 1 or stat.S_IMODE(info.st_mode) != 0o600):
    raise SystemExit("management UI hold metadata is invalid")
token = hold.read_text(encoding="ascii").strip()
takeover = state / "recovery/takeover.json"
request_id = ""
if re.fullmatch(r"[0-9a-f]{32}", token) and takeover.is_file():
    record = json.loads(takeover.read_text())
    if record.get("hold_token") == token:
        request_id = record.get("request_id", "")
elif re.fullmatch(r"deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}", token):
    journal = state / "recovery" / (token + ".json")
    if journal.is_file() and json.loads(journal.read_text()).get("request_id") == token:
        request_id = token
if not re.fullmatch(r"deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}", request_id):
    raise SystemExit("management UI hold is not request-bound")
for claim in (state / "requests/claimed").glob("*.json"):
    if claim.stem != request_id:
        raise SystemExit("another deploy request claim is present")
PY
}

if [[ "$mode" == quiesce || "$mode" == verify-closed ]]; then
  check_inherited_lock || exit 75
else
  exec 8>"$state/poller.lock"
  flock -n 8 || exit 75
  exec 9>"$state/deploy.lock"
  flock -n 9 || exit 75
  check_inherited_lock || exit 75
  [[ "$(readlink -f "$release_root/current")" == "$release" ]] || exit 75
  if [[ "$mode" == activate ]]; then
    [[ ! -e "$state/recovery/hold" && ! -L "$state/recovery/hold" ]] || exit 75
    if compgen -G "$state/requests/claimed/*.json" >/dev/null; then exit 75; fi
  else
    check_maintenance_hold || exit 75
  fi
fi
python3 - "$mode" "$release" 3<&0 <<'PY'
import fcntl
import grp
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import stat
import subprocess
import sys
import time
from urllib.parse import unquote, urlsplit

mode, release_text = sys.argv[1:]
release = Path(release_text)
root = release.parent
unit = 'qintopia-agentos-foundation-broker.service'
account = 'qintopia-foundation-broker'
env_file = Path('/etc/qintopia/foundation-broker.env')
runtime = Path('/run/qintopia-foundation-erhua')
socket = runtime / 'broker.sock'
unit_file = Path('/etc/systemd/system') / unit
clock_deadline = None

def remaining():
    if clock_deadline is None:
        return 10
    seconds = clock_deadline - time.monotonic()
    if seconds <= 0:
        raise RuntimeError('stop deadline reached')
    return min(seconds, 10)

def command(args):
    return subprocess.run(args, capture_output=True, text=True, check=True, timeout=remaining()).stdout

def metadata(path, permissions, owner=0):
    item = path.lstat()
    if (not stat.S_ISREG(item.st_mode) or item.st_uid != owner or item.st_gid != 0 or
            item.st_nlink != 1 or stat.S_IMODE(item.st_mode) != permissions):
        raise RuntimeError('immutable/private metadata drift')
    return item

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def verified_binary(directory):
    if directory.parent != root or not re.fullmatch('[0-9a-f]{40}', directory.name):
        raise RuntimeError('binary release is not immutable')
    for name in ('manifest.json', 'sidecar/artifact-manifest.json', 'deploy-bundle/artifact-manifest.json'):
        metadata(directory / name, 0o444)
    manifest = json.loads((directory / 'manifest.json').read_text())
    artifact = json.loads((directory / 'sidecar/artifact-manifest.json').read_text())
    if manifest.get('release_sha') != directory.name or artifact.get('commit_sha') != manifest.get('runtime_sha'):
        raise RuntimeError('runtime identity mismatch')
    binary = directory / 'sidecar/qintopia-message-sidecar'
    metadata(binary, 0o755)
    entries = [item for item in artifact.get('files', []) if item.get('path') == 'qintopia-message-sidecar']
    if len(entries) != 1 or sha(binary) != entries[0].get('sha256'):
        raise RuntimeError('runtime digest mismatch')
    return binary

def verify_release():
    binary = verified_binary(release)
    manifest = json.loads((release / 'manifest.json').read_text())
    artifact = json.loads((release / 'deploy-bundle/artifact-manifest.json').read_text())
    if artifact.get('commit_sha') != manifest.get('deploy_bundle_sha'):
        raise RuntimeError('deploy bundle identity mismatch')
    for relative in ('deploy/runner/foundation-broker-lifecycle.sh','deploy/sidecar/scripts/render-systemd-units.sh'):
        file = release / relative
        metadata(file,0o755)
        entries = [item for item in artifact.get('files',[]) if item.get('path')=='payload/'+relative]
        if len(entries)!=1 or sha(file)!=entries[0].get('sha256'):
            raise RuntimeError('helper/renderer digest mismatch')
    return binary

def account_identity(required=False):
    try:
        user = pwd.getpwnam(account)
    except KeyError:
        if required:
            raise RuntimeError('broker account absent')
        return None
    runner = pwd.getpwnam('ubuntu')
    group = grp.getgrnam('ubuntu')
    if (runner.pw_uid <= 0 or runner.pw_gid != group.gr_gid or user.pw_uid <= 0 or
            user.pw_uid == runner.pw_uid or user.pw_gid != runner.pw_gid or
            user.pw_dir != '/nonexistent' or user.pw_shell != '/usr/sbin/nologin' or
            any(user.pw_name in g.gr_mem for g in grp.getgrall())):
        raise RuntimeError('broker/runner account identity drift')
    return user

def process_identity(pid):
    path = Path('/proc') / str(pid)
    try:
        raw = (path / 'stat').read_text()
        start = raw[raw.rfind(')') + 2:].split()[19]
        try:
            executable = os.readlink(path / 'exe')
            identity = (path / 'exe').stat()
            return [str(pid), start, str(identity.st_dev), str(identity.st_ino), executable]
        except FileNotFoundError:
            return [str(pid), start, None, None, None]
    except FileNotFoundError:
        return None

def snapshot():
    keys = ('LoadState','ActiveState','SubState','UnitFileState','InvocationID','Result',
            'ExecMainCode','ExecMainStatus','MainPID','ControlPID','ControlGroup','NRestarts')
    text = command(['/usr/bin/systemctl','show',unit]+['--property='+k for k in keys])
    data = dict(line.split('=',1) for line in text.splitlines() if '=' in line)
    if any(key not in data for key in keys) or data['LoadState'] not in ('loaded','not-found'):
        raise RuntimeError('unit state unknown')
    data['ProcessIdentity'] = process_identity(int(data['MainPID'])) if int(data['MainPID']) else None
    if data['ProcessIdentity']:
        user = account_identity(True)
        process = Path('/proc')/data['MainPID']
        status = dict(line.split(':',1) for line in (process/'status').read_text().splitlines() if ':' in line)
        if process.stat().st_uid!=user.pw_uid or any(int(g)!=user.pw_gid for g in status['Gid'].split()):
            raise RuntimeError('running broker UID/GID drift')
        executable = Path(data['ProcessIdentity'][-1])
        if executable != verified_binary(executable.parent.parent):
            raise RuntimeError('running executable identity drift')
        args = (process/'cmdline').read_bytes().split(b'\0')
        if [a for a in args if a] != [str(executable).encode(),b'run-foundation-production']:
            raise RuntimeError('running broker command drift')
        expected_group = '/system.slice/'+unit
        if data['ControlGroup']!=expected_group or ('0::'+expected_group) not in (process/'cgroup').read_text().splitlines():
            raise RuntimeError('running broker cgroup ownership drift')
        item = Path('/sys/fs/cgroup'+expected_group).stat()
        data['OriginalCgroupIdentity'] = [item.st_dev,item.st_ino]
    return data

def empty_cgroup(group, identity=None):
    if not group:
        return
    if not group.startswith('/') or '..' in Path(group).parts:
        raise RuntimeError('invalid cgroup')
    path = Path('/sys/fs/cgroup'+group)
    if path.exists():
        item = path.stat()
        if identity and identity != [item.st_dev,item.st_ino]:
            raise RuntimeError('original cgroup identity changed')
        if 'populated 0' not in (path/'cgroup.events').read_text().splitlines():
            raise RuntimeError('original/current cgroup still populated')
        for child in [path]+list(path.rglob('*')):
            if child.is_dir() and (child/'cgroup.procs').read_text().strip():
                raise RuntimeError('cgroup tasks remain')

def successful_invocation(data, before):
    invocation = data['InvocationID']
    journal_success = False
    if not invocation:
        # Daemon-reload warnings also have UNIT but no lifecycle job. Select the
        # latest start/stop job, so a newer start/failure cannot reuse an old stop.
        text = command(['/usr/bin/journalctl','--no-pager','-n','1','-o','json',
                        'UNIT='+unit,'JOB_TYPE=start','JOB_TYPE=stop'])
        if text.strip():
            event = json.loads(text.strip().splitlines()[-1])
            if (event.get('JOB_TYPE') != 'stop' or event.get('JOB_RESULT') != 'done' or
                    event.get('MESSAGE_ID') != '9d1aaa27d60140bd96365438aad20286'):
                raise RuntimeError('final journal event not successful stop')
            invocation = event.get('INVOCATION_ID','')
            if not re.fullmatch('[0-9a-f]{32}',invocation):
                raise RuntimeError('stop invocation missing')
            text = command(['/usr/bin/journalctl','--no-pager','-n','1','-o','json','UNIT='+unit,
                            'INVOCATION_ID='+invocation,'MESSAGE_ID=7ad2d189f7e94e70a38c781354912448'])
            if not text.strip():
                raise RuntimeError('successful deactivation missing')
            event = json.loads(text.strip().splitlines()[-1])
            journal_success = event.get('INVOCATION_ID') == invocation and event.get('UNIT') == unit
    if before and before['InvocationID'] and invocation != before['InvocationID']:
        raise RuntimeError('stop invocation changed')
    if invocation:
        if not re.fullmatch('[0-9a-f]{32}',invocation):
            raise RuntimeError('invocation identity invalid')
        if not journal_success and data['ExecMainCode'] not in ('1','exited'):
            raise RuntimeError('exit not definite')
        text = command(['/usr/bin/journalctl','--no-pager','-o','cat','_SYSTEMD_INVOCATION_ID='+invocation])
        if any(word in text for word in ('outcome_unknown','configuration_commit_outcome_unknown')):
            raise RuntimeError('business outcome unknown')
    elif before and before['ProcessIdentity']:
        raise RuntimeError('missing stopped invocation')

def verify_closed(before=None):
    data = snapshot()
    if data['NRestarts'] != '0' or data['ActiveState'] != 'inactive' or data['MainPID'] != '0' or data['ControlPID'] != '0':
        raise RuntimeError('unit not closed')
    if data['LoadState'] == 'loaded':
        if data['UnitFileState'] != 'disabled' or data['Result'] != 'success' or data['ExecMainStatus'] != '0':
            raise RuntimeError('unit exit abnormal')
        successful_invocation(data,before)
    if before:
        if data['NRestarts'] != before['NRestarts']:
            raise RuntimeError('restart history changed')
        identity = before['ProcessIdentity']
        if identity and (current_identity := process_identity(int(identity[0]))) and current_identity[:2] == identity[:2]:
            raise RuntimeError('original process remains')
        empty_cgroup(before['ControlGroup'],before.get('OriginalCgroupIdentity'))
    empty_cgroup(data['ControlGroup'])
    if socket.exists() or socket.is_symlink():
        raise RuntimeError('residual socket remains')
    user = account_identity()
    if runtime.exists() or runtime.is_symlink():
        item = runtime.lstat()
        if not user or not stat.S_ISDIR(item.st_mode) or item.st_uid != user.pw_uid or item.st_gid != user.pw_gid or stat.S_IMODE(item.st_mode) != 0o750:
            raise RuntimeError('runtime directory metadata drift')
        if any(stat.S_ISSOCK(p.lstat().st_mode) or p.is_symlink() for p in runtime.iterdir()):
            raise RuntimeError('runtime socket or symlink remains')
    for path in Path('/proc').iterdir():
        if not path.name.isdecimal():
            continue
        try:
            args = (path/'cmdline').read_bytes().split(b'\0')
            if b'run-foundation-production' not in args:
                continue
            relevant = user and path.stat().st_uid == user.pw_uid
            if not relevant:
                environment = (path/'environ').read_bytes().split(b'\0')
                relevant = (b'QINTOPIA_FOUNDATION_PROFILE=erhua' in environment or
                            b'QINTOPIA_FOUNDATION_SOCKET='+str(socket).encode() in environment)
            if relevant:
                raise RuntimeError('broker executable process remains')
        except (FileNotFoundError, ProcessLookupError):
            continue
    return data

def read_env(path):
    metadata(path,0o600)
    raw = path.read_bytes()
    if not raw or len(raw)>8192 or not raw.endswith(b'\n') or b'\r' in raw:
        raise RuntimeError('private env format invalid')
    expected = {'QINTOPIA_FOUNDATION_'+key for key in ('PRODUCTION_ENABLE','PROFILE','TENANT','IDENTITY_NAMESPACE',
               'DATABASE_URL','DATABASE_URL_SHA256','TOKEN_SHA256','RUNNER_UID','RUNNER_GID','SOCKET','GATEWAY_ID','ERHUA_APPROVAL')}
    values = {}
    for line in raw.decode('ascii').splitlines():
        if not re.fullmatch('[A-Z][A-Z0-9_]*=[!-~]+',line):
            raise RuntimeError('private env syntax invalid')
        key,value = line.split('=',1)
        if key not in expected or key in values or any(c in value for c in "'\"\\`$#;"):
            raise RuntimeError('private env keys invalid')
        values[key]=value
    if set(values)!=expected:
        raise RuntimeError('private env incomplete')
    def v(key): return values['QINTOPIA_FOUNDATION_'+key]
    runner = pwd.getpwnam('ubuntu')
    if (v('PRODUCTION_ENABLE')!='1' or v('PROFILE')!='erhua' or v('SOCKET')!=str(socket) or
            v('ERHUA_APPROVAL')!='steward-foundation-reviewed' or
            v('RUNNER_UID')!=str(runner.pw_uid) or v('RUNNER_GID')!=str(runner.pw_gid) or
            not re.fullmatch('[A-Za-z0-9_-]{1,128}',v('TENANT')) or
            not re.fullmatch('[A-Za-z0-9_.:-]{1,128}',v('IDENTITY_NAMESPACE')) or
            not re.fullmatch('[A-Za-z0-9_.:-]{1,128}',v('GATEWAY_ID')) or
            not re.fullmatch('[0-9a-f]{64}',v('TOKEN_SHA256')) or
            v('DATABASE_URL_SHA256')!=hashlib.sha256(v('DATABASE_URL').encode()).hexdigest()):
        raise RuntimeError('private broker contract invalid')
    database = urlsplit(v('DATABASE_URL'))
    role = unquote(database.username or '')
    if (database.scheme not in ('postgres','postgresql') or not database.hostname or not database.password or
            not database.path.strip('/') or 'test' in database.path.lower() or database.fragment or
            not re.fullmatch('qintopia_[a-z0-9_]+',role) or role=='qintopia_management_ui'):
        raise RuntimeError('broker database identity invalid')
    return values

try:
    binary = verify_release()
    if mode == 'prepare':
        verify_closed()
        if env_file.exists() or env_file.is_symlink():
            raise RuntimeError('env exists; uncertain writes/rotation require inspection')
        parent = env_file.parent.lstat()
        if not stat.S_ISDIR(parent.st_mode) or parent.st_uid!=0 or parent.st_mode & 0o022:
            raise RuntimeError('private configuration parent drift')
        raw = os.read(3,8193)
        if len(raw)>8192 or os.read(3,1):
            raise RuntimeError('env input too large')
        import tempfile
        fd,name = tempfile.mkstemp(prefix='.foundation-broker.',dir=env_file.parent)
        temporary = Path(name)
        try:
            with os.fdopen(fd,'wb') as file:
                file.write(raw)
                file.flush()
                os.fsync(file.fileno())
            values = read_env(temporary)
            if account_identity() is None:
                command(['/usr/sbin/useradd','--system','--gid','ubuntu','--no-create-home',
                         '--home-dir','/nonexistent','--shell','/usr/sbin/nologin',account])
            account_identity(True)
            os.link(temporary,env_file,follow_symlinks=False)
        finally:
            temporary.unlink()
        read_env(env_file)
    elif mode == 'activate':
        user = account_identity(True)
        read_env(env_file)
        verify_closed()
        metadata(unit_file,0o644)
        source = (release/'deploy/sidecar/scripts/render-systemd-units.sh').read_text()
        templates = re.findall(r'(?m)^  write_file "qintopia-agentos-foundation-broker\.service" <<EOF\n(.*?)\nEOF\n',source,re.S)
        if len(templates)!=1:
            raise RuntimeError('immutable broker template ambiguous')
        expected = templates[0].replace('${BIN}',str(binary))+'\n'
        if '${' in expected or unit_file.read_text()!=expected:
            raise RuntimeError('complete unit contract drift')
        text = command(['/usr/bin/systemctl','show',unit,'--property=DropInPaths,FragmentPath,NeedDaemonReload,User,Group'])
        properties = dict(line.split('=',1) for line in text.splitlines() if '=' in line)
        if (properties.get('DropInPaths')!='' or properties.get('FragmentPath')!=str(unit_file) or
                properties.get('NeedDaemonReload')!='no' or properties.get('User')!=account or properties.get('Group')!='ubuntu'):
            raise RuntimeError('loaded broker unit is not the immutable expected configuration')
        command(['/usr/bin/systemctl','enable','--now',unit])
        ready_deadline = time.monotonic()+10
        while not socket.exists() and time.monotonic()<ready_deadline:
            time.sleep(.1)
        item = socket.lstat()
        parent = runtime.lstat()
        if (not stat.S_ISSOCK(item.st_mode) or item.st_uid!=user.pw_uid or item.st_gid!=user.pw_gid or
                stat.S_IMODE(item.st_mode)!=0o660 or not stat.S_ISDIR(parent.st_mode) or
                parent.st_uid!=user.pw_uid or parent.st_gid!=user.pw_gid or stat.S_IMODE(parent.st_mode)!=0o750):
            raise RuntimeError('broker socket identity not ready')
        data = snapshot()
        if data['ActiveState']!='active' or data['UnitFileState']!='enabled' or not data['ProcessIdentity'] or data['ProcessIdentity'][-1]!=str(binary):
            raise RuntimeError('activation not ready')
    elif mode == 'quiesce':
        before = snapshot()
        if before['ActiveState'] not in ('active','inactive'):
            raise RuntimeError('existing stop or failure is unresolved; do not resubmit stop')
        if before['LoadState']=='loaded':
            command(['/usr/bin/systemctl','disable',unit])
            if before['ActiveState']!='inactive' or before['MainPID']!='0' or before['ControlPID']!='0':
                clock_deadline = time.monotonic()+35
                client = subprocess.Popen(['/usr/bin/systemctl','stop',unit],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
                try:
                    client.wait(timeout=max(0,clock_deadline-time.monotonic()))
                    if client.returncode != 0:
                        raise RuntimeError('stop client failed')
                except subprocess.TimeoutExpired:
                    client.terminate()  # Only the waiting client, never the business process.
                    try:
                        client.wait(timeout=.05)
                    except subprocess.TimeoutExpired:
                        client.kill()  # Still only the systemctl waiting client.
                    print('foundation_broker_stop=deferred deadline_seconds=35 outcome=unknown',file=sys.stderr)
                    raise RuntimeError('35-second stop deferred')
        verify_closed(before)
        remaining()
    else:
        verify_closed()
    print('foundation_broker_lifecycle='+mode+' success=true')
except Exception:
    print('foundation broker lifecycle deferred or identity/closure unknown',file=sys.stderr)
    sys.exit(75)
PY
