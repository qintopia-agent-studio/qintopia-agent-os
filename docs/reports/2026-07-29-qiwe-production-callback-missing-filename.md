# QiWe Production Callback Missing Filename

Date: 2026-07-29

## Summary

The owner-approved production image-send exercise on release `v0.2.55` completed
authenticated Feishu readback, QiWe SDK temporary upload, same-byte temporary URL
readback, and asynchronous URL-upload acceptance. One `cmd=20000` callback reached the
Erhua webhook immediately afterward, but the callback bridge did not invoke the Rust
processor and `/msg/sendImage` was not called.

The worker timer and persistent send flag were returned to the disabled state before a
second attempt could start. The expired attempt must not be reused or retried.

## Sanitized Evidence

- The callback used the successful top-level envelope and the expected `cmd=20000`.
- Its request-id hash matched the one accepted asynchronous upload attempt.
- `msgData` contained `fileAesKey`, `fileId`, `fileMd5`, and `fileSize`.
- `msgData` did not contain `filename`, `fileName`, or `cloudUrl`.
- Three additional field names were present, but their names and values were not
  retained.
- The callback body and file credentials remained memory-only; the ordinary capture path
  retained only hashes and field-presence booleans.

## Root Cause

The bridge and Rust parser treated callback `filename` as mandatory because
`/msg/sendImage` requires a filename. That conflated two protocol boundaries. The
asynchronous upload request already supplied the canonical reviewed JPEG filename, and
the upload claim locked that filename together with the approved artifact id, MD5, byte
size, content hash, target group, and request correlation before external I/O.

The real callback supplied the four memory-only file credentials needed after upload,
but omitted the filename. The bridge therefore classified it as unrelated before the
transactional callback policy could validate the matching attempt.

## Resolution Boundary

- Recognize a successful `cmd=20000` image callback when request id, `msgData`,
  `fileAesKey`, `fileId`, `fileMd5`, and `fileSize` are present. `filename` and
  `fileName` are optional.
- Preserve the existing ambiguity rejection when both filename spellings are present.
- If the callback supplies a filename, require it to match the locked approved JPEG.
- If it omits a filename, obtain the send filename only from the transaction-locked
  approved artifact. Do not accept a caller default, provider URL path, unknown field,
  or mutable runtime value.
- Continue requiring exact request correlation, canonical MD5 equality, byte-size
  equality, current claim ownership, approved artifact state, final human confirmation,
  send-ready evidence, target-group allowlisting, and the at-most-once `sending` commit
  before `/msg/sendImage`.
- Add fixed sanitized schema ids for the two no-filename AES-key spellings. Do not
  expose callback values or unknown field names.

## Acceptance

1. Python bridge tests prove the real no-filename shape is routed while incomplete core
   credentials and unrelated or nested events remain rejected.
2. Rust parser tests accept both reviewed no-filename schema ids and retain all existing
   alias, ambiguity, size, MD5, and leakage checks.
3. State tests prove a missing callback filename resolves to the locked approved JPEG,
   while a supplied mismatched filename still fails before `sending`.
4. Send-request tests prove `/msg/sendImage` receives the locked approved filename and
   never an untrusted callback default.
5. Callback capture, staging evidence, and production evidence validators recognize the
   new sanitized schema ids without weakening their exact output contracts.
6. Relevant Python, Rust, deploy-contract, secret, and production-evidence checks pass.

## Production Follow-Up

After merge and an owner-published Release, deploy the immutable QiWe production
companion, leave the previous attempt terminal, and create one new human-confirmed send
request for the approved image. Enable only the reviewed timer, let it wake naturally,
and require one fresh callback, one successful `/msg/sendImage`, owner confirmation of
group arrival, and the final Xiaoman production evidence checks before claiming the
workflow is online.
