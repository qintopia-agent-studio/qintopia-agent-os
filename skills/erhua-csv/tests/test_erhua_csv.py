from __future__ import annotations

import csv
import importlib.util
import json
import os
import stat
import tempfile
import threading
import unittest
from unittest import mock
from datetime import date, timedelta
from pathlib import Path


PACKAGE_ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("erhua_csv_test_plugin", PACKAGE_ROOT / "__init__.py")
erhua_csv = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(erhua_csv)


class ErhuaCsvWorkspaceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        root = Path(self.temporary.name)
        self.workspace = erhua_csv.CsvWorkspace(
            root / "data" / "csv" / "v1",
            root / "data" / "csv-internal" / "v1",
        )
        self.context = self.ctx("group-a", "user-a", "message-1", "Alice")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    @staticmethod
    def ctx(chat: str, user: str, message: str, name: str = "Member"):
        return erhua_csv.SessionContext("qiwe", chat, user, name, message)

    def create_custom(self, *, context=None, fields=None, name="People") -> str:
        result = self.workspace.create(
            context or self.context,
            {
                "name": name,
                "description": "Group records",
                "preset": "custom",
                "fields": fields
                or [
                    {"name": "person", "type": "text", "required": True},
                    {"name": "score", "type": "decimal"},
                ],
            },
        )
        self.assertTrue(result["success"])
        return result["dataset"]["csv_id"]

    def test_group_context_is_required_and_direct_chat_is_rejected(self) -> None:
        for context in (
            erhua_csv.SessionContext("other", "group", "user", "User", "message"),
            erhua_csv.SessionContext("qiwe", "", "user", "User", "message"),
            erhua_csv.SessionContext("qiwe", "same", "same", "User", "message"),
        ):
            with self.assertRaises(erhua_csv.CsvWorkspaceError):
                self.workspace.list(context)

    def test_create_append_query_types_extra_and_hidden_audit_ids(self) -> None:
        csv_id = self.create_custom(
            fields=[
                {"name": "text_value", "type": "text", "required": True},
                {"name": "decimal_value", "type": "decimal"},
                {"name": "integer_value", "type": "integer"},
                {"name": "boolean_value", "type": "boolean"},
                {"name": "date_value", "type": "date"},
                {"name": "datetime_value", "type": "datetime"},
                {"name": "enum_value", "type": "enum", "enum": ["one", "two"]},
            ]
        )
        appended = self.workspace.append(
            self.ctx("group-a", "user-a", "message-2", "Alice"),
            csv_id,
            {
                "text_value": "hello",
                "decimal_value": "1.2300",
                "integer_value": 2,
                "boolean_value": True,
                "date_value": "2026-08-14",
                "datetime_value": "2026-08-14T09:30:00+08:00",
                "enum_value": "one",
                "future_field": "retained",
            },
        )
        self.assertTrue(appended["checksum_verified"])
        result = self.workspace.query(self.context, {"csv_id": csv_id, "count": True})
        self.assertEqual(result["count"], 1)
        row = result["rows"][0]
        self.assertEqual(row["fields"]["decimal_value"], "1.23")
        self.assertEqual(row["fields"]["integer_value"], 2)
        self.assertIs(row["fields"]["boolean_value"], True)
        self.assertEqual(row["fields"]["future_field"], "retained")
        encoded = json.dumps(result)
        self.assertNotIn("user-a", encoded)
        self.assertNotIn("message-2", encoded)
        self.assertNotIn(str(self.workspace.root), encoded)

    def test_schema_version_adds_optional_fields_and_promotes_old_extra(self) -> None:
        csv_id = self.create_custom(fields=[{"name": "name", "type": "text", "required": True}])
        self.workspace.append(
            self.ctx("group-a", "user-a", "message-2"),
            csv_id,
            {"name": "old", "age": "3"},
        )
        version_context = self.ctx("group-a", "user-a", "message-3")
        version = self.workspace.create(
            version_context,
            {
                "name": "People",
                "version_of": csv_id,
                "fields": [{"name": "age", "type": "integer", "required": False}],
            },
        )
        self.assertEqual(version["dataset"]["latest_version"], 2)
        replay = self.workspace.create(
            version_context,
            {
                "name": "People",
                "version_of": csv_id,
                "fields": [{"name": "age", "type": "integer", "required": False}],
            },
        )
        self.assertTrue(replay["idempotent"])
        self.workspace.append(
            self.ctx("group-a", "user-b", "message-4"),
            csv_id,
            {"name": "new", "age": 4},
        )
        rows = self.workspace.query(self.context, {"csv_id": csv_id, "count": True})
        self.assertEqual([row["fields"]["age"] for row in rows["rows"]], [3, 4])
        with self.assertRaises(erhua_csv.CsvWorkspaceError):
            self.workspace.create(
                self.ctx("group-a", "user-a", "message-5"),
                {
                    "name": "People",
                    "version_of": csv_id,
                    "fields": [{"name": "required_later", "type": "text", "required": True}],
                },
            )

    def test_schema_version_rejects_existing_extra_that_cannot_use_the_new_type(self) -> None:
        csv_id = self.create_custom(fields=[{"name": "name", "type": "text"}])
        self.workspace.append(
            self.ctx("group-a", "user-a", "message-2"),
            csv_id,
            {"name": "old", "future_integer": "not-an-integer"},
        )
        with self.assertRaises(erhua_csv.CsvWorkspaceError):
            self.workspace.create(
                self.ctx("group-a", "user-a", "message-3"),
                {
                    "name": "People",
                    "version_of": csv_id,
                    "fields": [{"name": "future_integer", "type": "integer"}],
                },
            )

    def test_append_is_idempotent_for_same_message_and_caller_row(self) -> None:
        csv_id = self.create_custom()
        context = self.ctx("group-a", "user-a", "message-2")
        first = self.workspace.append(context, csv_id, {"person": "Alice", "score": "2.00"})
        second = self.workspace.append(context, csv_id, {"person": "Alice", "score": "2.00"})
        self.assertFalse(first["idempotent"])
        self.assertTrue(second["idempotent"])
        self.assertEqual(first["row"]["_event_id"], second["row"]["_event_id"])
        queried = self.workspace.query(self.context, {"csv_id": csv_id, "count": True})
        self.assertEqual(queried["count"], 1)

    def test_query_filters_pagination_count_and_decimal_sum(self) -> None:
        csv_id = self.create_custom()
        for index, (person, score) in enumerate((("A", "0.1"), ("A", "0.2"), ("B", "3")), 2):
            self.workspace.append(
                self.ctx("group-a", f"user-{index}", f"message-{index}"),
                csv_id,
                {"person": person, "score": score},
            )
        result = self.workspace.query(
            self.context,
            {
                "csv_id": csv_id,
                "filters": {"person": "A"},
                "offset": 0,
                "limit": 1,
                "count": True,
                "sum_field": "score",
            },
        )
        self.assertEqual(result["count"], 2)
        self.assertEqual(result["sum"], {"field": "score", "value": "0.3"})
        self.assertEqual(result["returned"], 1)
        self.assertTrue(result["has_more"])

    def test_formula_injection_negative_integer_and_oversized_row_are_rejected(self) -> None:
        csv_id = self.create_custom(
            fields=[
                {"name": "text", "type": "text"},
                {"name": "integer", "type": "integer"},
                {"name": "signed", "type": "decimal"},
            ]
        )
        with self.assertRaises(erhua_csv.CsvWorkspaceError):
            self.workspace.append(self.ctx("group-a", "u", "m2"), csv_id, {"text": "=SUM(A1:A2)"})
        with self.assertRaises(erhua_csv.CsvWorkspaceError):
            self.workspace.append(self.ctx("group-a", "u", "m3"), csv_id, {"integer": -1})
        signed = self.workspace.append(self.ctx("group-a", "u", "m4"), csv_id, {"signed": "-1.25"})
        self.assertEqual(signed["row"]["fields"]["signed"], "-1.25")
        with self.assertRaises(erhua_csv.CsvWorkspaceError):
            self.workspace.append(self.ctx("group-a", "u", "m5"), csv_id, {"text": "x" * 17000})
        with self.assertRaises(erhua_csv.CsvWorkspaceError):
            self.workspace.create(
                self.ctx("group-a", "u", "m6"),
                {
                    "name": "Unsafe header",
                    "preset": "custom",
                    "fields": [{"name": "=formula", "type": "text"}],
                },
            )

    def test_decimal_expansion_is_bounded_before_fixed_point_formatting(self) -> None:
        for value in ("1e100000000", "1e-100000000"):
            with self.assertRaisesRegex(
                erhua_csv.CsvWorkspaceError, "supported decimal size"
            ):
                erhua_csv._canonical_decimal(value, "test decimal")

        with self.assertRaisesRegex(erhua_csv.CsvWorkspaceError, "supported decimal size"):
            self.workspace.create(
                self.ctx("group-a", "user-a", "decimal-default"),
                {
                    "name": "Unsafe default",
                    "preset": "custom",
                    "fields": [
                        {"name": "amount", "type": "decimal", "default": "1e100000000"}
                    ],
                },
            )

        csv_id = self.create_custom(
            fields=[{"name": "amount", "type": "decimal"}], name="Bounded decimals"
        )
        with self.assertRaisesRegex(erhua_csv.CsvWorkspaceError, "supported decimal size"):
            self.workspace.append(
                self.ctx("group-a", "user-a", "decimal-append"),
                csv_id,
                {"amount": "1e100000000"},
            )
        with self.assertRaisesRegex(erhua_csv.CsvWorkspaceError, "supported decimal size"):
            self.workspace.query(
                self.context,
                {"csv_id": csv_id, "filters": {"amount": "1e100000000"}},
            )

        ledger = self.workspace.create(
            self.ctx("group-a", "user-a", "decimal-ledger-create"),
            {"name": "Bounded ledger", "preset": "ledger"},
        )
        with self.assertRaisesRegex(erhua_csv.CsvWorkspaceError, "supported decimal size"):
            self.workspace.append(
                self.ctx("group-a", "user-a", "decimal-ledger-append"),
                ledger["dataset"]["csv_id"],
                {"direction": "income", "amount": "1e100000000"},
            )

    def test_groups_cannot_list_or_query_each_others_datasets(self) -> None:
        csv_id = self.create_custom()
        other = self.ctx("group-b", "user-a", "message-2")
        self.assertEqual(self.workspace.list(other)["dataset_count"], 0)
        with self.assertRaises(erhua_csv.CsvWorkspaceError):
            self.workspace.query(other, {"csv_id": csv_id})
        with self.assertRaises(erhua_csv.CsvWorkspaceError):
            self.workspace.append(other, csv_id, {"person": "cross-group"})

    def test_ledger_balances_accounts_decimal_precision_idempotency_and_reversal(self) -> None:
        created = self.workspace.create(
            self.context,
            {"name": "Group ledger", "description": "Cash book", "preset": "ledger"},
        )
        csv_id = created["dataset"]["csv_id"]
        income_context = self.ctx("group-a", "user-a", "ledger-2")
        income = self.workspace.append(
            income_context,
            csv_id,
            {"direction": "income", "amount": "0.10", "note": "income"},
        )
        replay = self.workspace.append(
            income_context,
            csv_id,
            {"direction": "income", "amount": "0.10", "note": "income"},
        )
        self.assertTrue(replay["idempotent"])
        expense = self.workspace.append(
            self.ctx("group-a", "user-b", "ledger-3"),
            csv_id,
            {"direction": "expense", "amount": "0.03"},
        )
        self.assertEqual(expense["ledger_balance"]["balance"], "0.07")
        separate = self.workspace.append(
            self.ctx("group-a", "user-b", "ledger-4"),
            csv_id,
            {"account": "bank", "currency": "USD", "direction": "income", "amount": "2"},
        )
        self.assertEqual(separate["ledger_balance"]["balance"], "2")
        reversal_context = self.ctx("group-a", "user-a", "ledger-5")
        reversal = self.workspace.append(
            reversal_context,
            csv_id,
            {"reverses_event_id": income["row"]["_event_id"]},
        )
        self.assertEqual(reversal["ledger_balance"]["balance"], "-0.03")
        replay_reversal = self.workspace.append(
            reversal_context,
            csv_id,
            {"reverses_event_id": income["row"]["_event_id"]},
        )
        self.assertTrue(replay_reversal["idempotent"])
        with self.assertRaises(erhua_csv.CsvWorkspaceError):
            self.workspace.append(
                self.ctx("group-a", "user-c", "ledger-6"),
                csv_id,
                {"reverses_event_id": income["row"]["_event_id"]},
            )

    def test_checksum_tamper_partial_tail_and_symlink_fail_closed(self) -> None:
        csv_id = self.create_custom()
        self.workspace.append(
            self.ctx("group-a", "user-a", "message-2"), csv_id, {"person": "Alice"}
        )
        group = self.workspace._group_path(self.context)
        rows_path = self.workspace._rows_path(group, csv_id, 1)
        original = rows_path.read_text(encoding="utf-8")
        rows_path.write_text(original.replace("Alice", "Mallory"), encoding="utf-8")
        with self.assertRaises(erhua_csv.CsvIntegrityError):
            self.workspace.query(self.context, {"csv_id": csv_id})
        rows_path.write_text(original + '"partial', encoding="utf-8")
        with self.assertRaises(erhua_csv.CsvIntegrityError):
            self.workspace.query(self.context, {"csv_id": csv_id})
        rows_path.write_text(original, encoding="utf-8")
        attack = group / "datasets" / "link"
        attack.symlink_to(rows_path)
        with self.assertRaises(erhua_csv.CsvIntegrityError):
            self.workspace.list(self.context)

    def test_hard_link_and_permission_drift_fail_closed(self) -> None:
        csv_id = self.create_custom()
        group = self.workspace._group_path(self.context)
        rows_path = self.workspace._rows_path(group, csv_id, 1)
        hard_link = group / "datasets" / "linked-rows.csv"
        os.link(rows_path, hard_link)
        with self.assertRaises(erhua_csv.CsvIntegrityError):
            self.workspace.list(self.context)
        hard_link.unlink()
        os.chmod(rows_path, 0o644)
        with self.assertRaises(erhua_csv.CsvIntegrityError):
            self.workspace.query(self.context, {"csv_id": csv_id})

    def test_directory_permission_drift_blocks_writes(self) -> None:
        csv_id = self.create_custom()
        group = self.workspace._group_path(self.context)
        os.chmod(group, 0o755)
        with self.assertRaises(erhua_csv.CsvIntegrityError):
            self.workspace.append(
                self.ctx("group-a", "user-a", "message-2"), csv_id, {"person": "Alice"}
            )

    def test_concurrent_appends_are_serialized_and_survive_restart(self) -> None:
        csv_id = self.create_custom()
        errors = []

        def append(index: int) -> None:
            try:
                workspace = erhua_csv.CsvWorkspace(self.workspace.root, self.workspace.internal_root)
                workspace.append(
                    self.ctx("group-a", f"user-{index}", f"concurrent-{index}"),
                    csv_id,
                    {"person": f"person-{index}", "score": str(index)},
                )
            except Exception as exc:  # pragma: no cover - asserted below
                errors.append(exc)

        threads = [threading.Thread(target=append, args=(index,)) for index in range(10)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        self.assertEqual(errors, [])
        restarted = erhua_csv.CsvWorkspace(self.workspace.root, self.workspace.internal_root)
        result = restarted.query(self.context, {"csv_id": csv_id, "count": True})
        self.assertEqual(result["count"], 10)

    def test_daily_snapshot_is_refreshed_and_files_are_private(self) -> None:
        csv_id = self.create_custom()
        self.workspace.append(self.ctx("group-a", "user-a", "message-2"), csv_id, {"person": "A"})
        self.workspace.append(self.ctx("group-a", "user-a", "message-3"), csv_id, {"person": "B"})
        scope = self.workspace._group_path(self.context).name
        snapshots = self.workspace.internal_root / "snapshots" / scope
        dated = [path for path in snapshots.iterdir() if erhua_csv.DATE_DIR_RE.fullmatch(path.name)]
        self.assertEqual(len(dated), 1)
        snapshot_text = "\n".join(
            path.read_text(encoding="utf-8")
            for path in dated[0].rglob("*")
            if path.is_file()
        )
        self.assertIn("message-3", snapshot_text)
        self.assertIn("B", snapshot_text)
        for path in (self.workspace.root, self.workspace._group_path(self.context), snapshots):
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o700)
        catalog = self.workspace._group_path(self.context) / "catalog.jsonl"
        self.assertEqual(stat.S_IMODE(catalog.stat().st_mode), 0o600)

    def test_snapshot_failure_does_not_misreport_verified_append_as_failed(self) -> None:
        csv_id = self.create_custom()
        with mock.patch.object(
            self.workspace,
            "_snapshot_after_mutation",
            side_effect=OSError("snapshot storage unavailable"),
        ):
            result = self.workspace.append(
                self.ctx("group-a", "user-a", "message-2"), csv_id, {"person": "Alice"}
            )
        self.assertTrue(result["success"])
        self.assertTrue(result["checksum_verified"])
        self.assertEqual(result["recovery_snapshot"], "failed")
        self.assertIn("committed and readable", result["warning"])
        queried = self.workspace.query(self.context, {"csv_id": csv_id, "count": True})
        self.assertEqual(queried["count"], 1)

    def test_snapshot_retention_keeps_latest_thirty_dates(self) -> None:
        snapshots = self.workspace.internal_root / "snapshots" / ("a" * 64)
        snapshots.mkdir(parents=True)
        start = date(2026, 1, 1)
        for offset in range(31):
            (snapshots / (start + timedelta(days=offset)).isoformat()).mkdir()
        self.workspace._prune_snapshots(snapshots)
        retained = sorted(path.name for path in snapshots.iterdir())
        self.assertEqual(len(retained), 30)
        self.assertEqual(retained[0], "2026-01-02")
        self.assertEqual(retained[-1], "2026-01-31")

    def test_tool_handler_rejects_spoofed_scope_and_missing_or_direct_context(self) -> None:
        saved = {name: os.environ.get(name) for name in (
            "HERMES_SESSION_PLATFORM",
            "HERMES_SESSION_CHAT_ID",
            "HERMES_SESSION_USER_ID",
            "HERMES_SESSION_USER_NAME",
            "HERMES_SESSION_MESSAGE_ID",
        )}
        previous_workspace = erhua_csv._DEFAULT_WORKSPACE
        erhua_csv._DEFAULT_WORKSPACE = self.workspace
        try:
            os.environ.update(
                {
                    "HERMES_SESSION_PLATFORM": "qiwe",
                    "HERMES_SESSION_CHAT_ID": "group-a",
                    "HERMES_SESSION_USER_ID": "user-a",
                    "HERMES_SESSION_USER_NAME": "Alice",
                    "HERMES_SESSION_MESSAGE_ID": "handler-message",
                }
            )
            spoofed = json.loads(
                erhua_csv.handle_qintopia_erhua_csv_create(
                    {
                        "name": "Spoofed",
                        "preset": "custom",
                        "fields": [{"name": "value", "type": "text"}],
                        "chat_id": "group-b",
                    }
                )
            )
            self.assertFalse(spoofed["success"])
            self.assertIn("unsupported properties", spoofed["error"])
            os.environ["HERMES_SESSION_CHAT_ID"] = "user-a"
            direct = json.loads(erhua_csv.handle_qintopia_erhua_csv_list({}))
            self.assertFalse(direct["success"])
            self.assertIn("direct chats", direct["error"])
            os.environ["HERMES_SESSION_CHAT_ID"] = "group-a"
            os.environ.pop("HERMES_SESSION_MESSAGE_ID")
            missing = json.loads(erhua_csv.handle_qintopia_erhua_csv_list({}))
            self.assertFalse(missing["success"])
            self.assertIn("incomplete", missing["error"])
        finally:
            erhua_csv._DEFAULT_WORKSPACE = previous_workspace
            for name, value in saved.items():
                if value is None:
                    os.environ.pop(name, None)
                else:
                    os.environ[name] = value

    def test_catalog_and_csv_are_append_only_shapes(self) -> None:
        csv_id = self.create_custom()
        self.workspace.append(
            self.ctx("group-a", "user-a", "message-2"), csv_id, {"person": "Alice"}
        )
        group = self.workspace._group_path(self.context)
        catalog_lines = (group / "catalog.jsonl").read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(catalog_lines), 1)
        catalog_event = json.loads(catalog_lines[0])
        self.assertEqual(catalog_event["event"], "dataset_created")
        self.assertIn("actor_user_id", catalog_event)
        with (self.workspace._rows_path(group, csv_id, 1)).open(newline="", encoding="utf-8") as handle:
            rows = list(csv.reader(handle))
        self.assertEqual(rows[0][:6], list(erhua_csv.SYSTEM_COLUMNS))
        self.assertEqual(rows[0][-1], erhua_csv.EXTRA_COLUMN)
        self.assertEqual(len(rows), 2)


if __name__ == "__main__":
    unittest.main()
