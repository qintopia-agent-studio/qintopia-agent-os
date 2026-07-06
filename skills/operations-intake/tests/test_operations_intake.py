from __future__ import annotations

import importlib.util
import json
import os
import unittest
from pathlib import Path


def load_plugin():
    plugin_path = Path(__file__).resolve().parents[1] / "__init__.py"
    spec = importlib.util.spec_from_file_location("operations_intake_plugin", plugin_path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class OperationsIntakeTest(unittest.TestCase):
    def setUp(self) -> None:
        self.module = load_plugin()

    def tearDown(self) -> None:
        self.module.configure_runtime()

    def test_complaint_intake_create_is_controlled(self):
        self.module.configure_runtime(
            kanban_create_complaint=lambda title, body, priority, key: ("t_test", "created")
        )

        payload = json.loads(
            self.module.handle_qintopia_complaint_intake_create(
                {
                    "source_channel": "qiwe_group_internal",
                    "source_conversation_id": "conv_1",
                    "source_message_id": "msg_1",
                    "requester_display_name": "小秦",
                    "requester_channel_user_id": "user_1",
                    "original_message": "我要投诉入住体验，晚上太吵了。",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["task_id"], "t_test")
        self.assertEqual(payload["task_type"], "complaint_intake")
        self.assertEqual(payload["owner_profile"], "default")
        self.assertEqual(payload["tenant"], "qintopia")
        kanban_action = payload["actions"][0]
        self.assertEqual(kanban_action["task_type"], "complaint_intake")
        self.assertEqual(kanban_action["assignee"], "default")
        private_action = payload["actions"][1]
        self.assertEqual(private_action["tool"], "qiwe_send_direct_message")
        self.assertEqual(private_action["recipient_user_id"], "user_1")
        self.assertEqual(private_action["conversation_scope"], "private")
        self.assertIn("为了避免在群里公开你的细节", private_action["message"])

    def test_complaint_intake_create_uses_qiwe_session_sender_id(self):
        old_user_id = os.environ.get("HERMES_SESSION_USER_ID")
        captured = {}

        def fake_create(title, body, priority, key):
            captured["body"] = body
            captured["key"] = key
            return "t_test", "created"

        self.module.configure_runtime(kanban_create_complaint=fake_create)
        os.environ["HERMES_SESSION_USER_ID"] = "7881303308049798"
        try:
            payload = json.loads(
                self.module.handle_qintopia_complaint_intake_create(
                    {
                        "source_channel": "qiwe_group_internal",
                        "source_conversation_id": "room_1",
                        "source_message_id": "msg_1",
                        "requester_display_name": "弦默",
                        "original_message": "我要投诉，房间门锁有问题。",
                    }
                )
            )
        finally:
            if old_user_id is None:
                os.environ.pop("HERMES_SESSION_USER_ID", None)
            else:
                os.environ["HERMES_SESSION_USER_ID"] = old_user_id

        self.assertTrue(payload["success"])
        self.assertTrue(payload["requester_channel_user_id_resolved"])
        self.assertIn("7881303308049798", captured["body"])
        self.assertIn("不要再创建或派发“二花私聊补充受理”子任务", captured["body"])
        private_action = payload["actions"][1]
        self.assertEqual(private_action["recipient_user_id"], "7881303308049798")
        self.assertEqual(private_action["idempotency_key"], f"{captured['key']}:direct:intake")

    def test_complaint_update_and_followup_are_private_and_append_only(self):
        self.module.configure_runtime(
            kanban_add_complaint_comment=lambda task_id, body: (12, "comment_added")
        )

        update = json.loads(
            self.module.handle_qintopia_complaint_intake_update(
                {
                    "task_id": "t_test",
                    "requester_display_name": "小秦",
                    "details": "昨晚 11 点后 2 栋走廊持续很吵。",
                    "location_or_area": "2 栋走廊",
                }
            )
        )
        self.assertTrue(update["success"])
        self.assertTrue(update["actions"][0]["does_not_assign_executor"])

        followup = json.loads(
            self.module.handle_qintopia_complaint_followup_send(
                {
                    "task_id": "t_test",
                    "requester_channel_user_id": "user_1",
                    "requester_display_name": "小秦",
                    "approved_resolution": "已安排工作人员检查并完成走廊夜间提醒。",
                }
            )
        )
        self.assertTrue(followup["success"])
        action = followup["actions"][0]
        self.assertEqual(action["conversation_scope"], "private")
        self.assertTrue(action["requires_approved_resolution"])
        self.assertIn("已安排工作人员检查", action["message"])

    def test_product_and_case_search_use_injected_public_kb(self):
        def fake_kb_search(args):
            if "客户案例" in args["query"]:
                return json.dumps({"results": [], "result_count": 0}, ensure_ascii=False)
            return json.dumps(
                {
                    "results": [
                        {
                            "path": "agent-os-public-faq.md",
                            "snippet": "Qintopia Agent OS 支持需求收集和任务交接。",
                        }
                    ],
                    "result_count": 1,
                },
                ensure_ascii=False,
            )

        self.module.configure_runtime(kb_search_handler=fake_kb_search)
        product = json.loads(
            self.module.handle_qintopia_external_product_kb_search(
                {"query": "Agent OS 可以做什么", "purpose": "回答外部客户"}
            )
        )
        self.assertTrue(product["success"])
        self.assertEqual(product["scope_used"], ["Public"])
        self.assertEqual(product["results"][0]["path"], "agent-os-public-faq.md")
        self.assertIn("Internal", product["not_accessed"])

        case = json.loads(self.module.handle_qintopia_public_case_search({"query": "客户案例"}))
        self.assertTrue(case["success"])
        self.assertFalse(case["approved_public_cases_available"])
        self.assertTrue(case["needs_human_review"])
        self.assertIn("没有检索到已批准公开的客户案例", case["safe_customer_message"])

    def test_sales_drafts_and_disclosure_are_review_gated(self):
        self.module.configure_runtime(
            kanban_create_sales_task=lambda title, body, task_type, priority, key: (
                "t_sales",
                "created",
            )
        )

        lead = json.loads(
            self.module.handle_qintopia_lead_capture(
                {
                    "task_type": "demo_request",
                    "customer_display_name": "某客户",
                    "source_channel": "wechat_external",
                    "source_conversation_id": "conv_1",
                    "customer_request": "想看 Agent OS 销售客服演示。",
                    "business_scenario": "企业微信客户咨询分流。",
                }
            )
        )
        self.assertTrue(lead["success"])
        self.assertEqual(lead["task_id"], "t_sales")
        self.assertEqual(lead["actions"][0]["assignee"], "xiaoqin")
        self.assertIn("我先帮您记录下来", lead["safe_customer_message"])

        proposal = json.loads(
            self.module.handle_qintopia_proposal_outline_generate(
                {"customer_display_name": "某客户", "business_scenario": "客户想把客服咨询沉淀成任务。"}
            )
        )
        self.assertTrue(proposal["requires_human_review_before_external_send"])
        self.assertIn("草案", proposal["draft"])

        disclosure = json.loads(
            self.module.handle_qintopia_external_disclosure_filter(
                {
                    "draft_answer": "我们可以给你固定报价和 SLA，也能展示内部服务器日志。",
                    "purpose": "回复外部客户",
                }
            )
        )
        self.assertTrue(disclosure["approval_required"])
        self.assertIn("commercial_commitment", disclosure["matched_risk_categories"])
        self.assertNotIn("服务器日志", disclosure["public_safe_draft"])

    def test_lead_capture_rejects_worktool_source_channel(self):
        payload = json.loads(
            self.module.handle_qintopia_lead_capture(
                {
                    "task_type": "sales_lead",
                    "customer_display_name": "某客户",
                    "source_channel": "worktool_external_contact",
                    "source_conversation_id": "conv_1",
                    "customer_request": "想了解 Agent OS。",
                }
            )
        )

        self.assertFalse(payload["success"])
        self.assertIn("source_channel is not allowed", payload["error"])


if __name__ == "__main__":
    unittest.main()
