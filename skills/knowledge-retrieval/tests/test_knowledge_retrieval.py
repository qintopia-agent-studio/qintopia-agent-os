from __future__ import annotations

import importlib.util
import json
import os
import unittest
from pathlib import Path


def load_plugin():
    plugin_path = Path(__file__).resolve().parents[1] / "__init__.py"
    spec = importlib.util.spec_from_file_location("knowledge_retrieval_plugin", plugin_path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class KnowledgeRetrievalTest(unittest.TestCase):
    def setUp(self) -> None:
        self.old_env = {
            name: os.environ.get(name)
            for name in [
                "QINTOPIA_DIFY_KB_BASE_URL",
                "QINTOPIA_DIFY_KB_API_KEY",
                "QINTOPIA_DIFY_ALLOWED_DATASET_IDS",
                "QINTOPIA_DIFY_LOOKUP_DATASET_ID",
            ]
        }
        os.environ["QINTOPIA_DIFY_KB_BASE_URL"] = "http://dify.example.test/v1"
        os.environ["QINTOPIA_DIFY_KB_API_KEY"] = "test-knowledge-key"
        os.environ["QINTOPIA_DIFY_ALLOWED_DATASET_IDS"] = "ds_allowed"
        os.environ.pop("QINTOPIA_DIFY_LOOKUP_DATASET_ID", None)
        self.module = load_plugin()

    def tearDown(self) -> None:
        for name, value in self.old_env.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value

    def test_dify_dataset_list_filters_configured_allowlist(self):
        os.environ["QINTOPIA_DIFY_ALLOWED_DATASET_IDS"] = "ds_allowed"

        def fake_request(method, path, *, params=None, body=None):
            self.assertEqual(method, "GET")
            self.assertEqual(path, "/datasets")
            self.assertEqual(params["page"], 1)
            return {
                "success": True,
                "status": 200,
                "data": {
                    "data": [
                        {"id": "ds_allowed", "name": "Allowed"},
                        {"id": "ds_other", "name": "Other"},
                    ],
                    "total": 2,
                },
            }

        self.module._dify_request = fake_request
        payload = json.loads(self.module.handle_qintopia_dify_dataset_list({"limit": 50}))

        self.assertTrue(payload["success"])
        self.assertTrue(payload["read_only"])
        self.assertTrue(payload["filtered_to_allowed_datasets"])
        self.assertEqual(payload["data"]["data"], [{"id": "ds_allowed", "name": "Allowed"}])
        self.assertTrue(payload["data"]["filtered_by_allowlist"])

    def test_dify_retrieve_uses_fixed_read_only_endpoint(self):
        captured = {}

        def fake_request(method, path, *, params=None, body=None):
            captured.update({"method": method, "path": path, "params": params, "body": body})
            return {
                "success": True,
                "status": 200,
                "data": {"records": [{"segment": {"content": "秦托邦知识片段"}, "score": 0.91}]},
            }

        self.module._dify_request = fake_request
        payload = json.loads(
            self.module.handle_qintopia_dify_knowledge_retrieve(
                {
                    "dataset_id": "ds_allowed",
                    "query": "秦托邦是什么",
                    "top_k": 5,
                    "score_threshold_enabled": False,
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(captured["method"], "POST")
        self.assertEqual(captured["path"], "/datasets/ds_allowed/retrieve")
        self.assertIsNone(captured["params"])
        self.assertEqual(captured["body"]["query"], "秦托邦是什么")
        self.assertEqual(captured["body"]["retrieval_model"]["search_method"], "semantic_search")
        self.assertEqual(captured["body"]["retrieval_model"]["top_k"], 5)
        self.assertFalse(captured["body"]["retrieval_model"]["score_threshold_enabled"])
        self.assertFalse(captured["body"]["retrieval_model"]["reranking_enable"])
        self.assertTrue(payload["read_only"])
        self.assertNotIn("test-knowledge-key", json.dumps(payload, ensure_ascii=False))

    def test_dify_read_tools_block_unallowed_dataset_before_network(self):
        def fail_request(*args, **kwargs):
            raise AssertionError("network should not be called for denied dataset")

        self.module._dify_request = fail_request
        payload = json.loads(
            self.module.handle_qintopia_dify_document_list(
                {"dataset_id": "ds_denied", "page": 1, "limit": 10}
            )
        )

        self.assertFalse(payload["success"])
        self.assertEqual(payload["dataset_id"], "ds_denied")
        self.assertIn("allowlist", payload["error"])

    def test_wenyuange_lookup_returns_safe_basis_without_raw_long_chunk(self):
        long_content = "秦托邦有共享办公区、活动空间和来访须知。" * 30

        def fake_retrieve(args, **kwargs):
            return json.dumps(
                {
                    "success": True,
                    "data": {
                        "records": [
                            {
                                "score": 0.82,
                                "segment": {
                                    "id": "seg_1",
                                    "document_id": "doc_1",
                                    "content": long_content,
                                    "document": {"name": "公开 FAQ.md"},
                                },
                            }
                        ]
                    },
                },
                ensure_ascii=False,
            )

        self.module.handle_qintopia_dify_knowledge_retrieve = fake_retrieve
        self.module.handle_qintopia_dify_document_list = lambda args, **kwargs: json.dumps(
            {"success": True, "data": {"data": []}},
            ensure_ascii=False,
        )
        payload = json.loads(
            self.module.handle_qintopia_wenyuange_lookup(
                {
                    "query": "来访前要知道什么",
                    "caller_profile": "erhua",
                    "audience": "member_reply",
                    "purpose": "回答社区成员问题",
                    "top_k": 3,
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertTrue(payload["can_answer"])
        self.assertEqual(payload["result_count"], 1)
        self.assertLessEqual(len(payload["answer_basis"]), 1000)
        self.assertNotEqual(payload["answer_basis"], long_content)
        self.assertEqual(payload["sources"][0]["segment_id"], "seg_1")
        self.assertNotIn("test-knowledge-key", json.dumps(payload, ensure_ascii=False))

    def test_wenyuange_lookup_blocks_xiaoqin_internal_or_member_content(self):
        def fake_retrieve(args, **kwargs):
            return json.dumps(
                {
                    "success": True,
                    "data": {
                        "records": [
                            {
                                "score": 0.72,
                                "segment": {
                                    "id": "seg_2",
                                    "document_id": "doc_2",
                                    "content": "内部客户案例涉及成员资料和服务器日志，未公开。",
                                    "document": {"name": "internal-case.md"},
                                },
                            }
                        ]
                    },
                },
                ensure_ascii=False,
            )

        self.module.handle_qintopia_dify_knowledge_retrieve = fake_retrieve
        self.module.handle_qintopia_dify_document_list = lambda args, **kwargs: json.dumps(
            {"success": True, "data": {"data": []}},
            ensure_ascii=False,
        )
        payload = json.loads(
            self.module.handle_qintopia_wenyuange_lookup(
                {
                    "query": "能介绍客户案例吗",
                    "caller_profile": "xiaoqin",
                    "audience": "external_customer",
                    "purpose": "回答外部客户",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertFalse(payload["can_answer"])
        self.assertEqual(payload["answer_basis"], "")
        self.assertIn("member_scoped", payload["risk_flags"])
        self.assertIn("internal_information", payload["risk_flags"])

    def test_wenyuange_lookup_blocks_erhua_member_privacy_and_complaint(self):
        def fake_retrieve(args, **kwargs):
            return json.dumps(
                {
                    "success": True,
                    "data": {
                        "records": [
                            {
                                "score": 0.66,
                                "segment": {
                                    "id": "seg_3",
                                    "document_id": "doc_3",
                                    "content": "成员档案包含房间、生日和入住时间。",
                                    "document": {"name": "profiles.md"},
                                },
                            }
                        ]
                    },
                },
                ensure_ascii=False,
            )

        self.module.handle_qintopia_dify_knowledge_retrieve = fake_retrieve
        self.module.handle_qintopia_dify_document_list = lambda args, **kwargs: json.dumps(
            {"success": True, "data": {"data": []}},
            ensure_ascii=False,
        )
        payload = json.loads(
            self.module.handle_qintopia_wenyuange_lookup(
                {
                    "query": "我要投诉入住体验不好，问一下他的房间",
                    "caller_profile": "erhua",
                    "audience": "member_reply",
                    "purpose": "回答社区群消息",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertFalse(payload["can_answer"])
        self.assertIn("member_privacy", payload["risk_flags"])
        self.assertIn("complaint_or_service_recovery", payload["risk_flags"])

    def test_wenyuange_lookup_allows_public_wifi_even_with_boundary_terms(self):
        def fake_retrieve(args, **kwargs):
            return json.dumps(
                {
                    "success": True,
                    "data": {
                        "records": [
                            {
                                "score": 0.82,
                                "segment": {
                                    "id": "seg_wifi",
                                    "document_id": "doc_wifi",
                                    "content": (
                                        "秦托邦公共设施 WiFi 信息\n"
                                        "信息分级：Public / member-facing\n"
                                        "社区 WiFi 名称：秦托邦5G\n"
                                        "社区 WiFi 密码：yiqichuangzao（一起创造小全拼）"
                                    ),
                                    "document": {"name": "秦托邦公共设施 WiFi 信息"},
                                },
                            },
                            {
                                "score": 0.43,
                                "segment": {
                                    "id": "seg_boundary",
                                    "document_id": "doc_boundary",
                                    "content": "公开边界：不公开个人精确住址、房间号、门牌。",
                                    "document": {"name": "places.md"},
                                },
                            },
                        ]
                    },
                },
                ensure_ascii=False,
            )

        self.module.handle_qintopia_dify_knowledge_retrieve = fake_retrieve
        self.module.handle_qintopia_dify_document_list = lambda args, **kwargs: json.dumps(
            {"success": True, "data": {"data": []}},
            ensure_ascii=False,
        )
        payload = json.loads(
            self.module.handle_qintopia_wenyuange_lookup(
                {
                    "query": "秦托邦 WiFi 密码是多少",
                    "caller_profile": "erhua",
                    "audience": "member_reply",
                    "purpose": "回答社区成员关于公共设施 WiFi 的问题",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertTrue(payload["can_answer"])
        self.assertEqual(payload["risk_flags"], [])
        self.assertIn("秦托邦5G", payload["answer_basis"])

    def test_wenyuange_lookup_uses_document_keyword_fallback_for_safe_candidate(self):
        retrieve_calls = []
        document_calls = []
        segment_calls = []

        def fake_retrieve(args, **kwargs):
            retrieve_calls.append((args["search_method"], args["query"]))
            records = [
                {
                    "score": 0.42,
                    "segment": {
                        "id": "seg_profile",
                        "document_id": "doc_profile",
                        "content": "成员档案包含房间和入住时间。",
                        "document": {"name": "profiles.md"},
                    },
                }
            ]
            return json.dumps({"success": True, "data": {"records": records}}, ensure_ascii=False)

        def fake_document_list(args, **kwargs):
            document_calls.append(args.get("keyword"))
            if str(args.get("keyword")).lower() == "wifi":
                return json.dumps(
                    {
                        "success": True,
                        "data": {"data": [{"id": "doc_wifi", "name": "秦托邦公共设施 WiFi 信息"}]},
                    },
                    ensure_ascii=False,
                )
            return json.dumps({"success": True, "data": {"data": []}}, ensure_ascii=False)

        def fake_segment_list(args, **kwargs):
            segment_calls.append((args.get("document_id"), args.get("keyword")))
            return json.dumps(
                {
                    "success": True,
                    "data": {
                        "data": [
                            {
                                "id": "seg_wifi",
                                "content": (
                                    "秦托邦公共设施 WiFi 信息\n"
                                    "信息分级：Public / member-facing\n"
                                    "社区 WiFi 名称：秦托邦5G\n"
                                    "社区 WiFi 密码：yiqichuangzao（一起创造小全拼）"
                                ),
                            }
                        ]
                    },
                },
                ensure_ascii=False,
            )

        self.module.handle_qintopia_dify_knowledge_retrieve = fake_retrieve
        self.module.handle_qintopia_dify_document_list = fake_document_list
        self.module.handle_qintopia_dify_segment_list = fake_segment_list
        payload = json.loads(
            self.module.handle_qintopia_wenyuange_lookup(
                {
                    "query": "秦托邦 WiFi 密码是多少",
                    "caller_profile": "erhua",
                    "audience": "member_reply",
                    "purpose": "回答社区成员关于公共设施 WiFi 的问题",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertTrue(payload["can_answer"])
        self.assertIn(("semantic_search", "秦托邦 WiFi 密码是多少"), retrieve_calls)
        self.assertIn("wifi", [str(item).lower() for item in document_calls])
        self.assertIn(("doc_wifi", "wifi"), segment_calls)
        self.assertEqual(payload["risk_flags"], [])
        self.assertEqual(payload["blocked_result_count"], 1)
        self.assertIn("秦托邦5G", payload["answer_basis"])
        self.assertTrue(any(item["search_method"] == "document_keyword" for item in payload["retrieval_trace"]))

    def test_wenyuange_lookup_filters_member_sources_before_answering(self):
        def fake_retrieve(args, **kwargs):
            return json.dumps(
                {
                    "success": True,
                    "data": {
                        "records": [
                            {
                                "score": 0.91,
                                "segment": {
                                    "id": "seg_profile",
                                    "document_id": "doc_profile",
                                    "content": "普通群聊参与者。",
                                    "document": {"name": "groups / test / profiles / user.md"},
                                },
                            },
                            {
                                "score": 0.7,
                                "segment": {
                                    "id": "seg_public",
                                    "document_id": "doc_public",
                                    "content": "公共设施说明：社区网络名称为 QinTopia-Guest。",
                                    "document": {"name": "groups / test / wiki / public-facilities.md"},
                                },
                            },
                        ]
                    },
                },
                ensure_ascii=False,
            )

        self.module.handle_qintopia_dify_knowledge_retrieve = fake_retrieve
        self.module.handle_qintopia_dify_document_list = lambda args, **kwargs: json.dumps(
            {"success": True, "data": {"data": []}},
            ensure_ascii=False,
        )
        payload = json.loads(
            self.module.handle_qintopia_wenyuange_lookup(
                {
                    "query": "社区网络名称是什么",
                    "caller_profile": "erhua",
                    "audience": "member_reply",
                    "purpose": "回答社区成员关于公共设施的问题",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertTrue(payload["can_answer"])
        self.assertIn("QinTopia-Guest", payload["answer_basis"])
        self.assertNotIn("profiles", json.dumps(payload["sources"], ensure_ascii=False))
        self.assertEqual(payload["blocked_result_count"], 1)

    def test_wenyuange_lookup_prefers_authoritative_public_source_over_digest(self):
        def fake_retrieve(args, **kwargs):
            return json.dumps(
                {
                    "success": True,
                    "data": {
                        "records": [
                            {
                                "score": 0.95,
                                "segment": {
                                    "id": "seg_digest",
                                    "document_id": "doc_digest",
                                    "content": "日更摘要：有人问过 WiFi 密码，但没有查稳。",
                                    "document": {"name": "DifyRadio-qintuobang-2026-06-09.md"},
                                },
                            },
                            {
                                "score": 0.3,
                                "segment": {
                                    "id": "seg_wifi",
                                    "document_id": "doc_wifi",
                                    "content": (
                                        "信息分级：Public / member-facing\n"
                                        "公共设施说明：社区 WiFi 名称为 QinTopia-Guest。"
                                    ),
                                    "document": {"name": "wiki / public / 秦托邦公共设施.md"},
                                },
                            },
                            {
                                "score": 0.8,
                                "segment": {
                                    "id": "seg_stub",
                                    "document_id": "doc_stub",
                                    "content": "status: stub\nrisk: internal\n待补全。",
                                    "document": {"name": "wiki / topics / 社区日常.md"},
                                },
                            },
                        ]
                    },
                },
                ensure_ascii=False,
            )

        self.module.handle_qintopia_dify_knowledge_retrieve = fake_retrieve
        self.module.handle_qintopia_dify_document_list = lambda args, **kwargs: json.dumps(
            {"success": True, "data": {"data": []}},
            ensure_ascii=False,
        )
        payload = json.loads(
            self.module.handle_qintopia_wenyuange_lookup(
                {
                    "query": "社区 WiFi 名称是什么",
                    "caller_profile": "erhua",
                    "audience": "member_reply",
                    "purpose": "回答社区成员关于公共设施的问题",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertTrue(payload["can_answer"])
        self.assertIn("QinTopia-Guest", payload["answer_basis"])
        self.assertNotIn("没有查稳", payload["answer_basis"])
        self.assertEqual(payload["sources"][0]["document_id"], "doc_wifi")
        self.assertEqual(len(payload["sources"]), 1)
        self.assertEqual(payload["blocked_result_count"], 2)


if __name__ == "__main__":
    unittest.main()
