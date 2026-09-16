# Pending policy decisions

All rules below remain in force with their original conditions. This migration does not
interpret age, a merged PR, or a new release as permission to retire them.

| Question                                                                 | Existing evidence                                             | Disposition                                                                                               |
| ------------------------------------------------------------------------ | ------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| July Huabaosi/Xiaoman acceptance statements                              | Dated rules and referenced production evidence gates          | Preserve in the compatibility topic; verify actual acceptance in a separate task.                         |
| v0.2.10/v0.2.29/v0.2.30 release reuse exceptions                         | Original release/manifest repair clauses                      | Preserve until supported rollback releases are inventoried.                                               |
| Xiaoman profile distribution versus live cutover                         | Original PR #140/#141 restrictions                            | Preserve; package migration is not live cutover authorization.                                            |
| General draft sequencing versus the owner-approved v0.3.0 reconciliation | Collaboration model records the historical v0.2.177 exception | Preserve the general rule and the explicit exception; do not generalize it.                               |
| Server runtime edits versus owner-directed credential rotation           | Server change policy and session-specific operations          | Preserve the repository policy; this documentation task grants no new exception.                          |
| Repeated Sidecar/QiWe feature, transport and send clauses                | Root and Sidecar source blocks                                | Retain both detailed clauses when wording/conditions differ; short entries link to their canonical topic. |

The migration table marks explicitly dated historical blocks `pending-review`. Other
questions above concern interactions among multiple retained blocks, not authorization
to weaken any one of them.
