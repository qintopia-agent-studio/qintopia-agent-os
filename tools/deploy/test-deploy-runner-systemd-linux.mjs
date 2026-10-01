#!/usr/bin/env node

import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
const cosMockServerCode = String.raw`import hashlib, hmac, http.server, json, pathlib, sys, urllib.parse
config_path, port_path = map(pathlib.Path, sys.argv[1:3])
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        config = json.loads(config_path.read_text())
        key = config["key"]
        auth = urllib.parse.parse_qs(self.headers.get("Authorization", ""))
        times = auth.get("q-key-time", [""])[0]
        header_value = "host=" + urllib.parse.quote(self.headers.get("Host", ""), safe="~-._")
        http_string = "get\n" + self.path + "\n\n" + header_value + "\n"
        sign_text = "sha1\n" + times + "\n" + hashlib.sha1(http_string.encode()).hexdigest() + "\n"
        sign_key = hmac.new(b"simulated", times.encode(), hashlib.sha1).hexdigest()
        expected = hmac.new(sign_key.encode(), sign_text.encode(), hashlib.sha1).hexdigest()
        valid = (self.path == "/" + key and auth.get("q-signature") == [expected] and
                 auth.get("q-header-list") == ["host"] and auth.get("q-sign-time") == [times])
        mode = config["mode"] if valid else "unreadable"
        if mode == "absent":
            payload = ("<Error><Code>NoSuchKey</Code><Key>" + key + "</Key></Error>").encode()
            code = 404
        elif mode == "other-404":
            payload, code = b"<Error><Code>NoSuchBucket</Code></Error>", 404
        elif mode == "unreadable":
            payload, code = b"<Error><Code>AccessDenied</Code></Error>", 403
        else:
            payload, code = pathlib.Path(config["result"]).read_bytes(), 200
        self.send_response(code)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)
    def log_message(self, *args): pass
server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
port_path.write_text(str(server.server_port))
server.serve_forever()
`;
const managementUiLockProbe = `#!/usr/bin/env bash
set -euo pipefail
[[ "$#" == 1 && ( "$1" == quiesce || "$1" == verify-closed ) ]]
python3 - <<'PY'
import fcntl, os, stat
path = os.lstat('/var/lib/qintopia-agent-os-deploy/deploy.lock')
held = os.fstat(9)
assert stat.S_ISREG(path.st_mode) and (path.st_dev, path.st_ino) == (held.st_dev, held.st_ino)
fcntl.flock(9, fcntl.LOCK_EX | fcntl.LOCK_NB)
PY
`;

// Optional additive C1 cases in the existing real-systemd producer fixture.
// Requires a deliberately prepared disposable Lima/PG lab; no CI selector,
// timeout, shared framework, production configuration or live tool is changed.
if (
  process.argv[2] === "--management-ui-maintenance-systemd-producer" &&
  process.env.QINTOPIA_FOUNDATION_LAB_VM
) {
  assert.ok(process.env.QINTOPIA_FOUNDATION_LAB_PG_CONTAINER);
  const result = spawnSync(
    "python3",
    [
      "-c",
      String.raw`# This test uses an explicitly prepared disposable Linux/systemd/PG fixture.
# Never run against production. It preserves failed units and uncertain outcomes.
import datetime,hashlib,hmac,json,pathlib,subprocess,sys,time
repo=pathlib.Path(sys.argv[1]);vm=sys.argv[2];pg=sys.argv[3]
R='1111111111111111111111111111111111111111';T='2222222222222222222222222222222222222222'
root='/home/ubuntu/qintopia-agent-os-releases';release=root+'/'+R
state='/var/lib/qintopia-agent-os-deploy';unit='qintopia-agentos-foundation-broker.service'
helper=release+'/deploy/runner/foundation-broker-lifecycle.sh';ui=release+'/deploy/runner/management-ui-lifecycle.sh'
lock='exec 9>'+state+'/deploy.lock; flock -n 9; '
results=[]
def call(script,raw=None,expected=0):
 p=subprocess.run(['limactl','shell',vm,'--','sudo','bash','-c','set -euo pipefail; '+script],input=raw,capture_output=True)
 if expected is not None and p.returncode!=expected:
  raise AssertionError('Linux fixture exit '+str(p.returncode)+' expected '+str(expected)+'\n'+p.stdout.decode()+'\n'+p.stderr.decode())
 return p
def output(script):return call(script).stdout.decode().strip()
def case(name,**values):
 record={'case':name,**values};results.append(record);print(json.dumps(record),flush=True)
def pointer_state():return output('for name in current previous rollback-from; do printf "%s=" "$name"; readlink '+root+'/$name || true; done')
def state_data():
 text=output('systemctl show '+unit+' -p MainPID -p InvocationID -p ActiveState -p SubState -p Result -p ControlGroup -p UnitFileState')
 d=dict(l.split('=',1) for l in text.splitlines() if '=' in l)
 if d['MainPID']!='0':
  pid=d['MainPID'];d['starttime']=output("python3 -c \"s=open('/proc/"+pid+"/stat').read();print(s[s.rfind(')')+2:].split()[19])\"")
  d['cgroup']=output('cat /proc/'+pid+'/cgroup');d['proc_uid_gid']=output('stat -c "%u:%g" /proc/'+pid)
 return d
# Explicit prepared fixture; never infer/create credentials or overwrite an env.
call('test "$(readlink -f '+root+'/current)" = '+release+'; test -f /etc/qintopia/foundation-broker.env; test ! -e '+state+'/recovery/hold; test "$(systemctl show '+unit+' -p ActiveState --value)" = inactive')
identity={'simulated_release':R,'runtime_source':'cc7cecd5afaa00037b970b45b002526642d368e2','architecture':output('uname -m'),'systemd':output('systemctl --version | head -1'),'formal_release':False,'production_action':False,'files':{}}
for name in ['deploy/runner/foundation-broker-lifecycle.sh','deploy/runner/management-ui-lifecycle.sh','deploy/runner/rollback-release.sh','deploy/runner/install-release-systemd-units.sh','deploy/sidecar/scripts/render-systemd-units.sh']:
 local=hashlib.sha256((repo/name).read_bytes()).hexdigest();actual=output('sha256sum '+release+'/'+name).split()[0];assert local==actual
 identity['files'][name]=actual
for name in ['sidecar/qintopia-message-sidecar','actual-sdk.py','client-probe.py']:
 identity['files'][name]=output('sha256sum '+release+'/'+name).split()[0]
case('source_to_linux_fixture_identity',**identity)
# Source main runner/recovery is copied unchanged; only artifacts beyond the failed
# closure stage are plumbing probes. No real COS call, business send or replays.
for name in ['deploy/runner/qintopia-agent-os-deploy-runner','deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf']:
 call('mkdir -p '+str(pathlib.PurePosixPath(release+'/'+name).parent)+'; cat > '+release+'/'+name+'; chmod 0755 '+release+'/'+name,(repo/name).read_bytes())
call('mkdir -p '+root+'/'+T+'/deploy/runner/qintopia-agent-os-deploy-runner.service.d; cp '+release+'/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf '+root+'/'+T+'/deploy/runner/qintopia-agent-os-deploy-runner.service.d/; ln -s '+root+'/'+T+' '+root+'/previous')
call('cat > '+root+'/'+T+'/manifest.json; chmod 0444 '+root+'/'+T+'/manifest.json',json.dumps({'release_sha':T}).encode())
old_installer='#!/usr/bin/env bash\nset -euo pipefail\nunit_files=(\n  qintopia-agentos-foundation-broker.service\n)\nrunner_unit_files=(\n  qintopia-agent-os-deploy-runner.service\n  qintopia-agent-os-deploy-runner.timer\n)\nprintf "old T installer entered\\n" >> /tmp/qintopia-c1-install-events\nexit 55\n'
call('cat > '+root+'/'+T+'/deploy/runner/install-release-systemd-units.sh; chmod 0755 '+root+'/'+T+'/deploy/runner/install-release-systemd-units.sh',old_installer.encode())
call('cat > '+release+'/deploy/runner/promote-release.sh; chmod 0755 '+release+'/deploy/runner/promote-release.sh',b'#!/bin/bash\necho unexpected-promotion >> /tmp/qintopia-c1-install-events\nexit 96\n')
call('cp '+release+'/deploy/runner/promote-release.sh '+release+'/deploy/runner/quiesce-space-automation-runtime.sh; printf "#!/bin/bash\\nexit 0\\n" > '+release+'/deploy/runner/quiesce-space-automation-runtime.sh')
call('test ! -e /run/systemd/system/qintopia-agent-os-deploy-runner.timer; printf "[Unit]\\nDescription=Simulated C1 timer\\n[Timer]\\nOnCalendar=hourly\\n[Install]\\nWantedBy=timers.target\\n" > /run/systemd/system/qintopia-agent-os-deploy-runner.timer; systemctl daemon-reload; systemctl enable --now qintopia-agent-os-deploy-runner.timer; rm -f /tmp/qintopia-c1-install-events')
# Missing FD9 has a consistent deferred status; no stop is submitted.
p=call(helper+' verify-closed',expected=75);case('missing_fd9',exit=p.returncode)
# Complete unit comparison, not a line-presence assertion.
unit_file='/etc/systemd/system/'+unit
original=call('cat '+unit_file).stdout
for suffix in [b'\n[Service]\nUser=root\n',b'\n[Service]\nExecStartPre=/bin/true\n']:
 call('cat > '+unit_file+'; systemctl daemon-reload',original+suffix)
 p=call(helper+' activate',expected=75);assert state_data()['MainPID']=='0';case('complete_unit_drift_rejected',exit=p.returncode,extra_entry=suffix.decode().strip().splitlines()[-1])
call('cat > '+unit_file+'; systemctl daemon-reload',original)
call(helper+' activate');before=state_data();assert before['MainPID']!='0'
pointers=pointer_state();rollback=release+'/deploy/runner/rollback-release.sh --release-root '+root+' --expected-current-sha '+R+' --expected-previous-sha '+T
p=call(lock+rollback,expected=75);assert pointer_state()==pointers;assert state_data()['MainPID']==before['MainPID'];case('live_broker_blocks_first_rollback_pointer',exit=p.returncode,pointers_unchanged=True,broker_unstopped=True)
unit_digest=output('sha256sum '+unit_file).split()[0]
p=call(lock+release+'/deploy/runner/install-release-systemd-units.sh --release-root '+root+' --release-sha '+R,expected=75)
assert output('sha256sum '+unit_file).split()[0]==unit_digest;assert state_data()['MainPID']==before['MainPID'];case('live_broker_blocks_installer_unit_replacement',exit=p.returncode,unit_unchanged=True,broker_unstopped=True)
# Reject helper metadata and digest before executing it or writing a pointer.
for mode in ['metadata','digest']:
 original_ui=call('cat '+ui).stdout
 if mode=='metadata':call('chmod 0700 '+ui)
 else:call('printf "\\n# simulated digest drift\\n" >> '+ui)
 p=call(lock+rollback,expected=75);assert pointer_state()==pointers;assert state_data()['MainPID']==before['MainPID']
 call('cat > '+ui+'; chmod 0755 '+ui,original_ui);case('rollback_'+mode+'_drift',exit=p.returncode,pointers_unchanged=True,broker_unstopped=True)
# Closed R->old T does not look for a missing T helper. Its installer fails after
# the durable pointer phase; this is not claimed as a successful T installation.
call(lock+ui+' quiesce');p=call(lock+rollback,expected=55)
assert output('readlink -f '+root+'/current')==root+'/'+T
assert output('readlink -f '+root+'/previous')==release
assert output('readlink -f '+root+'/rollback-from')==release
assert output('cat /tmp/qintopia-c1-install-events')=='old T installer entered'
case('closed_fixed_R_to_old_T_installer_failure',exit=p.returncode,old_T_helper_absent=True,current=T,previous=R,rollback_from=R)
# Test-fixture restoration, never an automatic recovery/clearing of unknown hold.
call('ln -sfn '+release+' '+root+'/current; ln -sfn '+root+'/'+T+' '+root+'/previous; rm '+root+'/rollback-from; rm /tmp/qintopia-c1-install-events')
# The exact actual binary can run under another nonroot UID. Quiesce must refuse
# it before disable/stop (activation always rejects the modified unit).
wrong=original.replace(b'User=qintopia-foundation-broker',b'User=nobody')
call('cat > '+unit_file+'; systemctl daemon-reload; systemctl start '+unit,wrong)
for _ in range(100):
 if output('test -S /run/qintopia-foundation-erhua/broker.sock && echo ready || true')=='ready':break
 time.sleep(.1)
wrong_before=state_data();assert wrong_before['MainPID']!='0'
p=call(lock+helper+' quiesce',expected=75);assert state_data()['MainPID']==wrong_before['MainPID'];case('same_binary_wrong_uid_rejected_before_stop',exit=p.returncode,broker_unstopped=True,observed_uid_gid=wrong_before.get('proc_uid_gid'))
# Only the explicit local-fixture operator stops this idle identity-drift case.
call('systemctl stop '+unit+'; cat > '+unit_file+'; systemctl daemon-reload',original)
call(helper+' activate')
# Preserve the original identity while SQL is blocked beyond the drain deadlines.
request_id='deploy-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'-'+R[:7]
source_message='simulated-c1-inflight-'+request_id
probe=call('cat '+release+'/client-probe.py').stdout.decode().replace('simulated-install-lab-context-20261001',source_message)
call('cat > '+release+'/inflight-probe.py; chmod 0644 '+release+'/inflight-probe.py',probe.encode())
block=subprocess.Popen(['docker','exec',pg,'psql','-U','postgres','-d','qintopia_c1_lab','-v','ON_ERROR_STOP=1','-c','BEGIN; LOCK TABLE qintopia_identity.persons IN ACCESS EXCLUSIVE MODE; SELECT pg_sleep(53); COMMIT;'],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
try:
 for _ in range(100):
  p=subprocess.run(['docker','exec',pg,'psql','-U','postgres','-d','qintopia_c1_lab','-At','-c',"select count(*) from pg_locks where relation='qintopia_identity.persons'::regclass and mode='AccessExclusiveLock' and granted;"],capture_output=True,text=True,check=True)
  if p.stdout.strip()=='1':break
  time.sleep(.1)
 else:raise AssertionError('SQL blocker not ready')
 client=subprocess.Popen(['limactl','shell',vm,'--','sudo','setpriv','--reuid=1000','--regid=1001','--clear-groups','python3',release+'/inflight-probe.py'],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
 for _ in range(100):
  p=subprocess.run(['docker','exec',pg,'psql','-U','postgres','-d','qintopia_c1_lab','-At','-c',"select count(*) from pg_stat_activity where usename='qintopia_broker_lab' and wait_event_type='Lock';"],capture_output=True,text=True,check=True)
  if int(p.stdout.strip())>=1:break
  time.sleep(.1)
 else:raise AssertionError('actual broker SQL request not accepted')
 before=state_data();pointers=pointer_state()
 now=datetime.datetime.now(datetime.timezone.utc);stamp=now.isoformat();target='3'*40
 request={'schema_version':1,'request_id':request_id,'environment':'production','repository':'qintopia-agent-studio/qintopia-agent-os','requested_by':'simulated-C1-lab','created_at':stamp,'expires_at':(now+datetime.timedelta(hours=1)).isoformat(),'commit_sha':target,'runtime_sha':target,'runtime_artifact_profile':'huabaosi-production','deploy_bundle_sha':target,'release_sha':target,'release_scope':['sidecar-runtime','deploy-bundle','hermes-plugins'],'restart_targets':['qintopia-system-services'],'rollback_on_smoke_failure':True,'dry_run':False,'cos':{'bucket':'simulated','region':'simulated','prefix':'qintopia-agent-os','request_key':'qintopia-agent-os/deploy-requests/production/requests/'+request_id+'.json','result_key':'qintopia-agent-os/deploy-results/production/'+request_id+'.json'}}
 metadata={'algorithm':'hmac-sha256','issuer':'github-actions','key_id':'simulated','signed_at':stamp}
 canonical=json.dumps({'request':request,'signature':metadata},sort_keys=True,separators=(',',':'),ensure_ascii=False)
 request['signature']={**metadata,'value':hmac.new(b'simulated-key',canonical.encode(),hashlib.sha256).hexdigest()}
 request_path=state+'/requests/processed/'+request_id+'.json'
 call('mkdir -p '+state+'/requests/processed; cat > '+request_path,json.dumps(request).encode())
 started=time.monotonic()
 p=call('DEPLOY_REQUEST_SIGNING_KEY=simulated-key DEPLOY_REQUEST_SIGNING_KEY_ID=simulated TENCENT_COS_BUCKET=simulated TENCENT_COS_REGION=simulated '+release+'/deploy/runner/qintopia-agent-os-deploy-runner --request-file '+request_path,expected=75)
 elapsed=time.monotonic()-started;assert 34.8<=elapsed<38,p.stderr.decode()
 assert b'foundation_broker_stop=deferred deadline_seconds=35 outcome=unknown' in p.stderr
 after=state_data();assert after['MainPID']==before['MainPID'] and after['starttime']==before['starttime'] and after['InvocationID']==before['InvocationID'] and after['cgroup']==before['cgroup']
 hold=output('stat -c "%u:%g:%a" '+state+'/recovery/hold');assert hold=='0:0:600';assert output('cat '+state+'/recovery/hold')==request_id
 result=json.loads(output('cat '+state+'/results/'+request_id+'.json'));detail=json.loads(result['checks'][0]['detail']);assert detail['failure_stage']=='quiesce-management-ui' and detail['exit_status']==75 and detail['promoted_current'] is False
 assert pointer_state()==pointers;assert output('test ! -e /tmp/qintopia-c1-install-events && echo absent')=='absent'
 case('actual_inflight_35_second_runner_hold',exit=p.returncode,elapsed_seconds=round(elapsed,3),before=before,after=after,hold_metadata=hold,hold_created_by_original_runner=True,failure_stage=detail['failure_stage'],pointers_unchanged=True,no_installer_or_promotion=True)
 block_out,block_err=block.communicate(timeout=30);assert block.returncode==0,block_err.decode()
 client_out,client_err=client.communicate(timeout=10);assert client.returncode==0,client_err.decode()
 client_result=json.loads(client_out);assert client_result['ok'] is False and client_result['error']['code']=='outcome_unknown',client_result
 for _ in range(150):
  if output('test -e /proc/'+before['MainPID']+' && echo exists || true')!='exists':break
  time.sleep(.1)
 else:raise AssertionError('broker did not naturally finish')
 assert output('cat '+state+'/recovery/hold')==request_id and pointer_state()==pointers
 assert output('test ! -e /tmp/qintopia-c1-install-events && echo absent')=='absent'
 journal=output('journalctl --no-pager -o cat _SYSTEMD_INVOCATION_ID='+before['InvocationID'])
 assert 'foundation_broker_drain_deferred' in journal,journal
 # Report only whitelisted lifecycle markers; never quote request/private data.
 case('late_completion_preserves_hold_no_replay',hold_unchanged=True,pointers_unchanged=True,no_promotion_or_install=True,client_outcome='outcome_unknown',new_request_attempted=False,original_process_naturally_gone=True,drain_deferred_30_seconds_observed=True,final=state_data())
 # No reset-failed, no socket deletion, no retry of an unknown business call.
 p=call(lock+helper+' verify-closed',expected=75)
 assert output('cat '+state+'/recovery/hold')==request_id
 assert output('stat -c "%u:%g:%a" '+state+'/recovery/hold')=='0:0:600'
 case('post_late_completion_closure_status',exit=p.returncode,hold_retained=True)
finally:
 # The PostgreSQL transaction has bounded pg_sleep and commits naturally; do not
 # kill it or repeat an outcome-unknown request if a test assertion fails.
 if block.poll() is None:block.communicate(timeout=80)
print(json.dumps({'cases_passed':len(results),'actual_systemd_rust_sdk_pg':True,'installation_ready':False,'final_hold_retained':True}),flush=True)
`,
      process.cwd(),
      process.env.QINTOPIA_FOUNDATION_LAB_VM,
      process.env.QINTOPIA_FOUNDATION_LAB_PG_CONTAINER,
    ],
    { encoding: "utf8", stdio: "inherit" }
  );
  if (result.error) throw result.error;
  process.exit(result.status ?? 1);
}

if (
  process.argv[2] === "--management-ui-maintenance" ||
  process.argv[2] === "--management-ui-maintenance-systemd" ||
  process.argv[2] === "--management-ui-maintenance-systemd-producer"
) {
  const realProducer =
    process.argv[2] === "--management-ui-maintenance-systemd-producer";
  const realSystemd =
    realProducer || process.argv[2] === "--management-ui-maintenance-systemd";
  const repoRoot = process.env.QINTOPIA_FIXTURE_REPO ?? process.cwd();
  const fixture = fs.mkdtempSync(
    realSystemd
      ? path.join(os.tmpdir(), "qintopia-management-ui-maintenance-")
      : path.join(repoRoot, ".local-workspace/management-ui-maintenance-")
  );
  try {
    fs.writeFileSync(
      path.join(fixture, "run.sh"),
      String.raw`#!/usr/bin/env bash
set -euo pipefail
if [[ "$REAL_PRODUCER" == true ]]; then
  trap 'echo "fixture failure at line $LINENO: $BASH_COMMAND" >&2' ERR
fi
R=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
T=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
O=cccccccccccccccccccccccccccccccccccccccc
REQUEST_ID=deploy-20260929T120000Z-abcdef0
ROOT=/home/ubuntu/qintopia-agent-os-releases
STATE=/var/lib/qintopia-agent-os-deploy
RELEASE="$ROOT/$R"
SOURCE_ROOT="$QINTOPIA_FIXTURE_REPO"
if [[ "$REAL_SYSTEMD" == true ]]; then
  existing_dropin=/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf
  dropin_existed=false
  if [[ -e "$existing_dropin" || -L "$existing_dropin" ]]; then
    [[ -f "$existing_dropin" && ! -L "$existing_dropin" ]] || exit 75
    cmp -s "$existing_dropin" \
      "$SOURCE_ROOT/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf" || exit 75
    dropin_existed=true
  fi
  [[ "$(id -u)" == 0 && -d /run/systemd/system && ! -e "$ROOT" &&
    ! -e /etc/qintopia/cos-artifacts.env &&
    ! -e /etc/nginx/sites-available/qintopia-management-ui.conf &&
    ! -e "$STATE/requests/current.json" &&
    ! -e "$STATE/recovery/hold" &&
    ! -e /usr/bin/certbot ]] || exit 75
  if getent passwd qintopia-management-ui >/dev/null; then exit 75; fi
  cleanup() {
    /usr/bin/systemctl disable --now qintopia-agent-os-deploy-runner.timer >/dev/null 2>&1 || true
    /usr/bin/systemctl stop qintopia-agentos-management-ui.service >/dev/null 2>&1 || true
    rm -f /run/systemd/system/qintopia-agent-os-deploy-runner.timer \
      /run/systemd/system/qintopia-agent-os-deploy-runner.service \
      /run/systemd/system/qintopia-agentos-management-ui.service \
      /etc/nginx/sites-enabled/qintopia-management-ui.conf \
      /etc/nginx/sites-available/qintopia-management-ui.conf \
      /etc/qintopia/cos-artifacts.env /etc/qintopia/collaboration-management-ui.env \
      /etc/letsencrypt/renewal/qintopia-management-ui.conf \
      /usr/bin/certbot /usr/local/share/ca-certificates/qintopia-maintenance-test.crt
    if [[ "$dropin_existed" != true ]]; then rm -f "$existing_dropin"; fi
    rm -rf "$ROOT" /etc/letsencrypt/live/qintopia-management-ui \
      /tmp/qintopia-maintenance-injection /tmp/qintopia-maintenance-cos
    rm -f "$STATE/requests/processed/$REQUEST_ID.json" \
      "$STATE/results/$REQUEST_ID.json" "$STATE/recovery/$REQUEST_ID.json" \
      "$STATE/recovery/management-ui-maintenance.json" "$STATE/recovery/hold" \
      "$STATE/requests/current.json"
    /usr/sbin/userdel qintopia-management-ui >/dev/null 2>&1 || true
    /usr/bin/systemctl daemon-reload
    /usr/sbin/nginx -t >/dev/null 2>&1 && /usr/bin/systemctl reload nginx || true
    /usr/sbin/update-ca-certificates >/dev/null 2>&1 || true
  }
  trap cleanup EXIT
fi
mkdir -p "$RELEASE/deploy/runner/qintopia-agent-os-deploy-runner.service.d" \
  "$RELEASE/runtime/nginx/templates" "$RELEASE/deploy-bundle" \
  "$ROOT/$T" "$ROOT/$O" "$STATE/requests/processed" \
  "$STATE/requests/claimed" "$STATE/results" "$STATE/recovery" \
  /etc/qintopia /etc/nginx/sites-available /etc/nginx/sites-enabled \
  /etc/letsencrypt/live/qintopia-management-ui /etc/letsencrypt/renewal
chmod 0700 "$STATE"
cp "$SOURCE_ROOT/deploy/runner/management-ui-lifecycle.sh" \
  "$SOURCE_ROOT/deploy/runner/wait-deploy-result.sh" "$RELEASE/deploy/runner/"
cp "$SOURCE_ROOT/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf" \
  "$RELEASE/deploy/runner/qintopia-agent-os-deploy-runner.service.d/"
cp "$SOURCE_ROOT"/runtime/nginx/templates/management-ui-*.conf.template \
  "$RELEASE/runtime/nginx/templates/"
chmod 0755 "$RELEASE/deploy/runner/"*.sh
chmod 0644 "$RELEASE/deploy/runner/qintopia-agent-os-deploy-runner.service.d/"*.conf
ln -s "$RELEASE" "$ROOT/current"
ln -s "$ROOT/$T" "$ROOT/previous"
if [[ "$REAL_SYSTEMD" != true ]]; then
  cp "$RELEASE/runtime/nginx/templates/management-ui-https.conf.template" \
    /etc/nginx/sites-available/qintopia-management-ui.conf
  ln -s /etc/nginx/sites-available/qintopia-management-ui.conf \
    /etc/nginx/sites-enabled/qintopia-management-ui.conf
  openssl req -x509 -nodes -newkey rsa:2048 -days 9 \
    -subj /CN=agentos.qintopia.cn -addext subjectAltName=DNS:agentos.qintopia.cn \
    -keyout /tmp/management-ui-key.pem \
    -out /etc/letsencrypt/live/qintopia-management-ui/fullchain.pem \
    >/dev/null 2>&1
  : >/etc/letsencrypt/renewal/qintopia-management-ui.conf
fi
printf '%s\n' 'export TENCENT_COS_BUCKET=simulated' \
  'export TENCENT_COS_REGION=simulated' \
  'export TENCENT_COS_SECRET_ID=simulated' \
  'export TENCENT_COS_SECRET_KEY=simulated' \
  'export DEPLOY_REQUEST_SIGNING_KEY=simulated-key' \
  'export DEPLOY_REQUEST_SIGNING_KEY_ID=simulated' \
  >/etc/qintopia/cos-artifacts.env
chmod 0600 /etc/qintopia/cos-artifacts.env

python3 - "$RELEASE" "$ROOT" "$STATE" "$R" "$T" "$O" "$REQUEST_ID" <<'PY'
import hashlib, hmac, json, sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

release, root, state = map(Path, sys.argv[1:4])
r, t, o, request_id = sys.argv[4:8]
def write(path, data, mode):
    path.write_text(json.dumps(data, sort_keys=True) + "\n")
    path.chmod(mode)
def digest(data):
    return hashlib.sha256(data).hexdigest()
def canonical(value):
    if isinstance(value, list):
        return "[" + ",".join(map(canonical, value)) + "]"
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(k, separators=(",", ":")) + ":" + canonical(value[k])
                              for k in sorted(value)) + "}"
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
def sign(value, field, issuer, timestamp):
    metadata = {"algorithm": "hmac-sha256", "issuer": issuer,
                "key_id": "simulated", "signed_at": timestamp}
    signature = hmac.new(b"simulated-key", canonical({field: value, "signature": metadata}).encode(),
                         hashlib.sha256).hexdigest()
    return {**value, "signature": {**metadata, "value": signature}}
scope = ["sidecar-runtime", "deploy-bundle", "hermes-plugins"]
targets = ["qintopia-system-services", "hermes-erhua", "hermes-xiaoman",
           "hermes-silaoshi", "hermes-huabaosi", "hermes-anan"]
now = datetime.now(timezone.utc)
stamp = now.isoformat()
request = sign({"schema_version": 1, "request_id": request_id, "environment": "production",
                "created_at": stamp, "expires_at": (now + timedelta(hours=1)).isoformat(),
                "release_sha": r, "commit_sha": r, "runtime_sha": r,
                "runtime_artifact_profile": "huabaosi-production", "deploy_bundle_sha": r,
                "release_scope": scope, "restart_targets": targets, "dry_run": False,
                "cos": {"bucket": "simulated", "region": "simulated",
                        "prefix": "qintopia-agent-os",
                        "request_key": f"qintopia-agent-os/deploy-requests/production/requests/{request_id}.json",
                        "result_key": f"qintopia-agent-os/deploy-results/production/{request_id}.json"}},
               "request", "github-actions", stamp)
request_path = state / "requests/processed" / (request_id + ".json")
write(request_path, request, 0o644)
result = sign({"schema_version": 1, "request_id": request_id, "environment": "production",
               "status": "succeeded", "started_at": stamp, "finished_at": stamp,
               "release_sha": r, "commit_sha": r, "runtime_sha": r,
               "runtime_artifact_profile": "huabaosi-production", "deploy_bundle_sha": r,
               "release_scope": scope, "restart_targets": targets,
               "previous_sha": t, "current_target": str(release),
               "checks": [{"name": "deploy-runner", "status": "passed"}],
               "rollback": {"attempted": False, "status": "not_needed"}},
              "result", "qintopia-deploy-runner", stamp)
result_path = state / "results" / (request_id + ".json")
write(result_path, result, 0o644)
write(root / o / "manifest.json", {"release_sha": o}, 0o444)
write(root / t / "manifest.json", {"release_sha": t, "previous_sha": o,
                                    "commit_sha": t, "runtime_sha": o, "deploy_bundle_sha": r,
                                    "release_scope": ["deploy-bundle"],
                                    "restart_targets": ["qintopia-system-services"]}, 0o444)
write(release / "manifest.json", {"release_sha": r, "previous_sha": t,
                                    "commit_sha": r, "runtime_sha": r, "deploy_bundle_sha": r,
                                    "runtime_artifact_profile": "huabaosi-production",
                                    "release_scope": scope, "restart_targets": targets}, 0o444)
names = ["deploy/runner/management-ui-lifecycle.sh", "deploy/runner/wait-deploy-result.sh",
         "deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf",
         "runtime/nginx/templates/management-ui-http.conf.template",
         "runtime/nginx/templates/management-ui-https.conf.template"]
write(release / "deploy-bundle/artifact-manifest.json",
      {"commit_sha": r, "files": [{"path": "payload/" + name,
                                     "sha256": digest((release / name).read_bytes())}
                                    for name in names]}, 0o444)
write(state / "recovery" / (request_id + ".json"),
      {"schema_version": 1, "request_id": request_id,
       "request_sha256": digest(request_path.read_bytes()), "direction": "T→R",
       "phase": "smoke-passed", "hold_token": request_id,
       "original_current_sha": t, "original_previous_sha": o,
       "manifest_sha256": {"current": digest((root / t / "manifest.json").read_bytes()),
                           "previous": digest((root / o / "manifest.json").read_bytes())},
       "result_upload": {"phase": "upload_intent",
                         "payload_sha256": digest(result_path.read_bytes())}}, 0o600)
PY

if [[ "$REAL_SYSTEMD" == true ]]; then
  printf '[Unit]\nDescription=Simulated runner\n[Service]\nType=oneshot\nExecStart=/bin/true\n' \
    >/run/systemd/system/qintopia-agent-os-deploy-runner.service
  printf '[Unit]\nDescription=Simulated timer\n[Timer]\nOnCalendar=hourly\n[Install]\nWantedBy=timers.target\n' \
    >/run/systemd/system/qintopia-agent-os-deploy-runner.timer
  printf '[Unit]\nDescription=Simulated management UI\n[Service]\nType=simple\nExecStart=/usr/bin/sleep infinity\n[Install]\nWantedBy=multi-user.target\n' \
    >/run/systemd/system/qintopia-agentos-management-ui.service
  /usr/bin/systemctl daemon-reload
  if [[ "$SIMULATED_TIMER_INITIAL" == enabled ]]; then
    /usr/bin/systemctl enable --now qintopia-agent-os-deploy-runner.timer
  fi
  /usr/bin/systemctl start qintopia-agentos-management-ui.service
  /usr/bin/systemctl stop qintopia-agentos-management-ui.service
else
cat >/usr/bin/systemctl <<'SH'
#!/usr/bin/env bash
set -euo pipefail
timer=/tmp/management-ui-timer-state
case "$1" in
  show)
    if [[ "$2" == qintopia-agent-os-deploy-runner.timer ]]; then
      if [[ "$3" == --property=UnitFileState ]]; then sed -n '1p' "$timer"; exit; fi
      if [[ "$3" == --property=ActiveState ]]; then sed -n '2p' "$timer"; exit; fi
    elif [[ "$2" == qintopia-agentos-management-ui.service ]]; then
      if [[ -e /tmp/management-ui-active ]]; then
        printf '%s\n' 'LoadState=loaded' 'ActiveState=active' 'SubState=running' \
          'UnitFileState=enabled' 'InvocationID=11111111111111111111111111111111' \
          'Result=success' 'ExecMainCode=0' 'ExecMainStatus=0' \
          'MainPID=1' 'ControlPID=0' 'ControlGroup=' 'NRestarts=0'
        exit
      fi
      printf '%s\n' 'LoadState=loaded' 'ActiveState=inactive' 'SubState=dead' \
        'UnitFileState=disabled' 'InvocationID=' 'Result=success' 'ExecMainCode=0' \
        'ExecMainStatus=0' 'MainPID=0' 'ControlPID=0' 'ControlGroup=' 'NRestarts=0'
      exit
    else
      printf '%s\n' 'LoadState=loaded' 'ActiveState=inactive' \
        'MainPID=0' 'ControlPID=0' 'ControlGroup='
      exit
    fi ;;
  disable) printf '%s\n' disabled inactive >"$timer"; exit ;;
  enable)
    [[ ! -e /tmp/management-ui-fail-enable ]] || exit 75
    printf '%s\n' enabled active >"$timer"; exit ;;
  is-active) [[ "$(sed -n '2p' "$timer")" == active ]]; exit ;;
  daemon-reload) exit ;;
esac
exit 75
SH
chmod 0755 /usr/bin/systemctl
cat >/usr/bin/journalctl <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod 0755 /usr/bin/journalctl
cat >/usr/bin/curl <<'SH'
#!/usr/bin/env bash
if [[ -e /tmp/management-ui-fail-https ]]; then printf '500'; else printf '503'; fi
SH
chmod 0755 /usr/bin/curl
printf '%s\n' "$SIMULATED_TIMER_INITIAL" \
  "$([[ "$SIMULATED_TIMER_INITIAL" == enabled ]] && echo active || echo inactive)" \
  >/tmp/management-ui-timer-state
fi
: >"$STATE/deploy.lock"
: >"$STATE/poller.lock"
if [[ "$REAL_PRODUCER" == true ]]; then
  generated_request="$SOURCE_ROOT/management-ui-producer-request.json"
  [[ -f "$generated_request" ]] || { echo 'generated request is missing' >&2; exit 75; }
  rm -f "$STATE/requests/processed/$REQUEST_ID.json" \
    "$STATE/results/$REQUEST_ID.json" "$STATE/recovery/$REQUEST_ID.json"
  REQUEST_ID="$(python3 - "$generated_request" "$R" <<'PY'
import json, sys
request = json.load(open(sys.argv[1]))
assert request["release_sha"] == request["commit_sha"] == request["runtime_sha"] == request["deploy_bundle_sha"] == sys.argv[2]
assert request["release_scope"] == ["sidecar-runtime", "deploy-bundle", "hermes-plugins"]
assert request["restart_targets"] == ["qintopia-system-services", "hermes-erhua", "hermes-xiaoman", "hermes-silaoshi", "hermes-huabaosi", "hermes-anan"]
assert request["dry_run"] is False
print(request["request_id"])
PY
)"
  ln -sfn "$ROOT/$T" "$ROOT/current"
  ln -sfn "$ROOT/$O" "$ROOT/previous"
  mkdir -p "$ROOT/$T/deploy/runner"
  cp "$SOURCE_ROOT/deploy/runner/qintopia-agent-os-deploy-runner" \
    "$SOURCE_ROOT/deploy/runner/wait-deploy-result.sh" "$ROOT/$T/deploy/runner/"
  cat >"$ROOT/$T/deploy/runner/quiesce-space-automation-runtime.sh" <<'SH'
#!/usr/bin/env bash
exit 0
SH
  cat >"$ROOT/$T/deploy/runner/management-ui-lifecycle.sh" <<'SH'
#!/usr/bin/env bash
exit 0
SH
  cat >"$ROOT/$T/deploy/runner/promote-release.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
ln -sfn /home/ubuntu/qintopia-agent-os-releases/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb \
  /home/ubuntu/qintopia-agent-os-releases/previous
ln -sfn /home/ubuntu/qintopia-agent-os-releases/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  /home/ubuntu/qintopia-agent-os-releases/current
SH
  for script in install-release-systemd-units.sh smoke-release.sh; do
    printf '#!/usr/bin/env bash\nexit 0\n' >"$RELEASE/deploy/runner/$script"
    chmod 0755 "$RELEASE/deploy/runner/$script"
  done
  chmod 0755 "$ROOT/$T/deploy/runner/"*.sh \
    "$ROOT/$T/deploy/runner/qintopia-agent-os-deploy-runner"
  cos_root=/tmp/qintopia-maintenance-cos
  mkdir -m 0700 -p "$cos_root/qintopia-agent-os/deploy-requests/production/requests"
  cp "$generated_request" \
    "$cos_root/qintopia-agent-os/deploy-requests/production/requests/$REQUEST_ID.json"
  python3 - "$generated_request" "$cos_root/qintopia-agent-os/deploy-requests/production/current.json" <<'PY'
import json, sys
request = json.load(open(sys.argv[1]))
pointer = {key: request[key] for key in ("schema_version", "environment", "repository", "request_id")}
pointer.update({"request_key": request["cos"]["request_key"],
                "result_key": request["cos"]["result_key"]})
with open(sys.argv[2], "w") as file:
    json.dump(pointer, file)
    file.write("\n")
PY
  cat >"$cos_root/coscli" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == config ]]; then exit 0; fi
[[ "$1" == cp ]] || exit 75
source="$2"; destination="$3"
if [[ "$source" == cos://* ]]; then
  source="/tmp/qintopia-maintenance-cos/$(printf '%s' "$source" | cut -d/ -f4-)"
  [[ -f "$source" ]] || { echo NoSuchKey >&2; exit 1; }
else
  destination="/tmp/qintopia-maintenance-cos/$(printf '%s' "$destination" | cut -d/ -f4-)"
  mkdir -p "$(dirname "$destination")"
fi
cp "$source" "$destination"
SH
  chmod 0755 "$cos_root/coscli"
  COSCLI_PATH="$cos_root/coscli" \
    QINTOPIA_DEPLOY_RUNNER_BIN="$ROOT/$T/deploy/runner/qintopia-agent-os-deploy-runner" \
    "$SOURCE_ROOT/deploy/runner/poll-deploy-requests.sh"
  test ! -e "$STATE/requests/claimed/$REQUEST_ID.json"
  test -f "$STATE/requests/processed/$REQUEST_ID.json"
  test "$(stat -c '%a' "$STATE/requests/processed/$REQUEST_ID.json")" = 644
  test "$(stat -c '%a' "$STATE/results/$REQUEST_ID.json")" = 644
  test "$(stat -c '%a' "$STATE/recovery/$REQUEST_ID.json")" = 600
  cmp "$STATE/results/$REQUEST_ID.json" \
    "$cos_root/qintopia-agent-os/deploy-results/production/$REQUEST_ID.json"
  printf 'real_runner_poller_producer=%s passed\n' "$SIMULATED_TIMER_INITIAL"
fi
helper="$RELEASE/deploy/runner/management-ui-lifecycle.sh"
mkdir -p /tmp/qintopia-maintenance-injection
cat >/tmp/qintopia-maintenance-injection/sitecustomize.py <<'PY'
import errno
import os
import signal
import sys

mode = os.environ.get("SIMULATED_MAINTENANCE_FAILURE", "")
record = (len(sys.argv) > 4 and
          sys.argv[1].endswith("/management-ui-maintenance.json"))
hold = (len(sys.argv) > 3 and
        sys.argv[1].endswith("/recovery") and
        sys.argv[2].startswith("deploy-"))
action = sys.argv[4] if record else sys.argv[3] if hold else ""

if record and action == "create" and mode == "write":
    original_open = os.open
    def fail_open(path, flags, *args, **kwargs):
        if os.fspath(path) == sys.argv[1] and flags & os.O_EXCL:
            raise OSError(errno.EIO, "simulated maintenance record write failure")
        return original_open(path, flags, *args, **kwargs)
    os.open = fail_open
elif record and action == "create" and mode == "fsync":
    def fail_fsync(_descriptor):
        raise OSError(errno.EIO, "simulated maintenance record fsync failure")
    os.fsync = fail_fsync
elif record and action == "create" and mode == "directory-fsync":
    original_fsync = os.fsync
    count = [0]
    def fail_directory_fsync(descriptor):
        count[0] += 1
        if count[0] == 2:
            raise OSError(errno.EIO, "simulated maintenance directory fsync failure")
        return original_fsync(descriptor)
    os.fsync = fail_directory_fsync

kill_at_start = ((mode == "kill:before-hold" and hold and action == "create") or
                 (mode == "kill:before-isolated" and record and action == "isolated") or
                 (mode == "kill:before-hold-remove" and hold and action == "remove"))
if kill_at_start:
    os.kill(os.getppid(), signal.SIGKILL)
    os._exit(137)

kill_on_fsync = ((mode == "kill:record-created" and record and action == "create") or
                 (mode == "kill:hold-created" and hold and action == "create") or
                 (mode == "kill:isolated" and record and action == "isolated") or
                 (mode == "kill:finishing" and record and action == "finishing") or
                 (mode == "kill:hold-removed" and hold and action == "remove"))
if kill_on_fsync:
    original_fsync = os.fsync
    count = [0]
    target = 1 if mode == "kill:hold-removed" else 2
    def kill_after_fsync(descriptor):
        result = original_fsync(descriptor)
        count[0] += 1
        if count[0] == target:
            os.kill(os.getppid(), signal.SIGKILL)
            os._exit(137)
        return result
    os.fsync = kill_after_fsync
PY

if [[ "$REAL_SYSTEMD" == true ]]; then
  for failure in write fsync directory-fsync; do
    if SIMULATED_MAINTENANCE_FAILURE="$failure" \
        PYTHONPATH=/tmp/qintopia-maintenance-injection \
        "$helper" begin "$REQUEST_ID" >/dev/null 2>&1; then
      echo "record $failure failure changed maintenance state" >&2; exit 1
    fi
    test "$(/usr/bin/systemctl show qintopia-agent-os-deploy-runner.timer --property=UnitFileState --value)" = "$SIMULATED_TIMER_INITIAL"
    test ! -e "$STATE/recovery/hold"
    rm -f "$STATE/recovery/management-ui-maintenance.json"
  done
  if "$helper" begin deploy-20260929T120001Z-abcdef1 >/dev/null 2>&1; then
    echo 'unknown maintenance request was accepted' >&2; exit 1
  fi
  cp "$STATE/results/$REQUEST_ID.json" /tmp/qintopia-maintenance-signed-result.json
  python3 - "$STATE/results/$REQUEST_ID.json" <<'PY'
import json, sys
path = sys.argv[1]
result = json.load(open(path))
result["signature"]["value"] = "0" * 64
open(path, "w").write(json.dumps(result) + "\n")
PY
  if "$helper" begin "$REQUEST_ID" >/dev/null 2>&1; then
    echo 'unknown signed result was accepted' >&2; exit 1
  fi
  cp /tmp/qintopia-maintenance-signed-result.json "$STATE/results/$REQUEST_ID.json"
  "$helper" begin "$REQUEST_ID"
  test "$(cat "$STATE/recovery/hold")" = "$REQUEST_ID"
  test "$(/usr/bin/systemctl show qintopia-agent-os-deploy-runner.timer --property=UnitFileState --value)" = disabled
  printf '%s\n' \
    'QINTOPIA_FOUNDATION_PRODUCTION_ENABLE=1' \
    'QINTOPIA_FOUNDATION_TENANT=simulated' \
    'QINTOPIA_FOUNDATION_IDENTITY_NAMESPACE=simulated' \
    'QINTOPIA_FOUNDATION_DATABASE_URL=postgres://qintopia_management_ui:simulated@127.0.0.1:65535/qintopia' \
    'QINTOPIA_COLLABORATION_PUBLIC_ORIGIN=https://agentos.qintopia.cn' \
    | "$helper" prepare
  "$helper" install-http
  mkdir -p /etc/letsencrypt/accounts
  cat >/usr/bin/certbot <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == certonly ]]; then
  openssl req -x509 -nodes -newkey rsa:2048 -days 9 \
    -subj /CN=agentos.qintopia.cn -addext subjectAltName=DNS:agentos.qintopia.cn \
    -keyout /etc/letsencrypt/live/qintopia-management-ui/privkey.pem \
    -out /etc/letsencrypt/live/qintopia-management-ui/fullchain.pem \
    >/dev/null 2>&1
  : >/etc/letsencrypt/renewal/qintopia-management-ui.conf
elif [[ "$1" != renew || "$2" != --cert-name || "$4" != --dry-run ]]; then
  exit 75
fi
SH
  chmod 0755 /usr/bin/certbot
  "$helper" issue-cert
  cp /etc/letsencrypt/live/qintopia-management-ui/fullchain.pem \
    /usr/local/share/ca-certificates/qintopia-maintenance-test.crt
  /usr/sbin/update-ca-certificates >/dev/null
  "$helper" install-https
  test "$(curl --noproxy '*' --silent --show-error --output /dev/null --write-out '%{http_code}' \
    --resolve agentos.qintopia.cn:443:127.0.0.1 \
    https://agentos.qintopia.cn/)" = 503
  "$helper" finish "$REQUEST_ID"
  test ! -e "$STATE/recovery/hold"
  test "$(/usr/bin/systemctl show qintopia-agent-os-deploy-runner.timer --property=UnitFileState --value)" = "$SIMULATED_TIMER_INITIAL"
  "$helper" finish "$REQUEST_ID"
  printf 'real_systemd_first_setup=%s passed\n' "$SIMULATED_TIMER_INITIAL"

  record="$STATE/recovery/management-ui-maintenance.json"
  for boundary in record-created before-hold hold-created before-isolated isolated \
      finishing before-hold-remove hold-removed; do
    printf 'real_systemd_boundary_start=%s:%s\n' "$SIMULATED_TIMER_INITIAL" "$boundary" >&2
    rm "$record"
    if [[ "$boundary" == record-created || "$boundary" == before-hold ||
          "$boundary" == hold-created || "$boundary" == before-isolated ||
          "$boundary" == isolated ]]; then
      if SIMULATED_MAINTENANCE_FAILURE="kill:$boundary" \
          PYTHONPATH=/tmp/qintopia-maintenance-injection \
          "$helper" begin "$REQUEST_ID" >/dev/null 2>&1; then
        echo "begin unexpectedly survived $boundary" >&2; exit 1
      fi
    else
      "$helper" begin "$REQUEST_ID"
      if SIMULATED_MAINTENANCE_FAILURE="kill:$boundary" \
          PYTHONPATH=/tmp/qintopia-maintenance-injection \
          "$helper" finish "$REQUEST_ID" >/dev/null 2>&1; then
        echo "finish unexpectedly survived $boundary" >&2; exit 1
      fi
    fi
    for attempt in 1 2 3 4 5 6 7 8 9 10; do
      if flock -n "$STATE/poller.lock" true && flock -n "$STATE/deploy.lock" true; then
        break
      fi
      sleep 0.1
    done
    printf 'real_systemd_boundary_state=%s:%s phase=%s timer=%s hold=%s\n' \
      "$SIMULATED_TIMER_INITIAL" "$boundary" \
      "$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["phase"])' "$record" 2>/dev/null || echo missing)" \
      "$(/usr/bin/systemctl show qintopia-agent-os-deploy-runner.timer --property=UnitFileState --value)" \
      "$([[ -e "$STATE/recovery/hold" ]] && echo present || echo absent)" >&2
    test "$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["phase"])' "$record")" = \
      "$(case "$boundary" in isolated) echo isolated ;; finishing|before-hold-remove|hold-removed) echo finishing ;; *) echo preparing ;; esac)"
    if [[ "$boundary" == record-created ]]; then
      test "$(/usr/bin/systemctl show qintopia-agent-os-deploy-runner.timer --property=UnitFileState --value)" = "$SIMULATED_TIMER_INITIAL"
      test ! -e "$STATE/recovery/hold"
    elif [[ "$boundary" == before-hold ]]; then
      test "$(/usr/bin/systemctl show qintopia-agent-os-deploy-runner.timer --property=UnitFileState --value)" = disabled
      test ! -e "$STATE/recovery/hold"
    elif [[ "$boundary" == hold-removed ]]; then
      test ! -e "$STATE/recovery/hold"
    else
      test "$(cat "$STATE/recovery/hold")" = "$REQUEST_ID"
    fi
    if [[ "$boundary" == record-created || "$boundary" == before-hold ||
          "$boundary" == hold-created || "$boundary" == before-isolated ||
          "$boundary" == isolated ]]; then
      "$helper" begin "$REQUEST_ID"
    fi
    "$helper" finish "$REQUEST_ID"
    test ! -e "$STATE/recovery/hold"
    test "$(/usr/bin/systemctl show qintopia-agent-os-deploy-runner.timer --property=UnitFileState --value)" = "$SIMULATED_TIMER_INITIAL"
    printf 'real_systemd_recovery=%s:%s passed\n' "$SIMULATED_TIMER_INITIAL" "$boundary"
  done
  exit 0
fi

chmod 0644 /etc/qintopia/cos-artifacts.env
if "$helper" begin "$REQUEST_ID" >/dev/null 2>&1; then
  echo 'readable signing environment was accepted' >&2; exit 1
fi
chmod 0600 /etc/qintopia/cos-artifacts.env
python3 - "$STATE/recovery/$REQUEST_ID.json" <<'PY'
import json, sys
path = sys.argv[1]
data = json.load(open(path))
data["result_upload"]["payload_sha256"] = "0" * 64
open(path, "w").write(json.dumps(data) + "\n")
PY
if "$helper" begin "$REQUEST_ID" >/dev/null 2>&1; then
  echo 'conflicting result digest was accepted' >&2; exit 1
fi
python3 - "$STATE/recovery/$REQUEST_ID.json" "$STATE/results/$REQUEST_ID.json" <<'PY'
import hashlib, json, sys
path, result = sys.argv[1:]
data = json.load(open(path))
data["result_upload"]["payload_sha256"] = hashlib.sha256(open(result, "rb").read()).hexdigest()
open(path, "w").write(json.dumps(data) + "\n")
PY
printf '%s\n' '{}' >"$STATE/requests/claimed/deploy-20260929T120001Z-bbbbbbb.json"
if "$helper" begin "$REQUEST_ID" >/dev/null 2>&1; then
  echo 'unfinished claim was accepted' >&2; exit 1
fi
rm "$STATE/requests/claimed/deploy-20260929T120001Z-bbbbbbb.json"
touch /tmp/management-ui-active
if "$helper" begin "$REQUEST_ID" >/dev/null 2>&1; then
  echo 'active management UI was accepted' >&2; exit 1
fi
rm /tmp/management-ui-active
printf 'foreign-request\n' >"$STATE/recovery/hold"
chmod 0600 "$STATE/recovery/hold"
if "$helper" begin "$REQUEST_ID" >/dev/null 2>&1; then
  echo 'foreign hold was accepted' >&2; exit 1
fi
rm "$STATE/recovery/hold"
printf '{}\n' >"$STATE/recovery/deploy-20260929T120001Z-abcdef1.json"
if "$helper" begin "$REQUEST_ID" >/dev/null 2>&1; then
  echo 'later journal was accepted' >&2; exit 1
fi
rm "$STATE/recovery/deploy-20260929T120001Z-abcdef1.json"
cp "$STATE/results/$REQUEST_ID.json" /tmp/management-ui-original-result.json
python3 - "$STATE/results/$REQUEST_ID.json" "$STATE/recovery/$REQUEST_ID.json" <<'PY'
import hashlib, json, sys
result_path, journal_path = sys.argv[1:]
result = json.load(open(result_path))
result["signature"]["value"] = "0" * 64
open(result_path, "w").write(json.dumps(result) + "\n")
journal = json.load(open(journal_path))
journal["result_upload"]["payload_sha256"] = hashlib.sha256(open(result_path, "rb").read()).hexdigest()
open(journal_path, "w").write(json.dumps(journal) + "\n")
PY
if "$helper" begin "$REQUEST_ID" >/dev/null 2>&1; then
  echo 'invalid signed result was accepted' >&2; exit 1
fi
cp /tmp/management-ui-original-result.json "$STATE/results/$REQUEST_ID.json"
python3 - "$STATE/results/$REQUEST_ID.json" "$STATE/recovery/$REQUEST_ID.json" <<'PY'
import hashlib, json, sys
result_path, journal_path = sys.argv[1:]
journal = json.load(open(journal_path))
journal["result_upload"]["payload_sha256"] = hashlib.sha256(open(result_path, "rb").read()).hexdigest()
open(journal_path, "w").write(json.dumps(journal) + "\n")
PY
for failure in write fsync; do
  if SIMULATED_MAINTENANCE_FAILURE="$failure" \
      PYTHONPATH=/tmp/qintopia-maintenance-injection \
      "$helper" begin "$REQUEST_ID" >/dev/null 2>&1; then
    echo "maintenance record $failure failure was accepted" >&2; exit 1
  fi
  test "$(sed -n '1p' /tmp/management-ui-timer-state)" = "$SIMULATED_TIMER_INITIAL"
  test ! -e "$STATE/recovery/hold"
  rm -f "$STATE/recovery/management-ui-maintenance.json"
done
"$helper" begin "$REQUEST_ID"
test "$(cat "$STATE/recovery/hold")" = "$REQUEST_ID"
test "$(sed -n '1p' /tmp/management-ui-timer-state)" = disabled
python3 - "$STATE/recovery/management-ui-maintenance.json" <<'PY'
import json, sys
path = sys.argv[1]
record = json.load(open(path))
record["phase"] = "preparing"
open(path, "w").write(json.dumps(record) + "\n")
PY
rm "$STATE/recovery/hold"
"$helper" begin "$REQUEST_ID"
test "$(cat "$STATE/recovery/hold")" = "$REQUEST_ID"
"$helper" begin "$REQUEST_ID"
rm "$ROOT/current"
ln -s "$ROOT/$T" "$ROOT/current"
if "$helper" finish "$REQUEST_ID" >/dev/null 2>&1; then
  echo 'pointer conflict was accepted' >&2; exit 1
fi
test -f "$STATE/recovery/hold"
rm "$ROOT/current"
ln -s "$RELEASE" "$ROOT/current"
touch /tmp/management-ui-fail-https
if "$helper" finish "$REQUEST_ID" >/dev/null 2>&1; then
  echo 'failed HTTPS 503 check cleared hold' >&2; exit 1
fi
test -f "$STATE/recovery/hold"
rm /tmp/management-ui-fail-https
if [[ "$SIMULATED_TIMER_INITIAL" == enabled ]]; then
  touch /tmp/management-ui-fail-enable
  if "$helper" finish "$REQUEST_ID" >/dev/null 2>&1; then
    echo 'timer restore failure cleared hold' >&2; exit 1
  fi
  test -f "$STATE/recovery/hold"
  rm /tmp/management-ui-fail-enable
fi
"$helper" finish "$REQUEST_ID"
test ! -e "$STATE/recovery/hold"
test "$(sed -n '1p' /tmp/management-ui-timer-state)" = "$SIMULATED_TIMER_INITIAL"
"$helper" finish "$REQUEST_ID"
printf 'maintenance_window=%s passed\n' "$SIMULATED_TIMER_INITIAL"
`,
      { mode: 0o755 }
    );
    for (const initial of ["enabled", "disabled"]) {
      const result = realSystemd
        ? spawnSync("bash", [path.join(fixture, "run.sh")], {
            encoding: "utf8",
            timeout: 600000,
            env: {
              ...process.env,
              REAL_SYSTEMD: "true",
              REAL_PRODUCER: String(realProducer),
              QINTOPIA_FIXTURE_REPO: repoRoot,
              SIMULATED_TIMER_INITIAL: initial,
            },
          })
        : spawnSync(
            "docker",
            [
              "run",
              "--rm",
              "-v",
              `${repoRoot}:/repo:ro`,
              "-v",
              `${fixture}:/fixture:ro`,
              "-e",
              `SIMULATED_TIMER_INITIAL=${initial}`,
              "-e",
              "REAL_SYSTEMD=false",
              "-e",
              "REAL_PRODUCER=false",
              "-e",
              "QINTOPIA_FIXTURE_REPO=/repo",
              "python:3.12-slim",
              "bash",
              "/fixture/run.sh",
            ],
            { encoding: "utf8", timeout: 120000 }
          );
      assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
      console.log(result.stdout.trim());
    }
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
  process.exit(0);
}

if (process.argv[2] === "--management-ui-mock") {
  const repoRoot = process.cwd();
  const fixtureBase = path.join(repoRoot, ".local-workspace");
  fs.mkdirSync(fixtureBase, { recursive: true });
  const fixture = fs.mkdtempSync(path.join(fixtureBase, "management-ui-mock-"));
  try {
    fs.writeFileSync(
      path.join(fixture, "systemctl"),
      '#!/usr/bin/env bash\nset -euo pipefail\nif [[ "$1" == show ]]; then cat /tmp/management-ui-state; exit 0; fi\nif [[ "$1" == disable && "$2" == --now ]]; then\n  sed -n "s/^InvocationID=//p" /tmp/management-ui-state >/tmp/management-ui-last-invocation\n  kill -TERM "$(cat /tmp/management-ui-pid)"\n  cp /tmp/management-ui-stopped /tmp/management-ui-state\n  if [[ -f /tmp/management-ui-emit-unknown ]]; then printf "%s\\n" production_ui_stop_outcome_unknown >>/tmp/management-ui-journal; fi\n  exit 0\nfi\nexit 75\n',
      { mode: 0o755 }
    );
    fs.writeFileSync(
      path.join(fixture, "journalctl"),
      `#!/usr/bin/env bash
set -euo pipefail
if [[ " $* " == *" -o json "* ]]; then
  if [[ -s /tmp/management-ui-last-invocation ]]; then
    if [[ " $* " == *"MESSAGE_ID=7ad2d189f7e94e70a38c781354912448"* ]]; then
      printf '{"UNIT":"qintopia-agentos-management-ui.service","MESSAGE_ID":"7ad2d189f7e94e70a38c781354912448","INVOCATION_ID":"%s"}\\n' "$(cat /tmp/management-ui-last-invocation)"
    else
      printf '{"UNIT":"qintopia-agentos-management-ui.service","JOB_TYPE":"stop","JOB_RESULT":"done","MESSAGE_ID":"9d1aaa27d60140bd96365438aad20286","INVOCATION_ID":"%s"}\\n' "$(cat /tmp/management-ui-last-invocation)"
    fi
  fi
else
  cat /tmp/management-ui-journal
fi
`,
      { mode: 0o755 }
    );
    fs.writeFileSync(
      path.join(fixture, "run.sh"),
      `#!/usr/bin/env bash
set -euo pipefail
sha=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
bundle=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
release=/home/ubuntu/qintopia-agent-os-releases/$sha
mkdir -p "$release/deploy/runner" "$release/runtime/nginx/templates" \
  "$release/deploy-bundle" "$release/sidecar" /var/lib/qintopia-agent-os-deploy
cp /repo/deploy/runner/management-ui-lifecycle.sh "$release/deploy/runner/"
cp /repo/runtime/nginx/templates/management-ui-*.conf.template "$release/runtime/nginx/templates/"
chmod 0755 "$release/deploy/runner/management-ui-lifecycle.sh"
chmod 0644 "$release/runtime/nginx/templates/"*.conf.template
python3 - "$release" "$sha" "$bundle" <<'PY'
import hashlib, json, pathlib, sys
root, sha, bundle = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
files = []
for relative in ('deploy/runner/management-ui-lifecycle.sh',
                 'runtime/nginx/templates/management-ui-http.conf.template',
                 'runtime/nginx/templates/management-ui-https.conf.template'):
    source = root / relative
    files.append({'path': 'payload/' + relative,
                  'sha256': hashlib.sha256(source.read_bytes()).hexdigest()})
(root / 'manifest.json').write_text(json.dumps({'release_sha': sha,
                                                 'deploy_bundle_sha': bundle}))
(root / 'deploy-bundle/artifact-manifest.json').write_text(
    json.dumps({'commit_sha': bundle, 'files': files}))
PY
chmod 0444 "$release/manifest.json" "$release/deploy-bundle/artifact-manifest.json"
ln -s "$release" /home/ubuntu/qintopia-agent-os-releases/current
install -m 0755 /fixture/systemctl /usr/bin/systemctl
install -m 0755 /fixture/journalctl /usr/bin/journalctl
printf '%s\\n' 'LoadState=loaded' 'ActiveState=inactive' 'SubState=dead' \
  'UnitFileState=disabled' 'InvocationID=' \
  'Result=success' 'ExecMainCode=0' 'ExecMainStatus=0' 'MainPID=0' \
  'ControlPID=0' 'ControlGroup=' 'NRestarts=0' >/tmp/management-ui-stopped
cp /tmp/management-ui-stopped /tmp/management-ui-state
: >/tmp/management-ui-journal
rm -f /tmp/management-ui-last-invocation
helper="$release/deploy/runner/management-ui-lifecycle.sh"
if "$helper" verify-closed >/dev/null 2>&1; then
  echo 'missing inherited FD9 was accepted' >&2
  exit 1
fi
(
  exec 9>/var/lib/qintopia-agent-os-deploy/deploy.lock
  if "$helper" verify-closed >/dev/null 2>&1; then
    echo 'unlocked inherited FD9 was accepted' >&2
    exit 1
  fi
)
flock /var/lib/qintopia-agent-os-deploy/poller.lock sleep 1 &
holder=$!
sleep 0.1
if "$helper" activate >/dev/null 2>/tmp/management-ui-lock-error ||
   ! grep -Fq 'poller lock is held' /tmp/management-ui-lock-error; then
  echo 'external poller lock order was bypassed' >&2
  exit 1
fi
wait "$holder"
flock /var/lib/qintopia-agent-os-deploy/deploy.lock sleep 1 &
holder=$!
sleep 0.1
if "$helper" activate >/dev/null 2>/tmp/management-ui-lock-error ||
   ! grep -Fq 'deploy lock is held' /tmp/management-ui-lock-error; then
  echo 'external deploy lock order was bypassed' >&2
  exit 1
fi
wait "$holder"
(
  exec 9>/var/lib/qintopia-agent-os-deploy/deploy.lock
  flock -n 9
  "$helper" verify-closed
  cp /usr/bin/sleep "$release/sidecar/qintopia-message-sidecar"
  "$release/sidecar/qintopia-message-sidecar" 100 &
  ui_pid=$!
  printf '%s\\n' "$ui_pid" >/tmp/management-ui-pid
  printf '%s\\n' 'LoadState=loaded' 'ActiveState=active' 'SubState=running' \
    'UnitFileState=enabled' 'InvocationID=11111111111111111111111111111111' \
    'Result=success' 'ExecMainCode=0' 'ExecMainStatus=0' "MainPID=$ui_pid" \
    'ControlPID=0' 'ControlGroup=' 'NRestarts=0' >/tmp/management-ui-state
  "$helper" quiesce
  wait "$ui_pid" || true
  "$release/sidecar/qintopia-message-sidecar" 100 &
  ui_pid=$!
  printf '%s\\n' "$ui_pid" >/tmp/management-ui-pid
  printf '%s\\n' 'LoadState=loaded' 'ActiveState=active' 'SubState=running' \
    'UnitFileState=enabled' 'InvocationID=22222222222222222222222222222222' \
    'Result=success' 'ExecMainCode=0' 'ExecMainStatus=0' "MainPID=$ui_pid" \
    'ControlPID=0' 'ControlGroup=' 'NRestarts=0' >/tmp/management-ui-state
  touch /tmp/management-ui-emit-unknown
  if "$helper" quiesce >/dev/null 2>&1; then
    echo 'UNKNOWN raised during drain was accepted' >&2
    exit 1
  fi
  wait "$ui_pid" || true
  if "$helper" verify-closed >/dev/null 2>&1; then
    echo 'UNKNOWN journal code was accepted' >&2
    exit 1
  fi
)
echo 'Management UI locks, drain snapshot and UNKNOWN checks passed.'
`,
      { mode: 0o755 }
    );
    const result = spawnSync(
      "docker",
      [
        "run",
        "--rm",
        "-v",
        `${repoRoot}:/repo:ro`,
        "-v",
        `${fixture}:/fixture:ro`,
        "qintopia-anan-linux-isolation-simulation:20260925",
        "bash",
        "/fixture/run.sh",
      ],
      { encoding: "utf8", timeout: 120000 }
    );
    assert.equal(result.status, 0, result.stderr || result.stdout);
    console.log(result.stdout.trim());
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
  process.exit(0);
}

if (process.argv[2] === "--anan-helper-only") {
  const helper = process.argv[3];
  assert.ok(helper && fs.existsSync(helper), "official Anan helper path is required");
  const exercise = `import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("restart_anan", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
class Control:
    def __init__(self, existing=None): self.marker = existing; self.clears = 0
    def read_drain_request(self, **kwargs): return self.marker
    def write_drain_request(self, **kwargs):
        self.marker = {"principal": kwargs["principal"]}; return self.marker
    def clear_drain_request(self, **kwargs):
        self.marker = None; self.clears += 1; return True
def scenario(records, timeout=10, existing=None):
    control = Control(existing); now = [0]; restarts = []; index = [0]
    def snapshot():
        value = records[min(index[0], len(records)-1)]; index[0] += 1; return value
    def sleep(seconds): now[0] += seconds
    outcome = "ready"
    try:
        module.drain_and_restart(control, snapshot, lambda: restarts.append(now[0]),
            home="simulated-profile", pid=42, clock=lambda: now[0], sleep=sleep, timeout=timeout)
    except module.Deferred as exc: outcome = str(exc)
    return outcome, restarts, control.clears, control.marker
def status(count, updated):
    return {"pid": 42, "active_agents": count, "gateway_state": "draining", "updated_at": updated}
success = scenario([status(1,"t0"), status(0,"t1"), status(0,"t2"), status(0,"t3")])
timeout = scenario([status(1,"t0")], timeout=3)
existing = scenario([], existing={"principal":"operator"})
assert success == ("ready", [3], 1, None), success
assert timeout == ("active_work_timeout", [], 1, None), timeout
assert existing == ("existing_drain", [], 0, {"principal":"operator"}), existing
print(json.dumps({"official_helper_zero_work": success[0],
                  "official_helper_timeout": timeout[0], "existing_drain": existing[0]}))
`;
  const result = spawnSync("python3", ["-c", exercise, helper], { encoding: "utf8" });
  if (result.status !== 0)
    throw new Error(`official Anan helper exercise failed: ${result.stderr}`);
  console.log(result.stdout.trim());
  process.exit(0);
}

if (
  process.getuid?.() !== 0 ||
  fs.readFileSync("/proc/1/comm", "utf8").trim() !== "systemd"
) {
  throw new Error("run as root inside the disposable Ubuntu 24.04 systemd PID1 VM");
}
if (process.argv[2] === "--anan-entry") {
  const [smokeSource, helperSource] = process.argv.slice(3);
  for (const source of [smokeSource, helperSource]) {
    assert.ok(source && fs.existsSync(source), "official Anan entry source is missing");
  }
  const core = "/home/ubuntu/.local/share/hermes-releases/v2026.9.21";
  const profile = "/home/ubuntu/.hermes/profiles/anan";
  const releaseRoot = "/home/ubuntu/qintopia-agent-os-releases";
  const release = path.join(releaseRoot, "a".repeat(40));
  const userUnit = "/home/ubuntu/.config/systemd/user/hermes-gateway-anan.service";
  const stateDir = fs.mkdtempSync("/tmp/anan-entry-state-");
  const requestId = "deploy-20260927T010203Z-abcdef0";
  const uid = 1000;
  const bus = `unix:path=/run/user/${uid}/bus`;
  const userEnv = {
    ...process.env,
    XDG_RUNTIME_DIR: `/run/user/${uid}`,
    DBUS_SESSION_BUS_ADDRESS: bus,
  };
  const run = (command, args, options = {}) =>
    spawnSync(command, args, {
      encoding: "utf8",
      ...options,
    });
  const asUser = (...args) =>
    run("runuser", [
      "-u",
      "ubuntu",
      "--",
      "env",
      `XDG_RUNTIME_DIR=/run/user/${uid}`,
      `DBUS_SESSION_BUS_ADDRESS=${bus}`,
      "systemctl",
      "--user",
      ...args,
    ]);
  const check = (result, label) => {
    assert.equal(result.status, 0, `${label}: ${result.stderr || result.stdout}`);
    return result;
  };
  const write = (target, content, mode = 0o644) => {
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, content, { mode });
    fs.chmodSync(target, mode);
  };
  for (const target of [core, releaseRoot, userUnit]) {
    assert.equal(
      fs.existsSync(target),
      false,
      `Anan entry fixture path exists: ${target}`
    );
  }
  const preexistingProfile = fs.existsSync(profile);
  if (preexistingProfile) {
    assert.deepEqual(
      fs.readdirSync(profile),
      [],
      "simulated Anan profile is not empty"
    );
  }
  let unitStarted = false;
  try {
    const python = path.join(core, "venv/bin/python");
    fs.mkdirSync(path.dirname(python), { recursive: true });
    fs.symlinkSync("/usr/bin/python3", python);
    write(path.join(core, "gateway/__init__.py"), "");
    write(
      path.join(core, "gateway/drain_control.py"),
      `import json
from pathlib import Path
def drain_request_path(home): return Path(home) / "simulated-drain.json"
def read_drain_request(home):
    p = drain_request_path(home)
    return json.loads(p.read_text()) if p.exists() else None
def write_drain_request(home, principal, suppress_notification):
    record = {"principal": principal}
    drain_request_path(home).write_text(json.dumps(record))
    return record
def clear_drain_request(home):
    drain_request_path(home).unlink()
    return True
`
    );
    write(
      path.join(core, "gateway/status.py"),
      `import subprocess, time
def read_runtime_status(path):
    pid = int(subprocess.check_output(["systemctl", "--user", "show",
        "hermes-gateway-anan.service", "--property=MainPID", "--value"]))
    return {"pid": pid, "active_agents": 0, "gateway_state": "draining",
            "updated_at": str(time.monotonic())}
def runtime_status_is_stale(record, ttl_s): return False
def runtime_status_pid_is_live(record): return record["pid"] > 0
`
    );
    fs.mkdirSync(profile, { recursive: true });
    write(
      userUnit,
      `[Unit]\nDescription=Simulated Anan gateway\n[Service]\nType=simple\nWorkingDirectory=${profile}\nExecStart=${python} -c 'import time; time.sleep(3600)'\n`
    );
    const smoke = path.join(release, "deploy/runner/smoke-release.sh");
    const helper = path.join(release, "runtime/hermes/restart_anan.py");
    fs.mkdirSync(path.join(stateDir, "results"));
    write(smoke, fs.readFileSync(smokeSource), 0o755);
    write(helper, fs.readFileSync(helperSource));
    check(
      run("chown", [
        "-R",
        "ubuntu:ubuntu",
        core,
        profile,
        "/home/ubuntu/.config/systemd/user",
      ]),
      "fixture ownership"
    );
    check(asUser("daemon-reload"), "user daemon reload");
    check(asUser("start", "hermes-gateway-anan.service"), "start simulated gateway");
    unitStarted = true;
    const before = check(
      asUser("show", "hermes-gateway-anan.service", "--property=MainPID", "--value"),
      "initial gateway PID"
    ).stdout.trim();
    assert.match(before, /^[1-9][0-9]*$/);
    const result = run(
      "bash",
      [smoke, "--restart-targets", "hermes-anan", "--release-root", releaseRoot],
      {
        env: {
          ...process.env,
          QINTOPIA_DEPLOY_REQUEST_ID: requestId,
          QINTOPIA_DEPLOY_RUNNER_STATE_DIR: stateDir,
        },
      }
    );
    assert.equal(
      result.status,
      0,
      `official Anan smoke failed: ${result.stderr || result.stdout}`
    );
    const receipt = JSON.parse(
      fs.readFileSync(
        path.join(stateDir, "results", `${requestId}.anan-drain.json`),
        "utf8"
      )
    );
    assert.equal(receipt.result, "success");
    assert.equal(receipt.exec_main_status, 0);
    const after = check(
      asUser("show", "hermes-gateway-anan.service", "--property=MainPID", "--value"),
      "restarted gateway PID"
    ).stdout.trim();
    assert.notEqual(after, before, "Anan gateway did not restart");
    assert.equal(
      fs.existsSync(path.join(profile, "simulated-drain.json")),
      false,
      "official helper left its drain marker"
    );
    const invocation = receipt.invocation_id;
    assert.match(invocation, /^[0-9a-f]{32}$/);
    assert.match(
      result.stdout,
      new RegExp(`invocation=${invocation} result=success exit=0`)
    );
    console.log(
      JSON.stringify({
        officialEntry: true,
        smokeEntry: true,
        userSystemd: true,
        gatewayRestarted: true,
        invocation,
      })
    );
  } finally {
    if (unitStarted) asUser("stop", "hermes-gateway-anan.service");
    fs.rmSync(userUnit, { force: true });
    asUser("daemon-reload");
    fs.rmSync(core, { recursive: true, force: true });
    if (!preexistingProfile) fs.rmSync(profile, { recursive: true, force: true });
    fs.rmSync(releaseRoot, { recursive: true, force: true });
    fs.rmSync(stateDir, { recursive: true, force: true });
  }
  process.exit(0);
}
if (process.argv[2] === "--takeover-finalize") {
  const [launcher, waiter, poller, recoveryHelper, holdSource] = process.argv.slice(3);
  for (const source of [launcher, waiter, poller, recoveryHelper, holdSource]) {
    assert.ok(
      source && fs.existsSync(source),
      "takeover finalization source is missing"
    );
  }
  const state = "/var/lib/qintopia-agent-os-deploy";
  const recovery = path.join(state, "recovery");
  const staged = path.join(recovery, "staged");
  const hold = path.join(recovery, "hold");
  const dropin =
    "/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf";
  const releaseRoot = "/home/ubuntu/qintopia-agent-os-releases";
  const service = "qintopia-agent-os-deploy-runner.service";
  const timer = "qintopia-agent-os-deploy-runner.timer";
  const servicePath = `/run/systemd/system/${service}`;
  const timerPath = `/run/systemd/system/${timer}`;
  const oSha = "16e8d56b98001579c6288ba13199b80d6d3dfc74";
  const pSha = "83d694f2c3bc21fd78a73d25da3197379e2a14d5";
  const tSha = "70e7984fab92ddab956009585212d0e9729767b5";
  const rSha = "4".repeat(40);
  const requestId = "deploy-20260927T010203Z-abcdef0";
  const key = "simulated-finalize-key";
  const fixture = fs.mkdtempSync("/tmp/qintopia-takeover-finalize-");
  const replayed = path.join(fixture, "poller-replayed");
  const written = [
    staged,
    hold,
    dropin,
    releaseRoot,
    servicePath,
    timerPath,
    path.join(recovery, "takeover.json"),
    path.join(recovery, "takeover-consumed"),
    path.join(recovery, `${requestId}.json`),
    path.join(state, "requests/processed", `${requestId}.json`),
    path.join(state, "results", `${requestId}.json`),
  ];
  for (const target of written) {
    assert.equal(
      fs.existsSync(target),
      false,
      `takeover fixture path exists: ${target}`
    );
  }
  const run = (command, args, options = {}) =>
    spawnSync(command, args, { encoding: "utf8", ...options });
  const check = (result, label) => {
    assert.equal(result.status, 0, `${label}: ${result.stderr || result.stdout}`);
    return result;
  };
  const write = (target, content, mode = 0o644) => {
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, content, { mode });
    fs.chmodSync(target, mode);
  };
  const sha256 = (value) => crypto.createHash("sha256").update(value).digest("hex");
  const canonical = (value) =>
    Array.isArray(value)
      ? `[${value.map(canonical).join(",")}]`
      : value && typeof value === "object"
        ? `{${Object.keys(value)
            .sort()
            .map((name) => `${JSON.stringify(name)}:${canonical(value[name])}`)
            .join(",")}}`
        : JSON.stringify(value);
  const sign = (unsigned, issuer, signedAt, field) => {
    const metadata = {
      algorithm: "hmac-sha256",
      issuer,
      key_id: "simulated",
      signed_at: signedAt,
    };
    return {
      ...unsigned,
      signature: {
        ...metadata,
        value: crypto
          .createHmac("sha256", key)
          .update(canonical({ [field]: unsigned, signature: metadata }))
          .digest("hex"),
      },
    };
  };
  const wrapperDir = path.join(fixture, "bin");
  write(
    path.join(wrapperDir, "systemctl"),
    `#!/bin/bash
set -euo pipefail
if [[ "$QINTOPIA_FAULT" == reload-fail && "$1" == daemon-reload ]]; then exit 41; fi
if [[ "$QINTOPIA_FAULT" == enable-fail && "$1" == enable ]]; then exit 42; fi
if [[ "$QINTOPIA_FAULT" == kill-after-enable && "$1" == enable ]]; then
  /usr/bin/systemctl "$@"
  kill -9 "$PPID"
  exit 99
fi
exec /usr/bin/systemctl "$@"
`,
    0o755
  );
  write(
    path.join(wrapperDir, "rm"),
    `#!/bin/bash
set -euo pipefail
if [[ "$QINTOPIA_FAULT" == kill-after-unlink && "$1" == "$QINTOPIA_HOLD_FILE" ]]; then
  /usr/bin/rm "$@"
  kill -9 "$PPID"
  exit 99
fi
exec /usr/bin/rm "$@"
`,
    0o755
  );
  const launch = (mode, fault = "") =>
    run("bash", [launcher, mode, ...(mode === "prepare" ? [] : [requestId])], {
      env: {
        ...process.env,
        PATH: `${wrapperDir}:${process.env.PATH}`,
        QINTOPIA_FAULT: fault,
        QINTOPIA_HOLD_FILE: hold,
        DEPLOY_REQUEST_SIGNING_KEY: key,
        DEPLOY_REQUEST_SIGNING_KEY_ID: "simulated",
        TENCENT_COS_BUCKET: "simulated",
        TENCENT_COS_REGION: "simulated",
        TENCENT_COS_SECRET_ID: "simulated",
        TENCENT_COS_SECRET_KEY: "simulated",
      },
    });
  try {
    write(
      servicePath,
      `[Unit]\nDescription=Simulated takeover runner\n[Service]\nType=oneshot\nExecStart=/usr/bin/touch ${replayed}\n`
    );
    write(
      timerPath,
      `[Unit]\nDescription=Simulated takeover timer\n[Timer]\nOnCalendar=daily\n[Install]\nWantedBy=timers.target\n`
    );
    check(run("systemctl", ["daemon-reload"]), "fixture daemon reload");
    check(run("systemctl", ["enable", "--now", timer]), "initial timer enable");
    const stagedRunner = path.join(staged, "payload/deploy/runner");
    fs.mkdirSync(stagedRunner, { recursive: true });
    const stagedFiles = [
      [poller, "payload/deploy/runner/poll-deploy-requests.sh"],
      [recoveryHelper, "payload/deploy/runner/recover-release-lineage.sh"],
      [
        holdSource,
        "payload/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf",
      ],
      [waiter, "payload/deploy/runner/wait-deploy-result.sh"],
    ];
    const manifestFiles = [];
    for (const [source, relative] of stagedFiles) {
      const target = path.join(staged, relative);
      write(target, fs.readFileSync(source), relative.endsWith(".sh") ? 0o755 : 0o644);
      manifestFiles.push({
        path: relative,
        sha256: sha256(fs.readFileSync(target)),
        size_bytes: fs.statSync(target).size,
      });
    }
    write(
      path.join(staged, "artifact-manifest.json"),
      JSON.stringify({
        schema_version: 1,
        target: "server-operator-files",
        files: manifestFiles,
      }) + "\n"
    );
    write(
      path.join(staged, "SHA256SUMS"),
      manifestFiles.map((item) => `${item.sha256}  ${item.path}\n`).join("")
    );
    const prepareFailed = launch("prepare", "reload-fail");
    assert.equal(
      prepareFailed.status,
      41,
      `daemon-reload injection: ${prepareFailed.stderr}`
    );
    assert.equal(fs.existsSync(hold), true, "daemon-reload failure cleared hold");
    assert.equal(
      check(
        run("systemctl", ["show", timer, "--property=UnitFileState", "--value"]),
        "timer state after reload failure"
      ).stdout.trim(),
      "disabled"
    );
    check(run("systemctl", ["daemon-reload"]), "load persistent guard after failure");

    const now = new Date().toISOString();
    const request = sign(
      {
        schema_version: 1,
        request_id: requestId,
        environment: "production",
        repository: "qintopia-agent-studio/qintopia-agent-os",
        requested_by: "fixture",
        created_at: now,
        expires_at: new Date(Date.parse(now) + 3600000).toISOString(),
        commit_sha: oSha,
        runtime_sha: pSha,
        deploy_bundle_sha: rSha,
        release_sha: tSha,
        runtime_artifact_profile: "huabaosi-production",
        release_scope: ["deploy-bundle"],
        restart_targets: ["qintopia-system-services"],
        rollback_on_smoke_failure: true,
        dry_run: false,
        cos: {
          bucket: "simulated",
          region: "simulated",
          prefix: "qintopia-agent-os",
          request_key: `qintopia-agent-os/deploy-requests/production/requests/${requestId}.json`,
          result_key: `qintopia-agent-os/deploy-results/production/${requestId}.json`,
        },
      },
      "github-actions",
      now,
      "request"
    );
    const result = sign(
      {
        schema_version: 1,
        request_id: requestId,
        environment: "production",
        status: "succeeded",
        started_at: now,
        finished_at: now,
        release_sha: tSha,
        commit_sha: oSha,
        runtime_sha: pSha,
        deploy_bundle_sha: rSha,
        runtime_artifact_profile: "huabaosi-production",
        release_scope: ["deploy-bundle"],
        restart_targets: ["qintopia-system-services"],
        previous_sha: oSha,
        current_target: path.join(releaseRoot, tSha),
        checks: [{ name: "deploy-runner", status: "passed" }],
        rollback: { attempted: false, status: "not_needed" },
      },
      "qintopia-deploy-runner",
      now,
      "result"
    );
    for (const sha of [oSha, tSha])
      fs.mkdirSync(path.join(releaseRoot, sha), { recursive: true });
    const oldManifest = { release_sha: oSha, previous_sha: pSha };
    const tManifest = {
      release_sha: tSha,
      previous_sha: oSha,
      request_id: requestId,
      release_scope: ["deploy-bundle"],
      restart_targets: ["qintopia-system-services"],
      commit_sha: oSha,
      runtime_sha: pSha,
      deploy_bundle_sha: rSha,
      runtime_artifact_profile: "huabaosi-production",
    };
    const oldManifestPath = path.join(releaseRoot, oSha, "manifest.json");
    const tManifestPath = path.join(releaseRoot, tSha, "manifest.json");
    write(oldManifestPath, JSON.stringify(oldManifest) + "\n");
    write(tManifestPath, JSON.stringify(tManifest) + "\n");
    fs.symlinkSync(path.join(releaseRoot, tSha), path.join(releaseRoot, "current"));
    fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "previous"));
    const requestPath = path.join(state, "requests/processed", `${requestId}.json`);
    const resultPath = path.join(state, "results", `${requestId}.json`);
    write(requestPath, JSON.stringify(request) + "\n", 0o600);
    write(resultPath, JSON.stringify(result) + "\n", 0o600);
    const takeoverRecord = JSON.parse(
      fs.readFileSync(path.join(recovery, "takeover.json"), "utf8")
    );
    assert.match(takeoverRecord.hold_token, /^[0-9a-f]{32}$/);
    takeoverRecord.request_id = requestId;
    write(
      path.join(recovery, "takeover.json"),
      JSON.stringify(takeoverRecord) + "\n",
      0o600
    );
    write(path.join(recovery, "takeover-consumed"), requestId + "\n", 0o600);
    write(
      path.join(recovery, `${requestId}.json`),
      JSON.stringify({
        direction: "O→T",
        phase: "intent",
        request_id: requestId,
        request_sha256: sha256(fs.readFileSync(requestPath)),
        hold_token: takeoverRecord.hold_token,
        original_current_sha: oSha,
        original_previous_sha: pSha,
        manifest_sha256: { current: sha256(fs.readFileSync(oldManifestPath)) },
      }) + "\n",
      0o600
    );
    const journalPath = path.join(recovery, `${requestId}.json`);
    const originalJournal = fs.readFileSync(journalPath, "utf8");
    write(
      journalPath,
      JSON.stringify({ ...JSON.parse(originalJournal), hold_token: "f".repeat(32) }) +
        "\n",
      0o600
    );
    const wrongJournalToken = launch("finalize");
    assert.equal(wrongJournalToken.status, 75, wrongJournalToken.stderr);
    assert.match(
      wrongJournalToken.stderr,
      /journal, request, smoke result, and pointers disagree/
    );
    assert.equal(fs.existsSync(hold), true);
    write(journalPath, originalJournal, 0o600);
    const failEnable = launch("finalize", "enable-fail");
    assert.equal(failEnable.status, 75, `timer enable failure: ${failEnable.stderr}`);
    assert.equal(fs.existsSync(hold), true, "timer enable failure cleared hold");
    const killedBeforeUnlink = launch("finalize", "kill-after-enable");
    assert.notEqual(killedBeforeUnlink.status, 0, "caller survived injected SIGKILL");
    assert.equal(fs.existsSync(hold), true, "caller death cleared hold before unlink");
    check(run("systemctl", ["start", service]), "held service start");
    assert.equal(
      fs.existsSync(replayed),
      false,
      "active timer bypassed recovery guard"
    );
    for (const lockName of ["poller.lock", "deploy.lock"]) {
      const lockPath = path.join(state, lockName);
      const ready = path.join(fixture, `${lockName}.ready`);
      const holder = spawn(
        "bash",
        ["-c", `exec 8>"${lockPath}"; flock 8; touch "${ready}"; sleep 30`],
        { detached: true, stdio: "ignore" }
      );
      try {
        for (let attempt = 0; attempt < 100 && !fs.existsSync(ready); attempt++)
          Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 20);
        assert.equal(fs.existsSync(ready), true, `${lockName} holder did not start`);
        const contested = launch("finalize");
        assert.equal(contested.status, 75, `${lockName}: ${contested.stderr}`);
        assert.equal(fs.readFileSync(hold, "utf8"), takeoverRecord.hold_token + "\n");
      } finally {
        process.kill(-holder.pid, "SIGKILL");
        assert.equal(
          spawnSync("flock", ["-w", "5", lockPath, "true"]).status,
          0,
          lockName + " holder did not release its lock"
        );
      }
    }
    check(launch("finalize"), "resume finalization without replay");
    assert.equal(fs.existsSync(hold), false, "successful finalization retained hold");
    assert.equal(fs.existsSync(replayed), false, "finalize replayed poller");
    assert.equal(
      check(run("systemctl", ["is-active", timer]), "restored timer").stdout.trim(),
      "active"
    );
    write(hold, takeoverRecord.hold_token + "\n", 0o600);
    const killedAfterUnlink = launch("finalize", "kill-after-unlink");
    assert.notEqual(
      killedAfterUnlink.status,
      0,
      "post-unlink SIGKILL was not injected"
    );
    assert.equal(fs.existsSync(hold), false, "post-unlink death restored stale hold");
    check(launch("finalize"), "idempotent finalization after caller death");
    assert.equal(fs.existsSync(replayed), false, "retry replayed poller");
    const laterId = "deploy-20260927T020304Z-abcdef1";
    const laterJournal = path.join(recovery, `${laterId}.json`);
    write(
      laterJournal,
      JSON.stringify({ request_id: laterId, direction: "T→R", phase: "unknown" }) +
        "\n",
      0o600
    );
    write(hold, laterId + "\n", 0o600);
    check(run("systemctl", ["disable", "--now", timer]), "later recovery hold");
    const staleFinalize = launch("finalize");
    assert.equal(staleFinalize.status, 75, staleFinalize.stderr);
    assert.equal(fs.readFileSync(hold, "utf8"), laterId + "\n");
    assert.equal(
      check(
        run("systemctl", ["show", timer, "--property=UnitFileState", "--value"]),
        "timer after stale finalization"
      ).stdout.trim(),
      "disabled"
    );
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "current")),
      path.join(releaseRoot, tSha)
    );
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "previous")),
      path.join(releaseRoot, oSha)
    );
    fs.rmSync(laterJournal);
    console.log("Fixed takeover finalization fault matrix passed.");
  } finally {
    run("systemctl", ["disable", "--now", timer]);
    for (const target of [
      servicePath,
      timerPath,
      dropin,
      path.join(recovery, "hold"),
      path.join(recovery, "takeover.json"),
      path.join(recovery, "takeover-consumed"),
      path.join(recovery, `${requestId}.json`),
      path.join(state, "requests/processed", `${requestId}.json`),
      path.join(state, "results", `${requestId}.json`),
    ])
      fs.rmSync(target, { force: true });
    fs.rmSync(staged, { recursive: true, force: true });
    fs.rmSync(releaseRoot, { recursive: true, force: true });
    run("systemctl", ["daemon-reload"]);
    fs.rmSync(fixture, { recursive: true, force: true });
  }
  process.exit(0);
}
if (process.argv[2] === "--recovery-negative") {
  const [recoverySource, waiterSource, holdSource] = process.argv.slice(3);
  for (const source of [recoverySource, waiterSource, holdSource])
    assert.ok(source && fs.existsSync(source), "recovery source is missing");
  const releaseRoot = "/home/ubuntu/qintopia-agent-os-releases";
  const state = "/var/lib/qintopia-agent-os-deploy";
  const unitDir = "/run/systemd/system";
  const dropin =
    "/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf";
  const envFile = "/etc/qintopia/cos-artifacts.env";
  const service = "qintopia-agent-os-deploy-runner.service";
  const timer = "qintopia-agent-os-deploy-runner.timer";
  const fixed = "qintopia-agent-os-fixed-takeover.service";
  const requestId = "deploy-20260927T010203Z-abcdef0";
  const oSha = "16e8d56b98001579c6288ba13199b80d6d3dfc74";
  const pSha = "83d694f2c3bc21fd78a73d25da3197379e2a14d5";
  const tSha = "70e7984fab92ddab956009585212d0e9729767b5";
  const rSha = "4".repeat(40);
  const signingKey = "simulated-recovery-key";
  const token = "a".repeat(32);
  const invocation = "b".repeat(32);
  const fixture = fs.mkdtempSync("/tmp/qintopia-recovery-negative-");
  const recovery = path.join(state, "recovery");
  const requestPath = path.join(state, "requests/pending", `${requestId}.json`);
  const processedPath = path.join(state, "requests/processed", `${requestId}.json`);
  const resultPath = path.join(state, "results", `${requestId}.json`);
  const reverseId = "deploy-20260927T020304Z-abcdef0";
  const reverseRequestPath = path.join(state, "requests/pending", `${reverseId}.json`);
  const reverseProcessedPath = path.join(
    state,
    "requests/processed",
    `${reverseId}.json`
  );
  const reverseResultPath = path.join(state, "results", `${reverseId}.json`);
  const reverseJournalPath = path.join(recovery, `${reverseId}.json`);
  const resultsDir = path.join(state, "results");
  const originalResultTemps = new Set(
    fs.existsSync(resultsDir)
      ? fs.readdirSync(resultsDir).filter((name) => name.startsWith(".signed-result-"))
      : []
  );
  const claimPath = path.join(state, "requests/claimed", `${requestId}.json`);
  const journalPath = path.join(recovery, `${requestId}.json`);
  const holdPath = path.join(recovery, "hold");
  const runnerPath = path.join(
    releaseRoot,
    oSha,
    "deploy/runner/qintopia-agent-os-deploy-runner"
  );
  const remoteRequest = path.join(fixture, "request.json");
  const remoteResult = path.join(fixture, "result.json");
  const serverConfig = path.join(fixture, "server.json");
  const serverPort = path.join(fixture, "port");
  const coscli = path.join(fixture, "coscli");
  const installMarker = path.join(fixture, "installed");
  const smokeMarker = path.join(fixture, "smoked");
  for (const target of [
    releaseRoot,
    requestPath,
    processedPath,
    claimPath,
    journalPath,
    holdPath,
    path.join(unitDir, service),
    path.join(unitDir, timer),
    path.join(unitDir, fixed),
    dropin,
    envFile,
  ])
    assert.equal(fs.existsSync(target), false, `fixture path exists: ${target}`);
  const write = (target, value, mode = 0o644) => {
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, value, { mode });
    fs.chmodSync(target, mode);
  };
  const digest = (value) => crypto.createHash("sha256").update(value).digest("hex");
  const canonical = (value) =>
    Array.isArray(value)
      ? `[${value.map(canonical).join(",")}]`
      : value && typeof value === "object"
        ? `{${Object.keys(value)
            .sort()
            .map((name) => `${JSON.stringify(name)}:${canonical(value[name])}`)
            .join(",")}}`
        : JSON.stringify(value);
  const now = new Date().toISOString();
  const sign = (unsigned, issuer, field) => {
    const metadata = {
      algorithm: "hmac-sha256",
      issuer,
      key_id: "simulated",
      signed_at: now,
    };
    return {
      ...unsigned,
      signature: {
        ...metadata,
        value: crypto
          .createHmac("sha256", signingKey)
          .update(canonical({ [field]: unsigned, signature: metadata }))
          .digest("hex"),
      },
    };
  };
  const unsigned = ({ signature: _signature, ...body }) => body;
  const request = sign(
    {
      schema_version: 1,
      request_id: requestId,
      environment: "production",
      repository: "qintopia-agent-studio/qintopia-agent-os",
      requested_by: "fixture",
      created_at: now,
      expires_at: new Date(Date.parse(now) + 3600000).toISOString(),
      commit_sha: oSha,
      runtime_sha: pSha,
      deploy_bundle_sha: rSha,
      release_sha: tSha,
      runtime_artifact_profile: "huabaosi-production",
      release_scope: ["deploy-bundle"],
      restart_targets: ["qintopia-system-services"],
      rollback_on_smoke_failure: true,
      dry_run: false,
      cos: {
        bucket: "simulated",
        region: "simulated",
        prefix: "qintopia-agent-os",
        request_key: `qintopia-agent-os/deploy-requests/production/requests/${requestId}.json`,
        result_key: `qintopia-agent-os/deploy-results/production/${requestId}.json`,
      },
    },
    "github-actions",
    "request"
  );
  const result = (status) =>
    sign(
      {
        schema_version: 1,
        request_id: requestId,
        environment: "production",
        status,
        started_at: now,
        finished_at: now,
        release_sha: tSha,
        commit_sha: oSha,
        runtime_sha: pSha,
        deploy_bundle_sha: rSha,
        runtime_artifact_profile: "huabaosi-production",
        release_scope: ["deploy-bundle"],
        restart_targets: ["qintopia-system-services"],
        previous_sha: oSha,
        current_target: path.join(releaseRoot, tSha),
        checks: [
          {
            name: "deploy-runner",
            status: status === "succeeded" ? "passed" : "failed",
          },
        ],
        rollback: { attempted: false, status: "not_needed" },
      },
      "qintopia-deploy-runner",
      "result"
    );
  const successBytes = `${JSON.stringify(result("succeeded"))}\n`;
  const failedBytes = `${JSON.stringify(result("failed"))}\n`;
  const serverCode = cosMockServerCode;
  let server;
  try {
    for (const sha of [pSha, oSha, tSha]) {
      const tree = path.join(releaseRoot, sha);
      const manifest = { release_sha: sha, previous_sha: sha === tSha ? oSha : pSha };
      if (sha === tSha)
        Object.assign(manifest, {
          request_id: requestId,
          commit_sha: oSha,
          runtime_sha: pSha,
          deploy_bundle_sha: rSha,
          runtime_artifact_profile: "huabaosi-production",
          release_scope: ["deploy-bundle"],
          restart_targets: ["qintopia-system-services"],
          dry_run: false,
        });
      write(path.join(tree, "manifest.json"), `${JSON.stringify(manifest)}\n`, 0o444);
      write(
        path.join(tree, "deploy/runner/install-release-systemd-units.sh"),
        `#!/bin/sh\ntouch ${installMarker}\n`,
        0o755
      );
      write(
        path.join(tree, "deploy/runner/smoke-release.sh"),
        `#!/bin/sh\ntouch ${smokeMarker}\n`,
        0o755
      );
    }
    write(runnerPath, "#!/bin/sh\nexit 0\n", 0o755);
    const helper = path.join(releaseRoot, tSha, "deploy/runner");
    fs.copyFileSync(recoverySource, path.join(helper, "recover-release-lineage.sh"));
    fs.writeFileSync(
      path.join(helper, "management-ui-lifecycle.sh"),
      managementUiLockProbe,
      { mode: 0o755 }
    );
    fs.chmodSync(path.join(helper, "management-ui-lifecycle.sh"), 0o755);
    fs.copyFileSync(waiterSource, path.join(helper, "wait-deploy-result.sh"));
    fs.chmodSync(path.join(helper, "wait-deploy-result.sh"), 0o755);
    const holdSourcePath = path.join(
      helper,
      "qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
    );
    fs.mkdirSync(path.dirname(holdSourcePath), { recursive: true });
    fs.copyFileSync(holdSource, holdSourcePath);
    write(path.join(helper, "rollback-release.sh"), "#!/bin/sh\nexit 99\n", 0o755);
    fs.symlinkSync(path.join(releaseRoot, tSha), path.join(releaseRoot, "current"));
    fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "previous"));
    write(requestPath, `${JSON.stringify(request)}\n`, 0o600);
    fs.copyFileSync(requestPath, remoteRequest);
    write(
      path.join(recovery, "takeover.json"),
      `${JSON.stringify({ request_id: requestId, hold_token: token })}\n`,
      0o600
    );
    write(path.join(recovery, "takeover-consumed"), `${requestId}\n`, 0o600);
    write(holdPath, `${token}\n`, 0o600);
    fs.mkdirSync(path.dirname(dropin), { recursive: true });
    fs.copyFileSync(holdSource, dropin);
    write(
      path.join(unitDir, service),
      "[Unit]\nDescription=Simulated runner\n[Service]\nType=oneshot\nExecStart=/usr/bin/true\n"
    );
    write(
      path.join(unitDir, timer),
      "[Unit]\nDescription=Simulated timer\n[Timer]\nOnCalendar=daily\n[Install]\nWantedBy=timers.target\n"
    );
    write(
      coscli,
      `#!/bin/bash\nset -euo pipefail\n[[ "$1" != cp ]] && exit 0\ncp "${remoteRequest}" "$3"\n`,
      0o755
    );
    write(
      serverConfig,
      JSON.stringify({ mode: "absent", key: request.cos.result_key })
    );
    server = spawn("python3", ["-c", serverCode, serverConfig, serverPort], {
      detached: true,
      stdio: "ignore",
    });
    for (let attempt = 0; attempt < 100 && !fs.existsSync(serverPort); attempt++)
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 20);
    assert.equal(fs.existsSync(serverPort), true, "mock COS server did not start");
    write(
      envFile,
      `export TENCENT_COS_BUCKET=simulated TENCENT_COS_REGION=simulated TENCENT_COS_SECRET_ID=simulated TENCENT_COS_SECRET_KEY=simulated DEPLOY_REQUEST_SIGNING_KEY=${signingKey} DEPLOY_REQUEST_SIGNING_KEY_ID=simulated COSCLI_PATH=${coscli} TENCENT_COS_ENDPOINT=http://127.0.0.1:${fs.readFileSync(serverPort, "utf8")}\n`,
      0o600
    );
    assert.equal(spawnSync("systemctl", ["daemon-reload"]).status, 0);
    const requestBytes = fs.readFileSync(requestPath);
    const baseClaim = {
      request_id: requestId,
      request_sha256: digest(requestBytes),
      phase: "possibly_executing",
      recovery_eligible: true,
      hold_token: token,
      execution: {
        path: runnerPath,
        sha256: digest(fs.readFileSync(runnerPath)),
        unit: fixed,
        invocation_id: invocation,
        unit_invocation_verified: true,
      },
      result_upload: { phase: "not_started" },
    };
    const baseJournal = {
      schema_version: 1,
      request_id: requestId,
      request_sha256: digest(requestBytes),
      direction: "O→T",
      original_current_sha: oSha,
      original_previous_sha: pSha,
      manifest_sha256: {
        current: digest(fs.readFileSync(path.join(releaseRoot, oSha, "manifest.json"))),
        previous: digest(
          fs.readFileSync(path.join(releaseRoot, pSha, "manifest.json"))
        ),
      },
      phase: "intent",
      execution: baseClaim.execution,
      hold_token: token,
      result_upload: { phase: "not_started" },
    };
    const setEvidence = (phase = "not_started", withClaim = true) => {
      const upload = { phase };
      write(
        journalPath,
        `${JSON.stringify({ ...baseJournal, result_upload: upload })}\n`,
        0o600
      );
      if (withClaim)
        write(
          claimPath,
          `${JSON.stringify({ ...baseClaim, result_upload: upload })}\n`,
          0o600
        );
      else fs.rmSync(claimPath, { force: true });
    };
    const setRemote = (mode, bytes = failedBytes) => {
      write(remoteResult, bytes, 0o600);
      write(
        serverConfig,
        JSON.stringify({ mode, key: request.cos.result_key, result: remoteResult })
      );
    };
    const launch = (extraEnv = {}, id = requestId) =>
      spawnSync(
        "bash",
        [
          ...(process.env.QINTOPIA_RECOVERY_TRACE ? ["-x"] : []),
          path.join(helper, "recover-release-lineage.sh"),
          "--request-id",
          id,
        ],
        { encoding: "utf8", env: { ...process.env, ...extraEnv } }
      );
    for (const [label, mode, phase, withClaim] of [
      ["unknown upload", "absent", "unknown", true],
      ["upload intent", "absent", "upload_intent", true],
      ["legacy claim", "absent", "not_started", false],
      ["authentication failure", "unreadable", "not_started", true],
      ["other 404", "other-404", "not_started", true],
      ["conflicting request", "absent", "not_started", true],
      ["local success and absent COS", "absent", "not_started", true],
      ["conflicting success", "success", "not_started", true],
    ]) {
      setEvidence(phase, withClaim);
      setRemote(mode, successBytes);
      fs.rmSync(resultPath, { force: true });
      fs.copyFileSync(requestPath, remoteRequest);
      if (label === "conflicting request")
        write(
          remoteRequest,
          `${JSON.stringify({ ...request, requested_by: "tampered" })}\n`
        );
      if (label === "local success and absent COS")
        write(resultPath, successBytes, 0o600);
      if (label === "conflicting success") write(resultPath, failedBytes, 0o600);
      const attempt = launch();
      assert.equal(attempt.status, 75, `${label}: ${attempt.stderr}`);
      assert.equal(
        fs.realpathSync(path.join(releaseRoot, "current")),
        path.join(releaseRoot, tSha)
      );
      assert.equal(
        fs.realpathSync(path.join(releaseRoot, "previous")),
        path.join(releaseRoot, oSha)
      );
      assert.equal(fs.readFileSync(holdPath, "utf8"), `${token}\n`);
    }
    setEvidence("not_started", false);
    setRemote("failed", failedBytes);
    write(resultPath, failedBytes, 0o600);
    fs.copyFileSync(requestPath, remoteRequest);
    fs.rmSync(path.join(releaseRoot, "current"));
    fs.rmSync(path.join(releaseRoot, "previous"));
    fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "current"));
    fs.symlinkSync(path.join(releaseRoot, pSha), path.join(releaseRoot, "previous"));
    const noClaimControl = launch();
    assert.equal(
      noClaimControl.status,
      0,
      `no-claim control: ${noClaimControl.stderr}`
    );
    assert.match(noClaimControl.stdout, /original pointers unchanged/);
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "current")),
      path.join(releaseRoot, oSha)
    );
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "previous")),
      path.join(releaseRoot, pSha)
    );
    assert.equal(fs.readFileSync(holdPath, "utf8"), `${token}\n`);
    assert.equal(fs.readFileSync(resultPath, "utf8"), failedBytes);
    assert.equal(fs.existsSync(installMarker), false);
    assert.equal(fs.existsSync(smokeMarker), false);
    const assertNoRecoveryEffects = (
      label,
      expectedError,
      expectedHold = `${token}\n`,
      prepare = () => {}
    ) => {
      setEvidence("not_started", false);
      prepare();
      const attempt = launch();
      assert.equal(attempt.status, 75, `${label}: ${attempt.stderr}`);
      assert.match(attempt.stderr, expectedError, `${label}: wrong rejection`);
      assert.equal(
        fs.realpathSync(path.join(releaseRoot, "current")),
        path.join(releaseRoot, oSha)
      );
      assert.equal(
        fs.realpathSync(path.join(releaseRoot, "previous")),
        path.join(releaseRoot, pSha)
      );
      assert.equal(fs.existsSync(installMarker), false, `${label}: installer ran`);
      assert.equal(fs.existsSync(smokeMarker), false, `${label}: smoke ran`);
      assert.equal(fs.readFileSync(holdPath, "utf8"), expectedHold);
      assert.equal(fs.readFileSync(resultPath, "utf8"), failedBytes);
      assert.equal(fs.existsSync(processedPath), false, `${label}: request archived`);
    };
    write(holdPath, `${"f".repeat(32)}\n`, 0o600);
    assertNoRecoveryEffects(
      "no-claim foreign hold",
      /legacy recovery hold belongs to another transaction/,
      `${"f".repeat(32)}\n`
    );
    write(holdPath, `${token}\n`, 0o600);
    assertNoRecoveryEffects(
      "no-claim unbound legacy journal",
      /legacy takeover hold identity conflicts/,
      `${token}\n`,
      () => {
        const { hold_token: _oldToken, ...unboundJournal } = baseJournal;
        write(journalPath, `${JSON.stringify(unboundJournal)}\n`, 0o600);
      }
    );
    const laterJournal = path.join(recovery, "deploy-20260927T020304Z-abcdef0.json");
    write(laterJournal, `${JSON.stringify({ request_id: "later" })}\n`, 0o600);
    assertNoRecoveryEffects(
      "no-claim later journal",
      /a later recovery journal owns the isolation state/
    );
    fs.rmSync(laterJournal);
    const otherClaim = path.join(
      state,
      "requests/claimed/deploy-20260927T020304Z-abcdef0.json"
    );
    write(otherClaim, "{}\n", 0o600);
    assertNoRecoveryEffects(
      "no-claim other claim",
      /another deploy request claim is present/
    );
    fs.rmSync(otherClaim);
    fs.rmSync(path.join(releaseRoot, "current"));
    fs.rmSync(path.join(releaseRoot, "previous"));
    fs.symlinkSync(path.join(releaseRoot, tSha), path.join(releaseRoot, "current"));
    fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "previous"));
    fs.copyFileSync(requestPath, remoteRequest);
    fs.rmSync(resultPath, { force: true });
    setEvidence();
    setRemote("absent");
    write(
      path.join(unitDir, fixed),
      "[Unit]\nDescription=Simulated active child\n[Service]\nType=simple\nExecStart=/usr/bin/sleep 30\n"
    );
    assert.equal(spawnSync("systemctl", ["daemon-reload"]).status, 0);
    assert.equal(spawnSync("systemctl", ["start", fixed]).status, 0);
    const activeChild = launch();
    assert.equal(activeChild.status, 75, activeChild.stderr);
    assert.equal(spawnSync("systemctl", ["stop", fixed]).status, 0);
    fs.rmSync(path.join(unitDir, fixed));
    assert.equal(spawnSync("systemctl", ["daemon-reload"]).status, 0);
    fs.rmSync(resultPath, { force: true });
    fs.rmSync(path.join(releaseRoot, "current"));
    fs.rmSync(path.join(releaseRoot, "previous"));
    fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "current"));
    fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "previous"));
    setEvidence();
    const absent = launch();
    if (absent.status !== 0 && process.env.QINTOPIA_RECOVERY_TRACE)
      console.error(absent.stderr);
    assert.equal(absent.status, 0, `missing result recovery: ${absent.stderr}`);
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "current")),
      path.join(releaseRoot, oSha)
    );
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "previous")),
      path.join(releaseRoot, pSha)
    );
    assert.equal(fs.existsSync(installMarker), true);
    assert.equal(fs.existsSync(smokeMarker), true);
    assert.equal(launch().status, 0, "completed recovery should be idempotent");
    for (const [phase, expected, installerRuns, smokeRuns] of [
      ["cas-previous-completed", 0, true, true],
      ["installer-completed", 0, false, true],
      ["installer-started", 75, false, false],
      ["smoke-started", 75, false, false],
      ["rollback-O-to-T-started", 75, false, false],
    ]) {
      setEvidence();
      write(
        journalPath,
        `${JSON.stringify({ ...baseJournal, recovery_phase: phase })}\n`,
        0o600
      );
      fs.rmSync(installMarker, { force: true });
      fs.rmSync(smokeMarker, { force: true });
      const resumed = launch();
      assert.equal(resumed.status, expected, `${phase}: ${resumed.stderr}`);
      assert.equal(fs.existsSync(installMarker), installerRuns, phase);
      assert.equal(fs.existsSync(smokeMarker), smokeRuns, phase);
      assert.equal(fs.readFileSync(holdPath, "utf8"), `${token}\n`);
    }
    fs.rmSync(path.join(releaseRoot, "current"));
    fs.rmSync(path.join(releaseRoot, "previous"));
    fs.symlinkSync(path.join(releaseRoot, tSha), path.join(releaseRoot, "current"));
    fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "previous"));
    setEvidence();
    setRemote("success", successBytes);
    const success = launch();
    assert.equal(success.status, 0, `signed success reconciliation: ${success.stderr}`);
    assert.deepEqual(fs.readFileSync(resultPath), Buffer.from(successBytes));
    assert.equal(fs.existsSync(processedPath), true);
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "current")),
      path.join(releaseRoot, tSha)
    );
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "previous")),
      path.join(releaseRoot, oSha)
    );
    assert.equal(
      launch().status,
      0,
      "signed success reconciliation should be idempotent"
    );
    fs.rmSync(claimPath, { force: true });
    const reverseTargets = [
      "qintopia-system-services",
      "hermes-erhua",
      "hermes-xiaoman",
      "hermes-silaoshi",
      "hermes-huabaosi",
      "hermes-anan",
    ];
    const reverseRequest = sign(
      {
        ...unsigned(request),
        request_id: reverseId,
        release_scope: ["deploy-bundle"],
        restart_targets: reverseTargets,
        release_rollback: { expected_current_sha: rSha, expected_previous_sha: tSha },
        cos: {
          ...request.cos,
          request_key: `qintopia-agent-os/deploy-requests/production/requests/${reverseId}.json`,
          result_key: `qintopia-agent-os/deploy-results/production/${reverseId}.json`,
        },
      },
      "github-actions",
      "request"
    );
    const reverseResult = sign(
      {
        ...unsigned(result("succeeded")),
        request_id: reverseId,
        restart_targets: reverseTargets,
      },
      "qintopia-deploy-runner",
      "result"
    );
    const reverseBytes = `${JSON.stringify(reverseResult)}\n`;
    write(
      path.join(releaseRoot, rSha, "manifest.json"),
      `${JSON.stringify({ release_sha: rSha, previous_sha: tSha })}\n`,
      0o444
    );
    write(reverseRequestPath, `${JSON.stringify(reverseRequest)}\n`, 0o600);
    fs.copyFileSync(reverseRequestPath, remoteRequest);
    write(
      reverseJournalPath,
      `${JSON.stringify({
        schema_version: 1,
        request_id: reverseId,
        request_sha256: digest(fs.readFileSync(reverseRequestPath)),
        direction: "R→T",
        original_current_sha: rSha,
        original_previous_sha: tSha,
        manifest_sha256: {
          current: digest(
            fs.readFileSync(path.join(releaseRoot, rSha, "manifest.json"))
          ),
          previous: digest(
            fs.readFileSync(path.join(releaseRoot, tSha, "manifest.json"))
          ),
        },
        hold_token: reverseId,
        phase: "intent",
      })}\n`,
      0o600
    );
    write(holdPath, `${reverseId}\n`, 0o600);
    write(remoteResult, reverseBytes, 0o600);
    write(
      serverConfig,
      JSON.stringify({
        mode: "success",
        key: reverseRequest.cos.result_key,
        result: remoteResult,
      })
    );
    fs.rmSync(installMarker, { force: true });
    fs.rmSync(smokeMarker, { force: true });
    const reverseLaunch = (extraEnv = {}) => launch(extraEnv, reverseId);
    const reverseNoEffects = (label, expectedStatus = 75, expectedError = "") => {
      const attempt = reverseLaunch();
      assert.equal(attempt.status, expectedStatus, `${label}: ${attempt.stderr}`);
      if (expectedError) assert.match(attempt.stderr, new RegExp(expectedError), label);
      assert.equal(
        fs.realpathSync(path.join(releaseRoot, "current")),
        path.join(releaseRoot, tSha)
      );
      assert.equal(
        fs.realpathSync(path.join(releaseRoot, "previous")),
        path.join(releaseRoot, oSha)
      );
      assert.equal(fs.existsSync(installMarker), false, `${label}: installer ran`);
      assert.equal(fs.existsSync(smokeMarker), false, `${label}: smoke ran`);
    };
    const reverseSuccess = reverseLaunch();
    assert.equal(reverseSuccess.status, 0, `R→T success: ${reverseSuccess.stderr}`);
    assert.deepEqual(fs.readFileSync(reverseResultPath), Buffer.from(reverseBytes));
    assert.equal(fs.existsSync(reverseProcessedPath), true);
    reverseNoEffects("R→T idempotent", 0);
    const resetReverse = () => {
      fs.rmSync(reverseResultPath, { force: true });
      if (fs.existsSync(reverseProcessedPath))
        fs.renameSync(reverseProcessedPath, reverseRequestPath);
      const journal = JSON.parse(fs.readFileSync(reverseJournalPath, "utf8"));
      delete journal.maintenance_evidence;
      write(reverseJournalPath, `${JSON.stringify(journal)}\n`, 0o600);
      fs.copyFileSync(reverseRequestPath, remoteRequest);
      write(remoteResult, reverseBytes, 0o600);
      write(
        serverConfig,
        JSON.stringify({
          mode: "success",
          key: reverseRequest.cos.result_key,
          result: remoteResult,
        })
      );
    };
    resetReverse();
    const wrongAction = sign(
      { ...unsigned(reverseResult), restart_targets: ["qintopia-system-services"] },
      "qintopia-deploy-runner",
      "result"
    );
    write(remoteResult, `${JSON.stringify(wrongAction)}\n`, 0o600);
    reverseNoEffects("R→T action targets conflict", 75, "restart_targets mismatch");
    resetReverse();
    const originalBytes = fs.readFileSync(resultPath);
    write(resultPath, failedBytes, 0o600);
    reverseNoEffects(
      "R→T original signed result conflicts",
      75,
      "Deploy result failed"
    );
    write(resultPath, originalBytes, 0o600);
    const faultDir = path.join(fixture, "fault-python");
    write(
      path.join(faultDir, "sitecustomize.py"),
      `import os, signal, tempfile
mode = os.environ.get("QINTOPIA_SIMULATED_RESULT_FAULT")
original_mkstemp = tempfile.mkstemp
def mkstemp(*args, **kwargs):
    descriptor, path = original_mkstemp(*args, **kwargs)
    if mode == "create" and os.path.basename(path).startswith(".signed-result-"):
        os.kill(os.getpid(), signal.SIGKILL)
    return descriptor, path
tempfile.mkstemp = mkstemp
original_fsync = os.fsync
def fsync(descriptor):
    try:
        path = os.readlink("/proc/self/fd/" + str(descriptor))
    except OSError:
        path = ""
    if mode == "write" and os.path.basename(path).startswith(".signed-result-"):
        os.ftruncate(descriptor, max(1, os.fstat(descriptor).st_size // 2))
        os.kill(os.getpid(), signal.SIGKILL)
    return original_fsync(descriptor)
os.fsync = fsync
`
    );
    for (const fault of ["create", "write"]) {
      resetReverse();
      const interrupted = reverseLaunch({
        PYTHONPATH: faultDir,
        QINTOPIA_SIMULATED_RESULT_FAULT: fault,
      });
      assert.equal(
        interrupted.status,
        75,
        `${fault} interruption: ${interrupted.stderr}`
      );
      assert.equal(
        fs.existsSync(reverseResultPath),
        false,
        `${fault}: formal result was published`
      );
      assert.equal(fs.existsSync(installMarker), false, `${fault}: installer ran`);
      assert.equal(fs.existsSync(smokeMarker), false, `${fault}: smoke ran`);
      const retried = reverseLaunch();
      assert.equal(retried.status, 0, `${fault} retry: ${retried.stderr}`);
      assert.deepEqual(fs.readFileSync(reverseResultPath), Buffer.from(reverseBytes));
    }
    resetReverse();
    const conflictingLocal = `${JSON.stringify(
      sign(
        { ...unsigned(reverseResult), status: "failed" },
        "qintopia-deploy-runner",
        "result"
      )
    )}\n`;
    write(reverseResultPath, conflictingLocal, 0o600);
    reverseNoEffects("R→T existing different signed result");
    assert.equal(fs.readFileSync(reverseResultPath, "utf8"), conflictingLocal);
    console.log(
      "Recovery missing-result, hold ownership, R→T success and result interruption matrix passed."
    );
  } finally {
    spawnSync("systemctl", ["stop", fixed]);
    spawnSync("systemctl", ["disable", "--now", timer]);
    for (const target of [
      path.join(unitDir, fixed),
      path.join(unitDir, service),
      path.join(unitDir, timer),
      dropin,
      envFile,
    ])
      fs.rmSync(target, { force: true });
    fs.rmSync(releaseRoot, { recursive: true, force: true });
    for (const target of [
      requestPath,
      processedPath,
      resultPath,
      reverseRequestPath,
      reverseProcessedPath,
      reverseResultPath,
      reverseJournalPath,
      claimPath,
      journalPath,
      holdPath,
      path.join(recovery, "takeover.json"),
      path.join(recovery, "takeover-consumed"),
    ])
      fs.rmSync(target, { force: true });
    if (fs.existsSync(resultsDir))
      for (const name of fs.readdirSync(resultsDir))
        if (name.startsWith(".signed-result-") && !originalResultTemps.has(name))
          fs.rmSync(path.join(resultsDir, name), { force: true });
    spawnSync("systemctl", ["daemon-reload"]);
    if (server) process.kill(-server.pid, "SIGKILL");
    fs.rmSync(fixture, { recursive: true, force: true });
  }
  process.exit(0);
}
if (process.argv[2] === "--fixed-takeover-lock") {
  const [oldRunnerSource, pollerSource, waiterSource] = process.argv.slice(3);
  for (const source of [oldRunnerSource, pollerSource, waiterSource]) {
    assert.ok(source && fs.existsSync(source), "fixed takeover source is missing");
  }
  const releaseRoot = "/home/ubuntu/qintopia-agent-os-releases";
  const oSha = "16e8d56b98001579c6288ba13199b80d6d3dfc74";
  const pSha = "83d694f2c3bc21fd78a73d25da3197379e2a14d5";
  const tSha = "70e7984fab92ddab956009585212d0e9729767b5";
  const rSha = "4".repeat(40);
  const key = "simulated-fixed-takeover-key";
  assert.equal(
    fs.existsSync(releaseRoot),
    false,
    "simulated release root already exists"
  );
  const fixture = fs.mkdtempSync("/tmp/qintopia-fixed-takeover-");
  const run = (command, args, options = {}) =>
    spawnSync(command, args, { encoding: "utf8", ...options });
  const waitFor = (target, timeout = 5000, errorLog = "") => {
    const deadline = Date.now() + timeout;
    while (!fs.existsSync(target) && Date.now() < deadline) {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 50);
    }
    assert.ok(
      fs.existsSync(target),
      "stage not reached: " +
        target +
        (errorLog && fs.existsSync(errorLog)
          ? "\n" + fs.readFileSync(errorLog, "utf8")
          : "")
    );
  };
  const canonical = (value) =>
    Array.isArray(value)
      ? `[${value.map(canonical).join(",")}]`
      : value && typeof value === "object"
        ? `{${Object.keys(value)
            .sort()
            .map((name) => `${JSON.stringify(name)}:${canonical(value[name])}`)
            .join(",")}}`
        : JSON.stringify(value);
  const write = (target, value, mode = 0o644) => {
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, value, { mode });
    fs.chmodSync(target, mode);
  };
  const oldRunner = path.join(
    releaseRoot,
    oSha,
    "deploy/runner/qintopia-agent-os-deploy-runner"
  );
  const coscli = path.join(fixture, "coscli");
  const unrelatedLock = path.join(fixture, "unrelated.lock");
  let unrelatedHolder;
  let transientUnit = false;
  try {
    for (const sha of [oSha, pSha]) {
      write(
        path.join(releaseRoot, sha, "manifest.json"),
        `${JSON.stringify({ release_sha: sha, previous_sha: sha === oSha ? pSha : "0".repeat(40) })}\n`
      );
    }
    fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "current"));
    fs.symlinkSync(path.join(releaseRoot, pSha), path.join(releaseRoot, "previous"));
    fs.mkdirSync(path.dirname(oldRunner), { recursive: true });
    fs.copyFileSync(oldRunnerSource, oldRunner);
    fs.chmodSync(oldRunner, 0o755);
    const runnerDir = path.dirname(oldRunner);
    fs.copyFileSync(waiterSource, path.join(runnerDir, "wait-deploy-result.sh"));
    fs.chmodSync(path.join(runnerDir, "wait-deploy-result.sh"), 0o755);
    const quiesceStarted = path.join(fixture, "quiesce-started");
    write(
      path.join(runnerDir, "quiesce-space-automation-runtime.sh"),
      `#!/bin/bash\necho started >"${quiesceStarted}"\nsleep 2\n`,
      0o755
    );
    write(
      path.join(runnerDir, "promote-release.sh"),
      `#!/bin/bash\nset -euo pipefail\nexec 9>"$QINTOPIA_UNRELATED_LOCK"\nif flock -n 9; then echo redirected >"$QINTOPIA_UNRELATED_RESULT"; else echo blocked >"$QINTOPIA_UNRELATED_RESULT"; fi\necho "$PPID" >"$QINTOPIA_RUNNER_PID_FILE"\necho started >"$QINTOPIA_PROMOTER_STARTED"\nsleep "$QINTOPIA_PROMOTE_SLEEP"\nexit 42\n`,
      0o755
    );
    write(
      coscli,
      `#!/bin/bash\nset -euo pipefail\n[[ "$1" != cp ]] && exit 0\nif [[ "$2" == cos://* ]]; then\n  case "$2" in\n    *deploy-requests/production/current.json) cp "$QINTOPIA_POINTER_FILE" "$3" ;;\n    *deploy-requests/production/requests/*) cp "$QINTOPIA_REQUEST_FILE" "$3" ;;\n    *deploy-results/production/*) if [[ -f "$QINTOPIA_REMOTE_RESULT" ]]; then cp "$QINTOPIA_REMOTE_RESULT" "$3"; else echo NoSuchKey >&2; exit 1; fi ;;\n    *) exit 64 ;;\n  esac\nelse\n  [[ "$3" == cos://*deploy-results/production/* ]] || exit 64\n  echo uploading >"$QINTOPIA_UPLOAD_STARTED"\n  sleep "$QINTOPIA_UPLOAD_SLEEP"\n  cp "$2" "$QINTOPIA_REMOTE_RESULT"\nfi\n`,
      0o755
    );
    unrelatedHolder = spawn("flock", ["-n", unrelatedLock, "sleep", "90"], {
      stdio: "ignore",
    });
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 150);
    assert.notEqual(run("flock", ["-n", unrelatedLock, "true"]).status, 0);
    const phases = [
      "drift",
      "journal-start",
      "parent-death",
      "runner-kill",
      "upload-gap",
      "cgroup-stop",
    ];
    for (const phase of phases) {
      fs.rmSync(quiesceStarted, { force: true });
      const state = path.join(fixture, phase, "state");
      const requestId = `deploy-20260927T02030${4 + phases.indexOf(phase)}Z-abcdef${phases.indexOf(phase)}`;
      const started = path.join(fixture, phase, "promoter-started");
      const upload = path.join(fixture, phase, "upload-started");
      const unrelated = path.join(fixture, phase, "unrelated-result");
      const runnerPid = path.join(fixture, phase, "runner-pid");
      const pointer = path.join(fixture, phase, "pointer.json");
      const requestFile = path.join(fixture, phase, "request.json");
      const remoteResult = path.join(fixture, phase, "remote-result.json");
      const now = new Date().toISOString();
      const cos = {
        bucket: "simulated",
        region: "simulated",
        prefix: "qintopia-agent-os",
        request_key: `qintopia-agent-os/deploy-requests/production/requests/${requestId}.json`,
        result_key: `qintopia-agent-os/deploy-results/production/${requestId}.json`,
      };
      const unsigned = {
        schema_version: 1,
        request_id: requestId,
        environment: "production",
        repository: "qintopia-agent-studio/qintopia-agent-os",
        requested_by: "fixture",
        created_at: now,
        expires_at: new Date(Date.parse(now) + 3600000).toISOString(),
        commit_sha: oSha,
        runtime_sha: pSha,
        deploy_bundle_sha: rSha,
        release_sha: tSha,
        runtime_artifact_profile: "huabaosi-production",
        release_scope: ["deploy-bundle"],
        restart_targets: ["qintopia-system-services"],
        rollback_on_smoke_failure: true,
        dry_run: false,
        cos,
      };
      const metadata = {
        algorithm: "hmac-sha256",
        issuer: "github-actions",
        key_id: "simulated",
        signed_at: now,
      };
      const request = {
        ...unsigned,
        signature: {
          ...metadata,
          value: crypto
            .createHmac("sha256", key)
            .update(canonical({ request: unsigned, signature: metadata }))
            .digest("hex"),
        },
      };
      write(requestFile, `${JSON.stringify(request)}\n`);
      write(
        pointer,
        `${JSON.stringify({
          schema_version: 1,
          environment: "production",
          repository: unsigned.repository,
          request_id: requestId,
          request_key: cos.request_key,
          result_key: cos.result_key,
        })}\n`
      );
      const holdToken = crypto.randomBytes(16).toString("hex");
      write(
        path.join(state, "recovery", "takeover.json"),
        JSON.stringify({ request_id: requestId, hold_token: holdToken }) + "\n",
        0o600
      );
      write(path.join(state, "recovery", "hold"), holdToken + "\n", 0o600);
      const env = {
        ...process.env,
        QINTOPIA_COS_ENV_FILE: path.join(fixture, "missing.env"),
        QINTOPIA_DEPLOY_RUNNER_STATE_DIR: state,
        QINTOPIA_DEPLOY_RUNNER_BIN: oldRunner,
        QINTOPIA_EXPECTED_DEPLOY_REQUEST_ID: requestId,
        COSCLI_PATH: coscli,
        TENCENT_COS_BUCKET: "simulated",
        TENCENT_COS_REGION: "simulated",
        TENCENT_COS_SECRET_ID: "simulated",
        TENCENT_COS_SECRET_KEY: "simulated",
        DEPLOY_REQUEST_SIGNING_KEY: key,
        DEPLOY_REQUEST_SIGNING_KEY_ID: "simulated",
        QINTOPIA_POINTER_FILE: pointer,
        QINTOPIA_REQUEST_FILE: requestFile,
        QINTOPIA_REMOTE_RESULT: remoteResult,
        QINTOPIA_UPLOAD_STARTED: upload,
        QINTOPIA_UPLOAD_SLEEP: phase === "upload-gap" ? "4" : "0",
        QINTOPIA_PROMOTER_STARTED: started,
        QINTOPIA_PROMOTE_SLEEP: ["parent-death", "runner-kill", "cgroup-stop"].includes(
          phase
        )
          ? "6"
          : "0",
        QINTOPIA_RUNNER_PID_FILE: runnerPid,
        QINTOPIA_UNRELATED_LOCK: unrelatedLock,
        QINTOPIA_UNRELATED_RESULT: unrelated,
        QINTOPIA_RELEASE_ROOT: releaseRoot,
      };
      if (phase === "drift") {
        fs.appendFileSync(oldRunner, "\n# simulated drift\n");
        const rejected = run("bash", [pollerSource], { env });
        assert.equal(rejected.status, 75, rejected.stderr);
        assert.match(rejected.stderr, /old runner identity mismatch/);
        assert.equal(fs.existsSync(started), false);
        assert.equal(
          fs.existsSync(path.join(state, "recovery", `${requestId}.json`)),
          false
        );
        fs.copyFileSync(oldRunnerSource, oldRunner);
        fs.chmodSync(oldRunner, 0o755);
        continue;
      }
      if (phase === "cgroup-stop") {
        const environment = path.join(fixture, phase, "environment");
        write(
          environment,
          Object.entries(env)
            .filter(
              ([name]) =>
                name.startsWith("QINTOPIA_") ||
                name.startsWith("TENCENT_") ||
                name.startsWith("DEPLOY_") ||
                name === "COSCLI_PATH"
            )
            .map(([name, value]) => `${name}=${value}`)
            .join("\n") + "\n",
          0o600
        );
        const unitName = "qintopia-agent-os-fixed-takeover.service";
        run("systemctl", ["reset-failed", unitName]);
        const launched = run("systemd-run", [
          "--unit",
          unitName,
          "--service-type=oneshot",
          "--no-block",
          "--uid=root",
          "--gid=root",
          `--property=EnvironmentFile=${environment}`,
          "/bin/bash",
          pollerSource,
        ]);
        assert.equal(launched.status, 0, launched.stderr);
        transientUnit = true;
        waitFor(started);
        const deployLock = path.join(state, "deploy.lock");
        assert.notEqual(run("flock", ["-n", deployLock, "true"]).status, 0);
        const stopped = run("systemctl", ["stop", unitName]);
        assert.equal(stopped.status, 0, stopped.stderr);
        assert.equal(run("flock", ["-n", deployLock, "true"]).status, 0);
        assert.equal(
          fs.existsSync(path.join(state, "requests", "claimed", `${requestId}.json`)),
          true
        );
        assert.equal(fs.existsSync(path.join(state, "recovery", "hold")), true);
        const retry = run("bash", [pollerSource], { env });
        assert.equal(retry.status, 75, retry.stderr);
        assert.match(retry.stderr, /unfinished deploy request claim/);
        run("systemctl", ["reset-failed", unitName]);
        transientUnit = false;
        continue;
      }
      const errorLog = path.join(fixture, phase, "poller.stderr");
      const errorFd = fs.openSync(errorLog, "w");
      const child = spawn("bash", [pollerSource], {
        env,
        stdio: ["ignore", "pipe", errorFd],
      });
      fs.closeSync(errorFd);
      let stdout = "",
        stderr = "";
      child.stdout.on("data", (chunk) => {
        stdout += chunk;
      });
      child.on("close", () => {
        stderr = fs.readFileSync(errorLog, "utf8");
      });
      if (phase === "journal-start") {
        waitFor(quiesceStarted, 5000, errorLog);
        assert.equal(
          fs.existsSync(path.join(state, "recovery", `${requestId}.json`)),
          true
        );
        assert.equal(fs.existsSync(started), false);
        assert.notEqual(
          run("flock", ["-n", path.join(state, "deploy.lock"), "true"]).status,
          0,
          "deploy lock was free after journal before promotion"
        );
      }
      waitFor(started, 5000, errorLog);
      assert.equal(
        fs.readFileSync(unrelated, "utf8").trim(),
        "blocked",
        "unrelated child flock was redirected to deploy.lock"
      );
      const deployLock = path.join(state, "deploy.lock");
      assert.notEqual(run("flock", ["-n", deployLock, "true"]).status, 0);
      if (phase === "parent-death") {
        child.kill("SIGKILL");
        assert.notEqual(
          run("flock", ["-n", deployLock, "true"]).status,
          0,
          "runner lost inherited lock when poller parent died"
        );
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 6600);
        assert.equal(
          run("flock", ["-n", deployLock, "true"]).status,
          0,
          "inherited lock remained after runner exited"
        );
        assert.equal(
          fs.existsSync(path.join(state, "requests", "claimed", `${requestId}.json`)),
          true
        );
        assert.equal(fs.existsSync(path.join(state, "recovery", "hold")), true);
        const retry = run("bash", [pollerSource], { env });
        assert.equal(retry.status, 75, retry.stderr);
        assert.match(retry.stderr, /unfinished deploy request claim/);
      } else if (phase === "runner-kill") {
        process.kill(Number(fs.readFileSync(runnerPid, "utf8").trim()), "SIGKILL");
        assert.notEqual(
          run("flock", ["-n", deployLock, "true"]).status,
          0,
          "promoter lost inherited lock when old runner died"
        );
        const completed = await new Promise((resolve) => child.on("close", resolve));
        assert.notEqual(completed, 0, `${stdout}\n${stderr}`);
        assert.equal(
          fs.existsSync(path.join(state, "requests", "claimed", `${requestId}.json`)),
          true
        );
        assert.equal(fs.existsSync(path.join(state, "recovery", "hold")), true);
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 6600);
        assert.equal(run("flock", ["-n", deployLock, "true"]).status, 0);
        const retry = run("bash", [pollerSource], { env });
        assert.equal(retry.status, 75, retry.stderr);
        assert.match(retry.stderr, /unfinished deploy request claim/);
      } else if (phase === "journal-start") {
        const completed = await new Promise((resolve) => child.on("close", resolve));
        assert.equal(completed, 42, `${stdout}\n${stderr}`);
        assert.equal(run("flock", ["-n", deployLock, "true"]).status, 0);
      } else {
        waitFor(upload, 6000);
        assert.notEqual(
          run("flock", ["-n", deployLock, "true"]).status,
          0,
          "deploy lock released before upload/archive completed"
        );
        const completed = await new Promise((resolve) => child.on("close", resolve));
        assert.equal(completed, 42, `${stdout}\n${stderr}`);
        assert.equal(run("flock", ["-n", deployLock, "true"]).status, 0);
        assert.equal(
          fs.existsSync(path.join(state, "requests", "claimed", `${requestId}.json`)),
          false
        );
        assert.equal(
          fs.existsSync(path.join(state, "requests", "failed", `${requestId}.json`)),
          true
        );
      }
    }
    const adapter = fs
      .readFileSync(pollerSource, "utf8")
      .match(/^    flock\(\) \{[\s\S]*?^    \}/m)?.[0];
    assert.ok(adapter, "fixed poller lock adapter was not found");
    const adapterState = path.join(fixture, "adapter-state");
    fs.mkdirSync(adapterState, { recursive: true });
    const adapterLock = path.join(adapterState, "deploy.lock");
    const adapterEnv = {
      ...process.env,
      QINTOPIA_FIXED_TAKEOVER_LOCK: "1",
      QINTOPIA_DEPLOY_RUNNER_BIN: oldRunner,
      QINTOPIA_DEPLOY_RUNNER_STATE_DIR: adapterState,
      fixed_runner: oldRunner,
      fixed_runner_sha256: crypto
        .createHash("sha256")
        .update(fs.readFileSync(oldRunner))
        .digest("hex"),
    };
    const adapterCall = (setup, args = "-n 9", runnerName = oldRunner) =>
      run("bash", ["-c", `${setup}\n${adapter}\nflock ${args}`, runnerName], {
        env: adapterEnv,
      });
    const missingFd = adapterCall("");
    assert.equal(missingFd.status, 75, `missing FD7 was accepted: ${missingFd.stderr}`);
    assert.equal(
      adapterCall(`exec 7>"${unrelatedLock}"`).status,
      75,
      "wrong FD7 inode was accepted"
    );
    assert.equal(
      adapterCall(`exec 7>"${adapterLock}"; command flock -n 7`).status,
      0,
      "correct inherited FD7 was rejected"
    );
    assert.notEqual(
      adapterCall(
        `exec 7>"${adapterLock}"; command flock -n 7; exec 8>"${unrelatedLock}"`,
        "-n 8"
      ).status,
      0,
      "other flock arguments were redirected"
    );
    assert.notEqual(
      adapterCall(
        `exec 7>"${adapterLock}"; command flock -n 7; exec 9>"${unrelatedLock}"`,
        "-n 9",
        "/tmp/unrelated-script"
      ).status,
      0,
      "unrelated script flock was redirected"
    );
    fs.appendFileSync(oldRunner, "\n# simulated post-check drift\n");
    assert.equal(
      adapterCall(`exec 7>"${adapterLock}"; command flock -n 7`).status,
      75,
      "post-check runner drift was accepted"
    );
    fs.copyFileSync(oldRunnerSource, oldRunner);
    console.log("Exact old-runner fixed takeover lock handoff passed.");
  } finally {
    if (transientUnit) {
      run("systemctl", ["stop", "qintopia-agent-os-fixed-takeover.service"]);
      run("systemctl", ["reset-failed", "qintopia-agent-os-fixed-takeover.service"]);
    }
    unrelatedHolder?.kill("SIGKILL");
    fs.rmSync(releaseRoot, { recursive: true, force: true });
    fs.rmSync(fixture, { recursive: true, force: true });
  }
  process.exit(0);
}
const osRelease = fs.readFileSync("/etc/os-release", "utf8");
assert.match(osRelease, /VERSION_ID="24\.04"/);
fs.mkdirSync("/var/lib/qintopia-agent-os-deploy", { recursive: true });
const root = fs.mkdtempSync("/var/lib/qintopia-agent-os-deploy/systemd-linux-");
fs.chmodSync(root, 0o755);
const profile = "/home/ubuntu/.hermes/profiles/anan";
const otherProfile = "/home/ubuntu/.hermes/profiles/erhua";
const unit = "qintopia-agent-os-anan-drain-restart.service";
const deployUnit = "qintopia-agent-os-deploy-runner.service";
const managementUiProbeUnit = "qintopia-agentos-management-ui-drain-probe.service";
const state = path.join(root, "state");
const marker = path.join(profile, ".drain_request.json");
const completion = path.join(profile, `.qintopia-${path.basename(root)}-finished`);
let serviceCreated = false;
let holdCreated = false;
let releaseFixtureCreated = false;
let installedHoldDropin = false;
let timerCreated = false;
let recoveryArtifactsCreated = false;
let recoveryArtifactPaths = [];
let cosEnvCreated = false;
let managementUiProbeStarted = false;
let cosServer;
const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, { encoding: "utf8", ...options });
  if (result.error) throw result.error;
  return result;
};
const requireSuccess = (command, args, options) => {
  const result = run(command, args, options);
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed: ${result.stderr}`);
  }
  return result.stdout;
};
const sleep = (milliseconds) =>
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
const show = (name, property) =>
  requireSuccess("systemctl", [
    "show",
    name,
    `--property=${property}`,
    "--value",
  ]).trim();
const write = (file, content, mode = 0o644) => {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, content, { mode });
  fs.chmodSync(file, mode);
};
const baseProperties = [
  "--unit=qintopia-agent-os-anan-drain-restart.service",
  "--service-type=oneshot",
  "--wait",
  "--uid=ubuntu",
  "--gid=ubuntu",
  "--property=TimeoutStartSec=infinity",
  "--property=NoNewPrivileges=yes",
  "--property=PrivateTmp=yes",
  "--property=ProtectSystem=strict",
  "--property=ProtectHome=read-only",
  `--property=ReadWritePaths=${profile}`,
  `--property=Environment=XDG_RUNTIME_DIR=/run/user/${run("id", ["-u", "ubuntu"]).stdout.trim()}`,
];

try {
  assert.equal(
    run("id", ["-u", "ubuntu"]).status,
    0,
    "VM needs a simulated ubuntu user"
  );
  const managementUiProbe = path.join(root, "management-ui-drain-probe.py");
  write(
    managementUiProbe,
    `import signal, sys, time
def drain(_signal, _frame):
    print("management_ui_drain_complete", flush=True)
    sys.exit(0)
signal.signal(signal.SIGTERM, drain)
while True: time.sleep(0.1)
`,
    0o644
  );
  const managementUiProbeUnitPath = `/run/systemd/system/${managementUiProbeUnit}`;
  assert.equal(fs.existsSync(managementUiProbeUnitPath), false);
  write(
    managementUiProbeUnitPath,
    `[Unit]
Description=Simulated management UI drain probe
[Service]
Type=simple
ExecStart=/usr/bin/python3 ${managementUiProbe}
Restart=no
KillSignal=SIGTERM
KillMode=control-group
TimeoutStopSec=35s
SendSIGKILL=no
`,
    0o644
  );
  managementUiProbeStarted = true;
  requireSuccess("systemctl", ["daemon-reload"]);
  requireSuccess("systemctl", ["start", managementUiProbeUnit]);
  for (
    let attempt = 0;
    attempt < 40 && show(managementUiProbeUnit, "ActiveState") !== "active";
    attempt++
  )
    sleep(100);
  assert.equal(show(managementUiProbeUnit, "ActiveState"), "active");
  const managementUiInvocation = show(managementUiProbeUnit, "InvocationID");
  assert.match(managementUiInvocation, /^[0-9a-f]{32}$/);
  requireSuccess("systemctl", ["stop", managementUiProbeUnit]);
  assert.equal(show(managementUiProbeUnit, "ActiveState"), "inactive");
  assert.equal(show(managementUiProbeUnit, "InvocationID"), "");
  const stopEvent = JSON.parse(
    requireSuccess("journalctl", [
      "--no-pager",
      "-b",
      "-n",
      "1",
      "-o",
      "json",
      `UNIT=${managementUiProbeUnit}`,
    ])
      .trim()
      .split("\n")
      .at(-1)
  );
  assert.equal(stopEvent.INVOCATION_ID, managementUiInvocation);
  assert.equal(stopEvent.JOB_TYPE, "stop");
  assert.equal(stopEvent.JOB_RESULT, "done");
  assert.equal(show(managementUiProbeUnit, "Result"), "success");
  assert.equal(show(managementUiProbeUnit, "ExecMainStatus"), "0");
  assert.equal(show(managementUiProbeUnit, "NRestarts"), "0");
  if (fs.existsSync(marker))
    throw new Error("preexisting Anan marker must not be removed");
  requireSuccess("install", [
    "-d",
    "-o",
    "ubuntu",
    "-g",
    "ubuntu",
    "-m",
    "0700",
    profile,
  ]);
  requireSuccess("install", [
    "-d",
    "-o",
    "ubuntu",
    "-g",
    "ubuntu",
    "-m",
    "0700",
    otherProfile,
  ]);
  const markerHelper = path.join(root, "marker-helper.py");
  write(
    markerHelper,
    `import errno, json, os, tempfile\nfrom pathlib import Path\nprofile = Path(${JSON.stringify(profile)})\nfd, temporary = tempfile.mkstemp(prefix=".drain-test-", dir=profile)\nwith os.fdopen(fd, "w") as file:\n    json.dump({"simulated": True}, file)\n    file.flush(); os.fsync(file.fileno())\nos.replace(temporary, profile / ".drain_request.json")\ntry:\n    (Path(${JSON.stringify(otherProfile)}) / "forbidden").write_text("fail")\nexcept OSError as error:\n    if error.errno not in (errno.EROFS, errno.EACCES): raise\nelse:\n    raise SystemExit("sandbox allowed writing another Profile")\n(profile / ".drain_request.json").unlink()\n`,
    0o644
  );
  const first = run("systemd-run", [
    ...baseProperties,
    "/usr/bin/python3",
    markerHelper,
  ]);
  if (first.status !== 0)
    throw new Error(`simulated marker helper failed: ${first.stderr}`);
  const firstOutput = first.stdout + first.stderr;
  const invocation = firstOutput.match(/invocation ID: ([0-9a-f]{32})/)?.[1];
  assert.ok(invocation, "--wait must return InvocationID");
  assert.match(firstOutput, /Finished with result: success/);
  assert.match(firstOutput, /Main processes terminated with: code=exited\/status=0/);
  assert.equal(
    fs.existsSync(marker),
    false,
    "helper must clear only its simulated marker"
  );
  assert.equal(fs.existsSync(path.join(otherProfile, "forbidden")), false);

  const longHelper = path.join(root, "long-helper.py");
  write(
    longHelper,
    `import os, time\nfrom pathlib import Path\ntime.sleep(4)\npath = Path(${JSON.stringify(completion)})\nwith path.open("w") as file:\n    file.write(os.environ["INVOCATION_ID"] + "\\n")\n    file.flush(); os.fsync(file.fileno())\nprint("SIMULATED_HELPER_FINISHED", flush=True)\n`,
    0o644
  );
  const client = spawn(
    "systemd-run",
    [...baseProperties, "/usr/bin/python3", longHelper],
    {
      stdio: ["ignore", "pipe", "pipe"],
    }
  );
  let active = false;
  for (let attempt = 0; attempt < 40; attempt++) {
    if (show(unit, "ActiveState") === "activating") {
      active = true;
      break;
    }
    sleep(100);
  }
  assert.ok(active, "long helper never became active");
  assert.equal(show(unit, "User"), "ubuntu");
  assert.equal(show(unit, "Group"), "ubuntu");
  assert.equal(show(unit, "Type"), "oneshot");
  assert.equal(show(unit, "TimeoutStartUSec"), "infinity");
  assert.equal(show(unit, "NoNewPrivileges"), "yes");
  assert.equal(show(unit, "PrivateTmp"), "yes");
  assert.equal(show(unit, "ProtectSystem"), "strict");
  assert.equal(show(unit, "ProtectHome"), "read-only");
  assert.equal(show(unit, "ReadWritePaths"), profile);
  const activeInvocation = show(unit, "InvocationID");
  assert.match(activeInvocation, /^[0-9a-f]{32}$/);
  const busy = run("systemd-run", [...baseProperties, "/usr/bin/true"]);
  assert.notEqual(busy.status, 0, "StartTransientUnit=fail must reject the busy name");
  client.kill("SIGKILL");
  assert.equal(
    show(unit, "InvocationID"),
    activeInvocation,
    "caller death restarted helper"
  );
  sleep(4300);
  assert.notEqual(
    show(unit, "ActiveState"),
    "activating",
    "helper did not finish naturally"
  );
  assert.notEqual(run("systemctl", ["is-active", "--quiet", unit]).status, 0);
  assert.equal(
    fs.readFileSync(completion, "utf8").trim(),
    activeInvocation,
    "helper did not persist completion for the original invocation"
  );
  const invocationJournal = requireSuccess("journalctl", [
    "-u",
    unit,
    "--no-pager",
    "-o",
    "json",
  ])
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line))
    .filter(
      (entry) =>
        entry.INVOCATION_ID === activeInvocation ||
        entry._SYSTEMD_INVOCATION_ID === activeInvocation
    );
  assert.ok(
    invocationJournal.some((entry) => entry.MESSAGE === "SIMULATED_HELPER_FINISHED"),
    "original helper completion was not journaled"
  );
  assert.ok(
    invocationJournal.some((entry) =>
      entry.MESSAGE?.includes("Deactivated successfully")
    ),
    "systemd did not record natural success for the same invocation"
  );
  assert.ok(
    invocationJournal.some((entry) => entry.JOB_RESULT === "done"),
    "systemd did not finish the same invocation's start job"
  );

  const servicePath = `/run/systemd/system/${deployUnit}`;
  const dropinDir = `/run/systemd/system/${deployUnit}.d`;
  const hold = "/var/lib/qintopia-agent-os-deploy/recovery/hold";
  if (fs.existsSync(servicePath) || fs.existsSync(hold)) {
    throw new Error("VM contains an existing runner unit or recovery hold");
  }
  const started = path.join(root, "service-started");
  write(
    servicePath,
    `[Unit]\nDescription=Simulated deploy runner\n[Service]\nType=oneshot\nExecStart=/usr/bin/touch ${started}\n`
  );
  serviceCreated = true;
  fs.mkdirSync(dropinDir, { recursive: true });
  fs.copyFileSync(
    path.join(
      process.cwd(),
      "deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
    ),
    path.join(dropinDir, "10-recovery-hold.conf")
  );
  fs.mkdirSync(path.dirname(hold), { recursive: true });
  write(hold, "hold\n", 0o600);
  holdCreated = true;
  requireSuccess("systemctl", ["daemon-reload"]);
  requireSuccess("systemctl", ["start", deployUnit]);
  assert.equal(
    fs.existsSync(started),
    false,
    "hold condition did not block old service"
  );
  fs.rmSync(hold);
  holdCreated = false;
  requireSuccess("systemctl", ["start", deployUnit]);
  assert.equal(
    fs.existsSync(started),
    true,
    "old service did not run after hold removal"
  );

  fs.mkdirSync(path.join(state, "requests", "claimed"), { recursive: true });
  const lock = path.join(state, "poller.lock");
  const lockHolder = spawn("bash", ["-c", `exec 9>"${lock}"; flock -n 9; sleep 3`], {
    stdio: "ignore",
  });
  sleep(300);
  const contender = run("flock", ["-n", lock, "true"]);
  assert.notEqual(contender.status, 0, "real flock accepted concurrent consumer");
  lockHolder.kill("SIGKILL");
  sleep(200);
  assert.equal(run("flock", ["-n", lock, "true"]).status, 0);
  const claim = path.join(
    state,
    "requests",
    "claimed",
    "deploy-20260927T000000Z-abcdef0.json"
  );
  write(claim, '{"phase":"possibly_executing"}\n', 0o600);
  const poller = run(
    "bash",
    [path.join(process.cwd(), "deploy/runner/poll-deploy-requests.sh")],
    {
      env: {
        ...process.env,
        QINTOPIA_COS_ENV_FILE: path.join(root, "no-env"),
        QINTOPIA_DEPLOY_RUNNER_STATE_DIR: state,
        TENCENT_COS_BUCKET: "simulated",
        TENCENT_COS_REGION: "simulated",
        TENCENT_COS_SECRET_ID: "simulated",
        TENCENT_COS_SECRET_KEY: "simulated",
        DEPLOY_REQUEST_SIGNING_KEY: "simulated",
        DEPLOY_REQUEST_SIGNING_KEY_ID: "simulated",
      },
    }
  );
  assert.equal(poller.status, 75, "unfinished claim did not block poller");
  assert.match(poller.stderr, /unfinished deploy request claim/);

  requireSuccess("systemctl", ["stop", deployUnit]);
  fs.rmSync(`/run/systemd/system/${deployUnit}`, { force: true });
  fs.rmSync(`/run/systemd/system/${deployUnit}.d`, { recursive: true, force: true });
  serviceCreated = false;
  requireSuccess("systemctl", ["daemon-reload"]);

  const releaseRoot = "/home/ubuntu/qintopia-agent-os-releases";
  if (fs.existsSync(releaseRoot))
    throw new Error("VM release fixture root already exists");
  releaseFixtureCreated = true;
  const tSha = "1".repeat(40);
  const oSha = "2".repeat(40);
  const pSha = "3".repeat(40);
  for (const sha of [tSha, oSha, pSha]) {
    fs.mkdirSync(path.join(releaseRoot, sha, "deploy", "runner"), { recursive: true });
  }
  write(
    path.join(releaseRoot, tSha, "manifest.json"),
    JSON.stringify({ release_sha: tSha, previous_sha: oSha }),
    0o444
  );
  write(
    path.join(releaseRoot, oSha, "manifest.json"),
    JSON.stringify({ release_sha: oSha, previous_sha: pSha }),
    0o444
  );
  write(
    path.join(releaseRoot, pSha, "manifest.json"),
    JSON.stringify({ release_sha: pSha }),
    0o444
  );
  fs.symlinkSync(path.join(releaseRoot, tSha), path.join(releaseRoot, "current"));
  fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "previous"));
  const candidateOnly = "qintopia-agentos-simulated-candidate-only.service";
  write(
    `/run/systemd/system/${candidateOnly}`,
    "[Unit]\nDescription=Simulated candidate only\n[Service]\nType=oneshot\nExecStart=/usr/bin/true\n"
  );
  const oldStarted = path.join(root, "old-service-started");
  const oldUnitTemplate = path.join(root, "old-runner.service");
  write(
    oldUnitTemplate,
    `[Unit]\nDescription=Simulated old O runner\n[Service]\nType=oneshot\nExecStart=/usr/bin/touch ${oldStarted}\n`
  );
  const tInstaller = path.join(
    releaseRoot,
    tSha,
    "deploy/runner/install-release-systemd-units.sh"
  );
  const oInstaller = path.join(
    releaseRoot,
    oSha,
    "deploy/runner/install-release-systemd-units.sh"
  );
  write(
    tInstaller,
    `#!/usr/bin/env bash\nset -euo pipefail\nunit_files=(\n  qintopia-agentos-simulated-shared.service\n  ${candidateOnly}\n)\nrunner_unit_files=(\n  qintopia-agent-os-deploy-runner.service\n  qintopia-agent-os-deploy-runner.timer\n)\n`,
    0o755
  );
  write(
    oInstaller,
    `#!/usr/bin/env bash\nset -euo pipefail\nunit_files=(\n  qintopia-agentos-simulated-shared.service\n)\nrunner_unit_files=(\n  qintopia-agent-os-deploy-runner.service\n  qintopia-agent-os-deploy-runner.timer\n)\ncp "$O_UNIT_TEMPLATE" /run/systemd/system/qintopia-agent-os-deploy-runner.service\nsystemctl daemon-reload\n`,
    0o755
  );
  const fixedDropin =
    "/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf";
  fs.mkdirSync(path.dirname(fixedDropin), { recursive: true });
  fs.copyFileSync(
    path.join(
      process.cwd(),
      "deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
    ),
    fixedDropin
  );
  installedHoldDropin = true;
  write(hold, "hold\n", 0o600);
  holdCreated = true;
  requireSuccess("systemctl", ["daemon-reload"]);
  const primitive = run(
    "bash",
    [
      path.join(process.cwd(), "deploy/runner/rollback-release.sh"),
      "--release-root",
      releaseRoot,
      "--expected-current-sha",
      tSha,
      "--expected-previous-sha",
      oSha,
      "--restore-previous-sha",
      pSha,
    ],
    {
      env: {
        ...process.env,
        O_UNIT_TEMPLATE: oldUnitTemplate,
        QINTOPIA_SYSTEMD_UNIT_DIR: "/run/systemd/system",
      },
    }
  );
  if (primitive.status !== 0)
    throw new Error(`real rollback primitive failed: ${primitive.stderr}`);
  assert.equal(
    fs.realpathSync(path.join(releaseRoot, "current")),
    path.join(releaseRoot, oSha)
  );
  assert.equal(
    fs.realpathSync(path.join(releaseRoot, "previous")),
    path.join(releaseRoot, pSha)
  );
  assert.equal(
    fs.existsSync(`/run/systemd/system/${candidateOnly}`),
    false,
    "real rollback did not remove candidate-only unit"
  );
  requireSuccess("systemctl", ["start", deployUnit]);
  assert.equal(
    fs.existsSync(oldStarted),
    false,
    "old O installer bypassed the persistent recovery hold"
  );

  const rSha = "4".repeat(40);
  fs.mkdirSync(path.join(releaseRoot, rSha, "deploy", "runner"), { recursive: true });
  write(
    path.join(releaseRoot, rSha, "manifest.json"),
    JSON.stringify({ release_sha: rSha, previous_sha: tSha }),
    0o444
  );
  write(
    path.join(releaseRoot, rSha, "deploy/runner/install-release-systemd-units.sh"),
    `#!/usr/bin/env bash\nset -euo pipefail\nunit_files=(\n  qintopia-agentos-simulated-shared.service\n)\nrunner_unit_files=(\n  qintopia-agent-os-deploy-runner.service\n  qintopia-agent-os-deploy-runner.timer\n)\n`,
    0o755
  );
  write(
    path.join(releaseRoot, tSha, "deploy/runner/smoke-release.sh"),
    "#!/usr/bin/env bash\nset -euo pipefail\nexit 0\n",
    0o755
  );
  write(
    path.join(releaseRoot, rSha, "deploy/runner/smoke-release.sh"),
    "#!/usr/bin/env bash\nset -euo pipefail\nexit 0\n",
    0o755
  );
  for (const script of [
    "recover-release-lineage.sh",
    "wait-deploy-result.sh",
    "rollback-release.sh",
  ]) {
    const destination = path.join(releaseRoot, tSha, "deploy/runner", script);
    fs.copyFileSync(path.join(process.cwd(), "deploy/runner", script), destination);
    fs.chmodSync(destination, 0o755);
  }
  write(
    path.join(releaseRoot, tSha, "deploy/runner/management-ui-lifecycle.sh"),
    managementUiLockProbe,
    0o755
  );
  fs.mkdirSync(
    path.join(
      releaseRoot,
      tSha,
      "deploy/runner/qintopia-agent-os-deploy-runner.service.d"
    ),
    { recursive: true }
  );
  fs.copyFileSync(
    path.join(
      process.cwd(),
      "deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
    ),
    path.join(
      releaseRoot,
      tSha,
      "deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
    )
  );
  const timerPath = "/run/systemd/system/qintopia-agent-os-deploy-runner.timer";
  if (fs.existsSync(timerPath)) throw new Error("VM contains an existing deploy timer");
  write(
    timerPath,
    "[Unit]\nDescription=Simulated deploy timer\n[Timer]\nOnCalendar=hourly\n[Install]\nWantedBy=timers.target\n"
  );
  timerCreated = true;
  requireSuccess("systemctl", ["daemon-reload"]);
  const requestTime = new Date().toISOString().replace(/[-:]/g, "").slice(0, 15) + "Z";
  const requestId = `deploy-${requestTime}-${crypto.randomBytes(7).toString("hex")}`;
  const signedAt = new Date().toISOString();
  const targets = [
    "qintopia-system-services",
    "hermes-erhua",
    "hermes-xiaoman",
    "hermes-silaoshi",
    "hermes-huabaosi",
    "hermes-anan",
  ];
  const canonical = (value) =>
    Array.isArray(value)
      ? `[${value.map(canonical).join(",")}]`
      : value !== null && typeof value === "object"
        ? `{${Object.keys(value)
            .sort()
            .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
            .join(",")}}`
        : JSON.stringify(value);
  const signingKey = "simulated-key";
  const request = {
    schema_version: 1,
    request_id: requestId,
    environment: "production",
    repository: "qintopia-agent-studio/qintopia-agent-os",
    requested_by: "simulated",
    created_at: signedAt,
    expires_at: new Date(Date.parse(signedAt) + 3600000).toISOString(),
    release_sha: rSha,
    commit_sha: rSha,
    runtime_sha: rSha,
    runtime_artifact_profile: "huabaosi-production",
    deploy_bundle_sha: rSha,
    release_scope: ["sidecar-runtime", "deploy-bundle", "hermes-plugins"],
    restart_targets: targets,
    rollback_on_smoke_failure: true,
    dry_run: false,
    cos: {
      prefix: "qintopia-agent-os",
      bucket: "simulated",
      region: "simulated",
      request_key: `qintopia-agent-os/deploy-requests/production/requests/${requestId}.json`,
      result_key: `qintopia-agent-os/deploy-results/production/${requestId}.json`,
    },
  };
  const requestMetadata = {
    algorithm: "hmac-sha256",
    issuer: "github-actions",
    key_id: "simulated",
    signed_at: signedAt,
  };
  request.signature = {
    ...requestMetadata,
    value: crypto
      .createHmac("sha256", signingKey)
      .update(canonical({ request, signature: requestMetadata }))
      .digest("hex"),
  };
  const requestPath =
    "/var/lib/qintopia-agent-os-deploy/requests/pending/" + requestId + ".json";
  const resultPath = `/var/lib/qintopia-agent-os-deploy/results/${requestId}.json`;
  const journalPath = `/var/lib/qintopia-agent-os-deploy/recovery/${requestId}.json`;
  recoveryArtifactPaths = [requestPath, resultPath, journalPath];
  if ([requestPath, resultPath, journalPath].some((file) => fs.existsSync(file))) {
    throw new Error("VM contains existing simulated recovery evidence");
  }
  write(requestPath, JSON.stringify(request) + "\n", 0o600);
  recoveryArtifactsCreated = true;
  write(
    path.join(releaseRoot, rSha, "manifest.json"),
    JSON.stringify({
      release_sha: rSha,
      previous_sha: tSha,
      request_id: requestId,
      commit_sha: request.commit_sha,
      runtime_sha: request.runtime_sha,
      deploy_bundle_sha: request.deploy_bundle_sha,
      runtime_artifact_profile: request.runtime_artifact_profile,
      release_scope: request.release_scope,
      restart_targets: request.restart_targets,
    }),
    0o444
  );
  const result = {
    schema_version: 1,
    request_id: requestId,
    environment: "production",
    status: "failed",
    started_at: signedAt,
    finished_at: signedAt,
    release_sha: rSha,
    commit_sha: rSha,
    runtime_sha: rSha,
    runtime_artifact_profile: "huabaosi-production",
    deploy_bundle_sha: rSha,
    release_scope: request.release_scope,
    restart_targets: targets,
    previous_sha: oSha,
    current_target: "",
    checks: [],
    rollback: { attempted: false, status: "not_needed" },
    error: "simulated interruption",
  };
  const resultMetadata = {
    algorithm: "hmac-sha256",
    issuer: "qintopia-deploy-runner",
    key_id: "simulated",
    signed_at: signedAt,
  };
  result.signature = {
    ...resultMetadata,
    value: crypto
      .createHmac("sha256", signingKey)
      .update(canonical({ result, signature: resultMetadata }))
      .digest("hex"),
  };
  write(resultPath, JSON.stringify(result) + "\n", 0o600);
  const cosServerConfig = path.join(root, "cos-server.json");
  const cosServerPort = path.join(root, "cos-port");
  write(
    cosServerConfig,
    JSON.stringify({ mode: "failed", key: request.cos.result_key, result: resultPath })
  );
  cosServer = spawn(
    "python3",
    ["-c", cosMockServerCode, cosServerConfig, cosServerPort],
    { detached: true, stdio: "ignore" }
  );
  for (let attempt = 0; attempt < 100 && !fs.existsSync(cosServerPort); attempt++)
    sleep(20);
  assert.equal(
    fs.existsSync(cosServerPort),
    true,
    "simulated COS server did not start"
  );
  const cosEnv = "/etc/qintopia/cos-artifacts.env";
  assert.equal(fs.existsSync(cosEnv), false, "VM contains an existing COS environment");
  const fakeCoscli = path.join(root, "simulated-coscli");
  write(
    fakeCoscli,
    `#!/bin/bash
set -euo pipefail
[[ "$1" == config ]] && exit 0
[[ "$1" == cp ]] || exit 64
case "$2" in
  *deploy-requests*) cp "${requestPath}" "$3" ;;
  *deploy-results*) cp "${resultPath}" "$3" ;;
  *) exit 64 ;;
esac
`,
    0o755
  );
  write(
    cosEnv,
    `export TENCENT_COS_BUCKET=simulated TENCENT_COS_REGION=simulated TENCENT_COS_SECRET_ID=simulated TENCENT_COS_SECRET_KEY=simulated DEPLOY_REQUEST_SIGNING_KEY=${signingKey} DEPLOY_REQUEST_SIGNING_KEY_ID=simulated COSCLI_PATH=${fakeCoscli} TENCENT_COS_ENDPOINT=http://127.0.0.1:${fs.readFileSync(cosServerPort, "utf8")}\n`,
    0o600
  );
  cosEnvCreated = true;
  const requestBytes = fs.readFileSync(requestPath);
  const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
  const journal = {
    schema_version: 1,
    request_id: requestId,
    request_sha256: sha256(requestBytes),
    direction: "T→R",
    original_current_sha: tSha,
    original_previous_sha: oSha,
    manifest_sha256: {
      current: sha256(fs.readFileSync(path.join(releaseRoot, tSha, "manifest.json"))),
      previous: sha256(fs.readFileSync(path.join(releaseRoot, oSha, "manifest.json"))),
    },
    hold_token: requestId,
    phase: "intent",
  };
  write(journalPath, JSON.stringify(journal) + "\n", 0o600);
  write(hold, requestId + "\n", 0o600);
  const helper = path.join(
    releaseRoot,
    tSha,
    "deploy/runner/recover-release-lineage.sh"
  );
  const stateCases = [
    { name: "T/O", current: tSha, previous: oSha },
    { name: "T/T", current: tSha, previous: tSha },
    { name: "R/T", current: rSha, previous: tSha },
  ];
  for (const scenario of stateCases) {
    write(journalPath, JSON.stringify(journal) + "\n", 0o600);
    for (const pointer of ["current", "previous", "rollback-from"]) {
      fs.rmSync(path.join(releaseRoot, pointer), { force: true });
    }
    fs.symlinkSync(
      path.join(releaseRoot, scenario.current),
      path.join(releaseRoot, "current")
    );
    fs.symlinkSync(
      path.join(releaseRoot, scenario.previous),
      path.join(releaseRoot, "previous")
    );
    const recovered = run(
      "bash",
      [
        ...(process.env.QINTOPIA_RECOVERY_TRACE ? ["-x"] : []),
        helper,
        "--request-id",
        requestId,
      ],
      {
        env: {
          ...process.env,
          DEPLOY_REQUEST_SIGNING_KEY: signingKey,
          DEPLOY_REQUEST_SIGNING_KEY_ID: "simulated",
          O_UNIT_TEMPLATE: oldUnitTemplate,
          QINTOPIA_SYSTEMD_UNIT_DIR: "/run/systemd/system",
        },
      }
    );
    if (recovered.status !== 0) {
      throw new Error(
        `${scenario.name} direction-bound recovery failed: ${recovered.stderr}`
      );
    }
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "current")),
      path.join(releaseRoot, tSha)
    );
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "previous")),
      path.join(releaseRoot, oSha)
    );
    assert.equal(
      fs.existsSync(hold),
      true,
      "recovery cleared hold before COS reconciliation"
    );
  }
  const { signature: _requestSignature, ...unsignedRequest } = request;
  const runnerForClaim = path.join(
    releaseRoot,
    tSha,
    "deploy/runner/qintopia-agent-os-deploy-runner"
  );
  fs.copyFileSync(
    path.join(process.cwd(), "deploy/runner/qintopia-agent-os-deploy-runner"),
    runnerForClaim
  );
  fs.chmodSync(runnerForClaim, 0o755);
  const claimPath = `/var/lib/qintopia-agent-os-deploy/requests/claimed/${requestId}.json`;
  recoveryArtifactPaths.push(claimPath);
  const execution = {
    path: runnerForClaim,
    sha256: sha256(fs.readFileSync(runnerForClaim)),
    unit: deployUnit,
    invocation_id: crypto.randomBytes(16).toString("hex"),
    unit_invocation_verified: true,
  };
  const recoveryClaim = {
    request_id: requestId,
    request_sha256: sha256(requestBytes),
    phase: "possibly_executing",
    recovery_eligible: true,
    result_upload: { phase: "not_started" },
    hold_token: requestId,
    execution,
  };
  fs.rmSync(resultPath);
  write(hold, requestId + "\n", 0o600);
  write(
    cosServerConfig,
    JSON.stringify({ mode: "absent", key: request.cos.result_key, result: resultPath })
  );
  for (const scenario of stateCases) {
    write(claimPath, JSON.stringify(recoveryClaim) + "\n", 0o600);
    write(
      journalPath,
      JSON.stringify({
        ...journal,
        execution,
        hold_token: requestId,
        result_upload: { phase: "not_started" },
      }) + "\n",
      0o600
    );
    for (const pointer of ["current", "previous", "rollback-from"])
      fs.rmSync(path.join(releaseRoot, pointer), { force: true });
    fs.symlinkSync(
      path.join(releaseRoot, scenario.current),
      path.join(releaseRoot, "current")
    );
    fs.symlinkSync(
      path.join(releaseRoot, scenario.previous),
      path.join(releaseRoot, "previous")
    );
    const recovered = run("bash", [helper, "--request-id", requestId], {
      env: {
        ...process.env,
        O_UNIT_TEMPLATE: oldUnitTemplate,
        QINTOPIA_SYSTEMD_UNIT_DIR: "/run/systemd/system",
      },
    });
    assert.equal(
      recovered.status,
      0,
      `${scenario.name} missing-result recovery: ${recovered.stderr}`
    );
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "current")),
      path.join(releaseRoot, tSha)
    );
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "previous")),
      path.join(releaseRoot, oSha)
    );
    assert.equal(fs.readFileSync(hold, "utf8"), requestId + "\n");
  }
  fs.rmSync(claimPath);
  write(hold, requestId + "\n", 0o600);
  write(resultPath, JSON.stringify(result) + "\n", 0o600);
  write(
    cosServerConfig,
    JSON.stringify({ mode: "failed", key: request.cos.result_key, result: resultPath })
  );
  const extraUnsigned = {
    ...unsignedRequest,
    restart_targets: [...targets, "hermes-wenyuange"],
  };
  const extraRequest = {
    ...extraUnsigned,
    signature: {
      ...requestMetadata,
      value: crypto
        .createHmac("sha256", signingKey)
        .update(canonical({ request: extraUnsigned, signature: requestMetadata }))
        .digest("hex"),
    },
  };
  write(requestPath, JSON.stringify(extraRequest) + "\n", 0o600);
  write(
    journalPath,
    JSON.stringify({
      ...journal,
      request_sha256: sha256(fs.readFileSync(requestPath)),
    }) + "\n",
    0o600
  );
  const extra = run("bash", [helper, "--request-id", requestId], {
    env: {
      ...process.env,
      DEPLOY_REQUEST_SIGNING_KEY: signingKey,
      DEPLOY_REQUEST_SIGNING_KEY_ID: "simulated",
      QINTOPIA_SYSTEMD_UNIT_DIR: "/run/systemd/system",
    },
  });
  assert.equal(extra.status, 75, `extra target was accepted: ${extra.stderr}`);
  assert.match(extra.stderr, /approved six-target live action/);
  assert.equal(
    fs.realpathSync(path.join(releaseRoot, "current")),
    path.join(releaseRoot, tSha)
  );
  assert.equal(
    fs.realpathSync(path.join(releaseRoot, "previous")),
    path.join(releaseRoot, oSha)
  );
  assert.equal(fs.existsSync(hold), true);
  console.log(
    JSON.stringify({
      os: "Ubuntu 24.04",
      arch: os.arch(),
      systemd: run("systemctl", ["--version"]).stdout.split("\n")[0],
      invocation,
      busyRejected: true,
      sandboxVerified: true,
      callerDeathRetained: true,
      holdVerified: true,
      realFlockVerified: true,
      claimVerified: true,
      oldUnitHoldVerified: true,
      candidateOnlyCleanupVerified: true,
      mixedForwardStatesVerified: stateCases.map(({ name }) => name),
      missingResultStatesVerified: stateCases.map(({ name }) => name),
      extraTargetRejected: true,
      managementUiDrainInvocationVerified: true,
    })
  );
} finally {
  if (managementUiProbeStarted) {
    run("systemctl", ["stop", managementUiProbeUnit]);
    run("systemctl", ["reset-failed", managementUiProbeUnit]);
    fs.rmSync(`/run/systemd/system/${managementUiProbeUnit}`, { force: true });
  }
  if (serviceCreated) {
    run("systemctl", ["stop", deployUnit]);
    fs.rmSync(`/run/systemd/system/${deployUnit}`, { force: true });
    fs.rmSync(`/run/systemd/system/${deployUnit}.d`, { recursive: true, force: true });
    run("systemctl", ["daemon-reload"]);
  }
  if (holdCreated)
    fs.rmSync("/var/lib/qintopia-agent-os-deploy/recovery/hold", { force: true });
  if (installedHoldDropin) {
    fs.rmSync(
      "/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf",
      { force: true }
    );
  }
  if (timerCreated) {
    run("systemctl", ["disable", "--now", "qintopia-agent-os-deploy-runner.timer"]);
    fs.rmSync("/run/systemd/system/qintopia-agent-os-deploy-runner.timer", {
      force: true,
    });
  }
  if (recoveryArtifactsCreated) {
    for (const file of recoveryArtifactPaths) fs.rmSync(file, { force: true });
  }
  if (cosEnvCreated) fs.rmSync("/etc/qintopia/cos-artifacts.env", { force: true });
  if (cosServer) process.kill(-cosServer.pid, "SIGKILL");
  if (releaseFixtureCreated) {
    fs.rmSync("/run/systemd/system/qintopia-agent-os-deploy-runner.service", {
      force: true,
    });
    fs.rmSync("/run/systemd/system/qintopia-agentos-simulated-candidate-only.service", {
      force: true,
    });
    fs.rmSync("/home/ubuntu/qintopia-agent-os-releases", {
      recursive: true,
      force: true,
    });
  }
  run("systemctl", ["daemon-reload"]);
  fs.rmSync(completion, { force: true });
  fs.rmSync(root, { recursive: true, force: true });
}
