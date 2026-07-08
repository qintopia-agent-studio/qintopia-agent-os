#!/usr/bin/env node

import {
  attachWorkflowRunMetadata,
  extractDeployResultsFromLog,
} from "./collect-release-deploy-results.mjs";

const log = [
  "request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0039484Z Deploy result succeeded: succeeded",
  "request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0395832Z {",
  'request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0405962Z     "schema_version": 1,',
  'request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0406955Z     "request_id": "deploy-20260708T052349Z-d083e5ccfce2",',
  'request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0407904Z     "environment": "production",',
  'request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0408572Z     "status": "succeeded",',
  'request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0410682Z     "release_sha": "d083e5ccfce2d07048e07c0ceb8c052671f65911",',
  'request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0411580Z     "previous_sha": "b24c3f714b19962c5a7b57a486f7aa18c4ae3e86",',
  'request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0421208Z     "rollback": {',
  'request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0421728Z       "attempted": false,',
  'request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0422200Z       "status": "not_needed"',
  "request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0423000Z     }",
  "request-deploy\tWait for server deploy result\t2026-07-08T05:24:21.0423500Z }",
].join("\n");

const results = extractDeployResultsFromLog(log);
if (results.length !== 1) {
  throw new Error(`expected one deploy result, got ${results.length}`);
}
if (results[0].status !== "succeeded") {
  throw new Error(`expected succeeded status, got ${results[0].status}`);
}
if (results[0].release_sha !== "d083e5ccfce2d07048e07c0ceb8c052671f65911") {
  throw new Error("release_sha was not extracted");
}

const withMetadata = attachWorkflowRunMetadata(results, {
  id: 123,
  created_at: "2026-07-08T05:20:00Z",
  run_started_at: "2026-07-08T05:21:00Z",
  updated_at: "2026-07-08T05:24:30Z",
});
if (withMetadata[0].workflow_run?.id !== "123") {
  throw new Error("workflow run id metadata was not attached");
}
if (withMetadata[0].workflow_run?.run_started_at !== "2026-07-08T05:21:00Z") {
  throw new Error("workflow run timestamp metadata was not attached");
}

console.log("Release deploy result log collector tests passed.");
