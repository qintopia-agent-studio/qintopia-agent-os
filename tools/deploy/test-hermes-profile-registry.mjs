#!/usr/bin/env node

import assert from "node:assert/strict";
import { loadHermesProfileRegistry } from "./hermes-profile-registry.mjs";

const expected = [
  ["default", "hermes-gateway.service", true, false],
  ["erhua", "hermes-gateway-erhua.service", false, true],
  ["guanerye", "hermes-gateway-guanerye.service", true, false],
  ["huabaosi", "hermes-gateway-huabaosi.service", true, false],
  ["silaoshi", "hermes-gateway-silaoshi.service", true, false],
  ["wenyuange", "hermes-gateway-wenyuange.service", false, false],
  ["xiaoman", "hermes-gateway-xiaoman.service", true, false],
];

const actual = loadHermesProfileRegistry().map((profile) => [
  profile.id,
  profile.systemd_user_service,
  profile.wecom.expected_enabled,
  profile.qiwe_platform.preserve,
]);

assert.deepEqual(actual, expected);
for (const profile of loadHermesProfileRegistry()) {
  assert.deepEqual(profile.wecom.required_bindings, ["bot_id", "secret"]);
}

console.log("Hermes profile registry test passed.");
