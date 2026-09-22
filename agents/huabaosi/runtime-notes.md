# Huabaosi Runtime Notes

Observed read-only on 2026-07-03:

- Runtime path: `/home/ubuntu/.hermes/profiles/huabaosi`
- User service: `hermes-gateway-huabaosi.service`
- Observed plugin: `qintopia-base-read`
- Observed non-secret config shape includes `SOUL.md`, `config.yaml`, `profile.yaml`,
  `processes.json`, and `channel_directory.json`.

The profile is represented as review-pool until the owner approves which parts of
Huabaosi behavior and adapter work are production direction.

## Local welcome task runtime

`welcome_runtime.py` registers this Agent's bounded welcome tools with
`workflows/resident-welcome/scripts/local_agent_runtime.py`. The host supplies the
locked WorkItem and business input through one stdin pipe; model arguments must be
empty. Tool output is consumed by the governed Rust workflow and tied to call/task,
input/output hashes and the exact plugin source hash. The child exposes no database,
network or direct-send tools. This is `local_scripted_agent_runtime` evidence, not real
Hermes or LLM execution and not production activation.
