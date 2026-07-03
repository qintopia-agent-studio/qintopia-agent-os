# Server Access

## SSH

```bash
ssh -i /Users/evans/qintopia/server_ssh.pem -o StrictHostKeyChecking=no ubuntu@122.51.77.220
```

## Hermes

The default Hermes gateway and 二花 QiWe gateway are separate ubuntu user systemd
services. QiWe production traffic is owned by the profile-local 二花 gateway.

```bash
systemctl --user status hermes-gateway.service --no-pager
journalctl --user-unit hermes-gateway.service -f --no-pager
systemctl --user status hermes-gateway-erhua.service --no-pager
journalctl --user-unit hermes-gateway-erhua.service -f --no-pager
```

Service file:

```text
/home/ubuntu/.config/systemd/user/hermes-gateway.service
```

Known service details:

```text
ExecStart=/home/ubuntu/.hermes/hermes-agent/venv/bin/python -m hermes_cli.main gateway run --replace
WorkingDirectory=/home/ubuntu/.hermes/hermes-agent
Environment="HERMES_HOME=/home/ubuntu/.hermes"
```

Hermes source:

```text
/home/ubuntu/.hermes/hermes-agent
```

Hermes plugin install target:

```text
/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform
```

## OpenClaw

OpenClaw is a rollback/legacy path. Its gateway is a root user service, not the ubuntu
Hermes service.

```bash
sudo env XDG_RUNTIME_DIR=/run/user/0 systemctl --user status openclaw-gateway.service --no-pager
sudo env XDG_RUNTIME_DIR=/run/user/0 journalctl --user-unit openclaw-gateway.service -f --no-pager
```

Do not restart or modify OpenClaw unless explicitly asked.

## Useful Read-Only Hermes Files

```bash
cd /home/ubuntu/.hermes/hermes-agent
sed -n '1,260p' gateway/platforms/ADDING_A_PLATFORM.md
sed -n '1,260p' website/docs/developer-guide/adding-platform-adapters.md
sed -n '1,220p' gateway/platforms/base.py
sed -n '1,260p' gateway/platforms/webhook.py
find plugins/platforms/irc -maxdepth 2 -type f -print
```

## Deployment Shape

Do not hot-edit plugin source on the server. After local development, tests, commit, and
GitHub publication, align the server checkout from git:

```bash
cd /home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform
git status --short --branch
git fetch origin
git merge --ff-only origin/main
python3 -m unittest discover -s tests -v
systemctl --user restart hermes-gateway-erhua.service
curl -sS http://127.0.0.1:18661/health
journalctl --user-unit hermes-gateway-erhua.service -f --no-pager
```

Use `git merge --ff-only origin/main` or an equivalent `git pull --ff-only`. If the
server checkout is dirty, stop and reconcile it through the local repository and GitHub
rather than editing server plugin files directly.
