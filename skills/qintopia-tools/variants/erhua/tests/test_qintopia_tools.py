from __future__ import annotations

import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path


def load_plugin():
    plugin_path = Path(__file__).resolve().parents[1] / "__init__.py"
    spec = importlib.util.spec_from_file_location("qintopia_tools_plugin", plugin_path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_fixture(index_dir: Path) -> None:
    index_dir.mkdir(parents=True, exist_ok=True)
    body = """# 秦托邦 GIS 位置信息

| 名称 | 经度 | 纬度 | 门头图片 |
|------|------|------|----------|
| 秦托邦社区 | 108.572935 | 34.023305 | ![社区](https://example.test/community.jpg) |
| 秦托邦1栋 | 108.572849 | 34.024317 | ![1栋](https://example.test/1.jpg) |
| 秦托邦2栋 | 108.572225 | 34.023833 | ![2栋](https://example.test/2.jpg) |
"""
    record = {
        "source_id": "gis123",
        "title": "秦托邦 GIS 位置信息",
        "path": "gis-locations.md",
        "information_class": "Public",
        "updated_at": "2026-06-01T14:41:20+00:00",
        "body": body,
    }
    product_record = {
        "source_id": "product123",
        "title": "公开 Agent OS 产品介绍 FAQ",
        "path": "agent-os-public-faq.md",
        "information_class": "Public",
        "updated_at": "2026-06-07T00:00:00+00:00",
        "body": (
            "Qintopia Agent OS 是面向组织协作场景的 Agent 工作系统。"
            "系统可以支持需求收集、知识检索、方案草拟、演示准备、任务流转和人工审批。"
        ),
    }
    (index_dir / "public.jsonl").write_text(
        json.dumps(record, ensure_ascii=False) + "\n" + json.dumps(product_record, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    (index_dir / "internal.jsonl").write_text("", encoding="utf-8")
    (index_dir / "member-scoped.jsonl").write_text("", encoding="utf-8")


class QintopiaToolsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmpdir = tempfile.TemporaryDirectory()
        self.index_dir = Path(self.tmpdir.name)
        write_fixture(self.index_dir)
        self.old_index = os.environ.get("QINTOPIA_KB_INDEX_DIR")
        os.environ["QINTOPIA_KB_INDEX_DIR"] = str(self.index_dir)
        self.old_dify_env = {
            name: os.environ.get(name)
            for name in [
                "QINTOPIA_DIFY_KB_BASE_URL",
                "QINTOPIA_DIFY_KB_API_KEY",
                "QINTOPIA_DIFY_ALLOWED_DATASET_IDS",
                "QINTOPIA_DIFY_LOOKUP_DATASET_ID",
                "QINTOPIA_PROFILE_ID",
                "QINTOPIA_DIFY_RAW_TOOLS_ENABLE",
                "QINTOPIA_MESSAGE_STORE_ENABLE",
                "QINTOPIA_MESSAGE_STORE_DATABASE_URL",
                "QINTOPIA_MESSAGE_STORE_EMBEDDING_URL",
                "QINTOPIA_MESSAGE_STORE_EMBEDDING_API_KEY",
                "QINTOPIA_MESSAGE_STORE_EMBEDDING_MODEL",
                "QINTOPIA_MESSAGE_STORE_EMBEDDING_DB_MODEL",
                "QINTOPIA_SIDECAR_DATABASE_URL",
                "QINTOPIA_DAILY_DIGEST_PUBLISH_ENABLE",
                "QINTOPIA_DAILY_DIGEST_PUBLISHER_BIN",
                "QINTOPIA_WEATHER_LOCATION",
                "QINTOPIA_WEATHER_LOCATION_NAME",
                "QINTOPIA_WEATHER_QWEATHER_CITY",
                "QINTOPIA_WEATHER_MCP_TIMEOUT_SECONDS",
                "QINTOPIA_AGENT_OS_SKILLS_DIR",
                "QINTOPIA_AGENT_OS_RELEASE_DIR",
                "QINTOPIA_AGENT_OS_MONOREPO_DIR",
            ]
        }
        os.environ["QINTOPIA_DIFY_KB_BASE_URL"] = "http://dify.example.test/v1"
        os.environ["QINTOPIA_DIFY_KB_API_KEY"] = "test-knowledge-key"
        os.environ["QINTOPIA_DIFY_ALLOWED_DATASET_IDS"] = "ds_allowed"
        os.environ.pop("QINTOPIA_DIFY_LOOKUP_DATASET_ID", None)
        os.environ.pop("QINTOPIA_PROFILE_ID", None)
        os.environ.pop("QINTOPIA_DIFY_RAW_TOOLS_ENABLE", None)
        os.environ.pop("QINTOPIA_MESSAGE_STORE_ENABLE", None)
        os.environ.pop("QINTOPIA_MESSAGE_STORE_DATABASE_URL", None)
        os.environ.pop("QINTOPIA_MESSAGE_STORE_EMBEDDING_URL", None)
        os.environ.pop("QINTOPIA_MESSAGE_STORE_EMBEDDING_API_KEY", None)
        os.environ.pop("QINTOPIA_MESSAGE_STORE_EMBEDDING_MODEL", None)
        os.environ.pop("QINTOPIA_MESSAGE_STORE_EMBEDDING_DB_MODEL", None)
        os.environ.pop("QINTOPIA_SIDECAR_DATABASE_URL", None)
        os.environ.pop("QINTOPIA_DAILY_DIGEST_PUBLISH_ENABLE", None)
        os.environ.pop("QINTOPIA_DAILY_DIGEST_PUBLISHER_BIN", None)
        os.environ.pop("QINTOPIA_WEATHER_LOCATION", None)
        os.environ.pop("QINTOPIA_WEATHER_LOCATION_NAME", None)
        os.environ.pop("QINTOPIA_WEATHER_QWEATHER_CITY", None)
        os.environ.pop("QINTOPIA_WEATHER_MCP_TIMEOUT_SECONDS", None)
        os.environ.pop("QINTOPIA_AGENT_OS_SKILLS_DIR", None)
        os.environ.pop("QINTOPIA_AGENT_OS_RELEASE_DIR", None)
        os.environ.pop("QINTOPIA_AGENT_OS_MONOREPO_DIR", None)
        self.module = load_plugin()

    def tearDown(self) -> None:
        if self.old_index is None:
            os.environ.pop("QINTOPIA_KB_INDEX_DIR", None)
        else:
            os.environ["QINTOPIA_KB_INDEX_DIR"] = self.old_index
        for name, value in self.old_dify_env.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value
        self.tmpdir.cleanup()

    def test_gis_lookup_1_building(self):
        payload = json.loads(self.module.handle_qintopia_gis_location_lookup({"query": "1 栋"}))

        self.assertTrue(payload["success"])
        self.assertTrue(payload["matched"])
        self.assertEqual(payload["location"]["name"], "秦托邦1栋")
        self.assertEqual(payload["location"]["longitude"], 108.572849)
        self.assertEqual(payload["location"]["latitude"], 34.024317)
        self.assertTrue(payload["location"]["amap_url"].startswith("https://uri.amap.com/marker?"))
        self.assertEqual(payload["scope_used"], ["Public"])

    def test_kb_search_defaults_public_only(self):
        payload = json.loads(self.module.handle_qintopia_kb_search({"query": "秦托邦1栋"}))

        self.assertTrue(payload["success"])
        self.assertEqual(payload["scope_used"], ["Public"])
        self.assertIn("Member-scoped", payload["not_accessed"])
        self.assertEqual(payload["results"][0]["path"], "gis-locations.md")

    def test_xiaoqin_product_search_is_public_only_and_has_baselines(self):
        payload = json.loads(
            self.module.handle_qintopia_external_product_kb_search(
                {"query": "Agent OS 可以做什么", "purpose": "回答外部客户"}
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["scope_used"], ["Public"])
        self.assertGreaterEqual(payload["result_count"], 1)
        self.assertEqual(payload["results"][0]["path"], "agent-os-public-faq.md")
        self.assertIn("Internal", payload["not_accessed"])
        self.assertTrue(payload["approved_public_baselines"])

    def test_xiaoqin_public_case_search_does_not_invent_cases(self):
        payload = json.loads(self.module.handle_qintopia_public_case_search({"query": "客户案例"}))

        self.assertTrue(payload["success"])
        self.assertEqual(payload["result_count"], 0)
        self.assertFalse(payload["approved_public_cases_available"])
        self.assertTrue(payload["needs_human_review"])
        self.assertIn("没有检索到已批准公开的客户案例", payload["safe_customer_message"])
        self.assertNotIn("Human Owner", payload["safe_customer_message"])

    def test_xiaoqin_customer_context_lookup_is_current_channel_only(self):
        payload = json.loads(
            self.module.handle_qintopia_customer_context_lookup(
                {
                    "customer_display_name": "某客户",
                    "source_channel": "wechat_external",
                    "source_conversation_id": "conv_1",
                    "customer_provided_context": "想了解 AI 客服试点。",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["mode"], "current_channel_context_only")
        self.assertFalse(payload["stored_context_found"])
        self.assertIn("CRM", payload["not_accessed"])

    def test_dify_read_tools_delegate_to_knowledge_retrieval_package(self):
        calls = []

        class FakeKnowledgeRetrieval:
            QINTOPIA_DIFY_DATASET_LIST_SCHEMA = {"description": "dataset"}
            QINTOPIA_DIFY_DATASET_GET_SCHEMA = {"description": "dataset get"}
            QINTOPIA_DIFY_KNOWLEDGE_RETRIEVE_SCHEMA = {"description": "retrieve"}
            QINTOPIA_DIFY_DOCUMENT_LIST_SCHEMA = {"description": "document list"}
            QINTOPIA_DIFY_DOCUMENT_GET_SCHEMA = {"description": "document get"}
            QINTOPIA_DIFY_INDEXING_STATUS_GET_SCHEMA = {"description": "indexing"}
            QINTOPIA_DIFY_SEGMENT_LIST_SCHEMA = {"description": "segment list"}
            QINTOPIA_DIFY_SEGMENT_GET_SCHEMA = {"description": "segment get"}
            QINTOPIA_WENYUANGE_LOOKUP_SCHEMA = {"description": "lookup"}

            def handle_qintopia_dify_dataset_list(self, args):
                calls.append(("dataset_list", args))
                return json.dumps({"success": True, "delegated": "dataset_list"})

            def handle_qintopia_dify_knowledge_retrieve(self, args):
                calls.append(("retrieve", args))
                return json.dumps({"success": True, "delegated": "retrieve"})

            def check_dify_read_requirements(self):
                return True

        fake_plugin = FakeKnowledgeRetrieval()
        self.module._KNOWLEDGE_RETRIEVAL_PLUGIN = fake_plugin

        list_payload = json.loads(self.module.handle_qintopia_dify_dataset_list({"limit": 50}))
        retrieve_payload = json.loads(
            self.module.handle_qintopia_dify_knowledge_retrieve(
                {"dataset_id": "ds_allowed", "query": "秦托邦是什么"}
            )
        )

        self.assertEqual(list_payload["delegated"], "dataset_list")
        self.assertEqual(retrieve_payload["delegated"], "retrieve")
        self.assertEqual(calls[0], ("dataset_list", {"limit": 50}))
        self.assertEqual(calls[1], ("retrieve", {"dataset_id": "ds_allowed", "query": "秦托邦是什么"}))
        self.assertTrue(self.module.check_dify_read_requirements())

    def test_wenyuange_lookup_delegates_to_knowledge_retrieval_package(self):
        calls = []

        class FakeKnowledgeRetrieval:
            def handle_qintopia_wenyuange_lookup(self, args):
                calls.append(args)
                return json.dumps({"success": True, "skill": "qintopia_wenyuange_lookup", "delegated": True})

        self.module._KNOWLEDGE_RETRIEVAL_PLUGIN = FakeKnowledgeRetrieval()
        payload = json.loads(
            self.module.handle_qintopia_wenyuange_lookup(
                {
                    "query": "社区 WiFi 名称是什么",
                    "caller_profile": "erhua",
                    "audience": "member_reply",
                    "purpose": "回答社区成员问题",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertTrue(payload["delegated"])
        self.assertEqual(calls[0]["query"], "社区 WiFi 名称是什么")

    def test_skill_dependency_loader_supports_explicit_skills_dir(self):
        skills_dir = self.index_dir / "agent-os-skills"
        knowledge_dir = skills_dir / "knowledge-retrieval"
        weather_dir = skills_dir / "qintopia-weather"
        knowledge_dir.mkdir(parents=True)
        weather_dir.mkdir(parents=True)
        (knowledge_dir / "__init__.py").write_text("MARKER = 'knowledge-from-explicit-skills-dir'\n", encoding="utf-8")
        (weather_dir / "__init__.py").write_text("MARKER = 'weather-from-explicit-skills-dir'\n", encoding="utf-8")
        os.environ["QINTOPIA_AGENT_OS_SKILLS_DIR"] = str(skills_dir)
        self.module._KNOWLEDGE_RETRIEVAL_PLUGIN = None
        self.module._QINTOPIA_WEATHER_PLUGIN = None

        knowledge_plugin = self.module._knowledge_retrieval_plugin()
        weather_plugin = self.module._qintopia_weather_plugin()

        self.assertEqual(knowledge_plugin.MARKER, "knowledge-from-explicit-skills-dir")
        self.assertEqual(weather_plugin.MARKER, "weather-from-explicit-skills-dir")

    def test_message_store_search_requires_wenyuange_caller(self):
        payload = json.loads(
            self.module.handle_qintopia_message_store_search(
                {"query": "端午节", "caller": "erhua", "purpose": "回答群聊问题"}
            )
        )

        self.assertFalse(payload["success"])
        self.assertIn("wenyuange", payload["error"])

    def test_message_store_search_returns_structured_messages(self):
        class FakeTime:
            def __init__(self, value):
                self.value = value

            def isoformat(self):
                return self.value

        row = {
            "id": "5b2c2e8e-3d9c-45a4-b9c1-4fe8a7c12222",
            "platform": "qiwe",
            "message_id": "msg_1",
            "chat_id": "room_1",
            "chat_type": "group",
            "sender_id": "user_1",
            "sender_name": "小秦",
            "message_kind": "text",
            "text": "今天大家在讨论端午节活动。",
            "is_mention_bot": False,
            "should_trigger": False,
            "trigger_reason": None,
            "sent_at": FakeTime("2026-06-19T10:00:00+08:00"),
            "received_at": FakeTime("2026-06-19T10:00:01+08:00"),
            "created_at": FakeTime("2026-06-19T10:00:02+08:00"),
        }
        self.module._run_message_store_search = lambda args: {
            "success": True,
            "skill": "qintopia_message_store_search",
            "source": "postgres_qintopia_messages",
            "read_only": True,
            "query": args.get("query", ""),
            "filters": {"chat_type": args.get("chat_type", "")},
            "result_count": 1,
            "messages": [self.module._message_store_row(row)],
        }
        os.environ["QINTOPIA_MESSAGE_STORE_DATABASE_URL"] = "postgres://example"
        payload = json.loads(
            self.module.handle_qintopia_message_store_search(
                {
                    "query": "端午节",
                    "chat_type": "group",
                    "caller": "wenyuange",
                    "purpose": "回答群聊近期讨论问题",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["result_count"], 1)
        self.assertEqual(payload["messages"][0]["message_id"], "msg_1")
        self.assertEqual(payload["messages"][0]["text"], "今天大家在讨论端午节活动。")

    def test_message_store_query_terms_expand_chinese_phrases(self):
        terms = self.module._message_store_query_terms("今天端午节大家都聊了什么")

        self.assertIn("端午节", terms)
        self.assertIn("端午", terms)
        self.assertNotIn("今天", terms)
        self.assertNotIn("大家", terms)

    def test_parse_embedding_payload_supports_openai_compatible_response(self):
        embedding = self.module._parse_embedding_payload(
            {"data": [{"embedding": ["0.1", 0.2, 3]}]}
        )

        self.assertEqual(embedding, [0.1, 0.2, 3.0])
        self.assertEqual(self.module._embedding_to_pgvector(embedding), "[0.1,0.2,3]")

    def test_message_store_search_hybrid_merges_semantic_keyword_and_recent(self):
        async def fake_search(args):
            class FakeTime:
                def __init__(self, value):
                    self.value = value

                def isoformat(self):
                    return self.value

            class FakeRow(dict):
                pass

            semantic_row = FakeRow(
                {
                    "id": "5b2c2e8e-3d9c-45a4-b9c1-4fe8a7c13333",
                    "platform": "qiwe",
                    "message_id": "msg_semantic",
                    "chat_id": "room_1",
                    "chat_type": "group",
                    "sender_id": "user_1",
                    "sender_name": "小秦",
                    "message_kind": "text",
                    "text": "下午有香囊手工活动。",
                    "is_mention_bot": False,
                    "should_trigger": False,
                    "trigger_reason": None,
                    "sent_at": FakeTime("2026-06-19T14:00:00+08:00"),
                    "received_at": FakeTime("2026-06-19T14:00:01+08:00"),
                    "created_at": FakeTime("2026-06-19T14:00:02+08:00"),
                    "semantic_distance": 0.12,
                }
            )
            keyword_row = FakeRow(
                {
                    "id": "5b2c2e8e-3d9c-45a4-b9c1-4fe8a7c14444",
                    "platform": "qiwe",
                    "message_id": "msg_keyword",
                    "chat_id": "room_1",
                    "chat_type": "group",
                    "sender_id": "user_2",
                    "sender_name": "希希",
                    "message_kind": "text",
                    "text": "端午香囊活动接龙。",
                    "is_mention_bot": False,
                    "should_trigger": False,
                    "trigger_reason": None,
                    "sent_at": FakeTime("2026-06-19T13:00:00+08:00"),
                    "received_at": FakeTime("2026-06-19T13:00:01+08:00"),
                    "created_at": FakeTime("2026-06-19T13:00:02+08:00"),
                }
            )
            recent_row = FakeRow(
                {
                    "id": "5b2c2e8e-3d9c-45a4-b9c1-4fe8a7c15555",
                    "platform": "qiwe",
                    "message_id": "msg_recent",
                    "chat_id": "room_1",
                    "chat_type": "group",
                    "sender_id": "user_3",
                    "sender_name": "知行",
                    "message_kind": "text",
                    "text": "收到。",
                    "is_mention_bot": False,
                    "should_trigger": False,
                    "trigger_reason": None,
                    "sent_at": FakeTime("2026-06-19T15:00:00+08:00"),
                    "received_at": FakeTime("2026-06-19T15:00:01+08:00"),
                    "created_at": FakeTime("2026-06-19T15:00:02+08:00"),
                }
            )
            query_terms = self.module._message_store_query_terms(args["query"])
            messages = []
            for method, row in [
                ("semantic", semantic_row),
                ("keyword", keyword_row),
                ("recent", recent_row),
            ]:
                item = self.module._message_store_row(row)
                item["retrieval_methods"] = [method]
                item["retrieval_score"] = {"semantic": 1088.0, "keyword": 510.0, "recent": 0.0}[method]
                if method == "semantic":
                    item["semantic_distance"] = row["semantic_distance"]
                if method == "keyword":
                    item["matched_terms"] = [term for term in query_terms if term in row["text"]]
                messages.append(item)
            return {
                "success": True,
                "skill": "qintopia_message_store_search",
                "source": "postgres_qintopia_messages",
                "read_only": True,
                "query": args["query"],
                "query_terms": query_terms,
                "search_mode": "hybrid",
                "retrieval_trace": [
                    {"search_method": "semantic", "success": True, "result_count": 1},
                    {"search_method": "keyword", "success": True, "result_count": 1},
                    {"search_method": "recent", "success": True, "result_count": 1},
                ],
                "filters": {"chat_type": args.get("chat_type", "")},
                "result_count": len(messages),
                "messages": sorted(messages, key=lambda item: item["retrieval_score"], reverse=True),
            }

        self.module._message_store_search_async = fake_search
        os.environ["QINTOPIA_MESSAGE_STORE_DATABASE_URL"] = "postgres://example"
        payload = json.loads(
            self.module.handle_qintopia_message_store_search(
                {
                    "query": "今天端午节大家聊什么",
                    "chat_type": "group",
                    "search_mode": "hybrid",
                    "caller": "wenyuange",
                    "purpose": "回答群聊近期讨论问题",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["search_mode"], "hybrid")
        self.assertIn("端午", payload["query_terms"])
        self.assertEqual(payload["messages"][0]["message_id"], "msg_semantic")
        self.assertEqual(payload["messages"][0]["retrieval_methods"], ["semantic"])
        self.assertEqual(payload["messages"][1]["matched_terms"][0], "端午")
        self.assertEqual(
            [item["search_method"] for item in payload["retrieval_trace"]],
            ["semantic", "keyword", "recent"],
        )

    def test_message_store_search_semantic_queries_pgvector_embeddings(self):
        import asyncio
        import sys
        import types

        class FakeTime:
            def __init__(self, value):
                self.value = value

            def isoformat(self):
                return self.value

        class FakeConnection:
            def __init__(self):
                self.sql = []
                self.params = []

            async def execute(self, sql):
                self.sql.append(sql)

            async def fetch(self, sql, *values):
                self.sql.append(sql)
                self.params.append(values)
                return [
                    {
                        "id": "5b2c2e8e-3d9c-45a4-b9c1-4fe8a7c16666",
                        "platform": "qiwe",
                        "message_id": "msg_vector",
                        "chat_id": "room_1",
                        "chat_type": "group",
                        "sender_id": "user_1",
                        "sender_name": "小秦",
                        "message_kind": "text",
                        "text": "端午香囊活动接龙。",
                        "is_mention_bot": False,
                        "should_trigger": False,
                        "trigger_reason": None,
                        "sent_at": FakeTime("2026-06-19T14:00:00+08:00"),
                        "received_at": FakeTime("2026-06-19T14:00:01+08:00"),
                        "created_at": FakeTime("2026-06-19T14:00:02+08:00"),
                        "semantic_distance": 0.08,
                    }
                ]

            async def close(self):
                pass

        fake_conn = FakeConnection()

        async def fake_connect(_):
            return fake_conn

        fake_asyncpg = types.SimpleNamespace(connect=fake_connect)
        old_asyncpg = sys.modules.get("asyncpg")
        sys.modules["asyncpg"] = fake_asyncpg
        self.module._message_store_query_embedding = lambda query: (
            [0.1, 0.2, 0.3],
            {
                "search_method": "query_embedding",
                "configured": True,
                "model": "test-embedding",
                "success": True,
                "dimension": 3,
            },
        )
        os.environ["QINTOPIA_MESSAGE_STORE_DATABASE_URL"] = "postgres://example"
        os.environ["QINTOPIA_MESSAGE_STORE_EMBEDDING_DB_MODEL"] = "test-embedding"
        try:
            payload = asyncio.run(
                self.module._message_store_search_async(
                    {
                        "query": "端午节",
                        "chat_type": "group",
                        "search_mode": "semantic",
                        "limit": 5,
                    }
                )
            )
        finally:
            if old_asyncpg is None:
                sys.modules.pop("asyncpg", None)
            else:
                sys.modules["asyncpg"] = old_asyncpg

        sql = "\n".join(fake_conn.sql)
        self.assertTrue(payload["success"])
        self.assertEqual(payload["messages"][0]["message_id"], "msg_vector")
        self.assertEqual(payload["messages"][0]["retrieval_methods"], ["semantic"])
        self.assertIn("qintopia_messages.message_embeddings", sql)
        self.assertIn("<=>", sql)
        self.assertIn("SET search_path TO qintopia_messages, public", sql)
        self.assertIn("[0.1,0.2,0.3]", fake_conn.params[0])
        self.assertIn("test-embedding", fake_conn.params[0])

    def test_xiaoqin_lead_capture_creates_controlled_sales_task(self):
        self.module._kanban_create_sales_task = lambda title, body, task_type, priority, key: ("t_sales", "created")

        payload = json.loads(
            self.module.handle_qintopia_lead_capture(
                {
                    "task_type": "demo_request",
                    "customer_display_name": "某客户",
                    "source_channel": "wechat_external",
                    "source_conversation_id": "conv_1",
                    "source_message_id": "msg_1",
                    "customer_request": "想看 Agent OS 销售客服演示。",
                    "business_scenario": "企业微信客户咨询分流。",
                    "budget_range": "待确认",
                    "urgency": "本月",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["task_id"], "t_sales")
        self.assertEqual(payload["task_type"], "demo_request")
        action = payload["actions"][0]
        self.assertEqual(action["action"], "kanban_task_create_request")
        self.assertEqual(action["assignee"], "xiaoqin")
        self.assertEqual(action["status"], "triage")
        self.assertIn("safe_customer_message", payload)
        self.assertIn("我先帮您记录下来", payload["safe_customer_message"])
        self.assertNotIn("t_sales", payload["safe_customer_message"])
        self.assertNotIn("任务", payload["safe_customer_message"])
        self.assertNotIn("复核", payload["safe_customer_message"])
        self.assertIn("Use safe_customer_message", payload["customer_response_policy"][0])
        self.assertNotIn("Human Owner", json.dumps(payload, ensure_ascii=False))
        self.assertNotIn("kanban_assign", json.dumps(payload, ensure_ascii=False))

    def test_xiaoqin_sales_task_create_uses_hermes_initial_status_contract(self):
        captured = []

        class FakeConnection:
            def close(self):
                captured.append({"closed": True})

        class FakeKanban:
            def create_task(self, conn, **kwargs):
                captured.append(kwargs)
                return "t_sales"

        self.module._kanban_runtime = lambda: (FakeKanban(), FakeConnection())

        task_id, status = self.module._kanban_create_sales_task("标题", "正文", "proposal", 1, "key-1")
        self.assertEqual(task_id, "t_sales")
        self.assertEqual(status, "created")
        self.assertEqual(captured[0]["initial_status"], "running")
        self.assertFalse(captured[0]["triage"])

        captured.clear()
        self.module._kanban_create_sales_task("标题", "正文", "external_disclosure_review", 1, "key-2")
        self.assertEqual(captured[0]["initial_status"], "blocked")

    def test_xiaoqin_lead_capture_rejects_uncontrolled_task_type(self):
        payload = json.loads(
            self.module.handle_qintopia_lead_capture(
                {
                    "task_type": "engineering",
                    "source_channel": "wechat_external",
                    "source_conversation_id": "conv_1",
                    "customer_request": "帮我改服务器。",
                }
            )
        )

        self.assertFalse(payload["success"])
        self.assertIn("not allowed", payload["error"])

    def test_xiaoqin_proposal_and_demo_are_review_gated(self):
        proposal = json.loads(
            self.module.handle_qintopia_proposal_outline_generate(
                {
                    "customer_display_name": "某客户",
                    "business_scenario": "客户想把客服咨询沉淀成任务。",
                    "goals": "降低漏跟进。",
                }
            )
        )
        demo = json.loads(
            self.module.handle_qintopia_demo_script_generate(
                {
                    "demo_goal": "展示需求收集到任务交接",
                    "business_scenario": "企业微信销售客服咨询。",
                    "allowed_materials": "公开样例。",
                }
            )
        )

        self.assertTrue(proposal["success"])
        self.assertTrue(proposal["requires_human_review_before_external_send"])
        self.assertIn("草案", proposal["draft"])
        self.assertTrue(demo["success"])
        self.assertTrue(demo["requires_human_review_before_external_send"])
        self.assertIn("公开样例", demo["script"])

    def test_xiaoqin_disclosure_filter_blocks_sensitive_draft(self):
        payload = json.loads(
            self.module.handle_qintopia_external_disclosure_filter(
                {
                    "draft_answer": "我们可以给你固定报价和 SLA，也能展示内部服务器日志。",
                    "purpose": "回复外部客户",
                    "recipient": "客户",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertTrue(payload["approval_required"])
        self.assertIn("commercial_commitment", payload["matched_risk_categories"])
        self.assertIn("internal_information", payload["matched_risk_categories"])
        self.assertNotIn("服务器日志", payload["public_safe_draft"])

    def test_xiaoqin_conversation_summary_suggests_disclosure_review_on_risk(self):
        payload = json.loads(
            self.module.handle_qintopia_conversation_summary(
                {
                    "conversation_text": "客户想看内部客户案例和报价合同。",
                    "customer_display_name": "某客户",
                    "source_channel": "wechat_external",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["suggested_task_type"], "external_disclosure_review")
        self.assertIn("需要团队负责人决策", payload["summary"])
        self.assertNotIn("Human Owner", payload["summary"])

    def test_complaint_intake_create_is_controlled(self):
        self.module._kanban_create_complaint = lambda title, body, priority, key: ("t_test", "created")

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
        self.assertEqual(kanban_action["action"], "kanban_task_create_request")
        self.assertEqual(kanban_action["task_type"], "complaint_intake")
        self.assertEqual(kanban_action["assignee"], "default")
        self.assertEqual(kanban_action["status"], "triage")
        self.assertNotIn("silaoshi", json.dumps(payload, ensure_ascii=False))
        private_action = payload["actions"][1]
        self.assertEqual(private_action["tool"], "qiwe_send_direct_message")
        self.assertEqual(private_action["recipient_user_id"], "user_1")
        self.assertEqual(private_action["conversation_scope"], "private")
        self.assertEqual(private_action["purpose"], "complaint_intake_detail_collection")
        self.assertEqual(private_action["idempotency_key"], f'{kanban_action["idempotency_key"]}:direct:intake')
        self.assertIn("为了避免在群里公开你的细节", private_action["message"])

    def test_complaint_intake_create_uses_qiwe_session_sender_id(self):
        old_user_id = os.environ.get("HERMES_SESSION_USER_ID")
        captured = {}

        def fake_create(title, body, priority, key):
            captured["body"] = body
            captured["key"] = key
            return "t_test", "created"

        self.module._kanban_create_complaint = fake_create
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
        self.assertEqual(private_action["purpose"], "complaint_intake_detail_collection")
        self.assertEqual(private_action["idempotency_key"], f"{captured['key']}:direct:intake")

    def test_complaint_intake_update_appends_comment_only(self):
        self.module._kanban_add_complaint_comment = lambda task_id, body: (12, "comment_added")

        payload = json.loads(
            self.module.handle_qintopia_complaint_intake_update(
                {
                    "task_id": "t_test",
                    "requester_display_name": "小秦",
                    "details": "昨晚 11 点后 2 栋走廊持续很吵。",
                    "location_or_area": "2 栋走廊",
                    "expected_resolution": "希望有人回复处理方式。",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["task_id"], "t_test")
        self.assertEqual(payload["comment_id"], 12)
        action = payload["actions"][0]
        self.assertEqual(action["action"], "kanban_comment_add_request")
        self.assertTrue(action["does_not_assign_executor"])
        self.assertNotIn("kanban_assign", json.dumps(payload, ensure_ascii=False))

    def test_complaint_followup_requires_approved_resolution_and_private_user(self):
        missing = json.loads(
            self.module.handle_qintopia_complaint_followup_send(
                {
                    "task_id": "t_test",
                    "requester_channel_user_id": "user_1",
                    "approved_resolution": "",
                }
            )
        )
        self.assertFalse(missing["success"])

        self.module._kanban_add_complaint_comment = lambda task_id, body: (13, "comment_added")
        payload = json.loads(
            self.module.handle_qintopia_complaint_followup_send(
                {
                    "task_id": "t_test",
                    "requester_channel_user_id": "user_1",
                    "requester_display_name": "小秦",
                    "approved_resolution": "已安排工作人员检查并完成走廊夜间提醒。",
                }
            )
        )

        self.assertTrue(payload["success"])
        action = payload["actions"][0]
        self.assertEqual(action["tool"], "qiwe_send_direct_message")
        self.assertEqual(action["recipient_user_id"], "user_1")
        self.assertEqual(action["conversation_scope"], "private")
        self.assertEqual(action["purpose"], "complaint_resolution_followup")
        self.assertIn("idempotency_key", action)
        self.assertTrue(action["requires_approved_resolution"])
        self.assertIn("已安排工作人员检查", action["message"])

    def test_daily_digest_publish_is_disabled_by_default(self):
        os.environ["QINTOPIA_PROFILE_ID"] = "xiaoman"

        payload = json.loads(
            self.module.handle_qintopia_daily_digest_publish(
                {
                    "digest_id": "00000000-0000-0000-0000-000000000001",
                    "actor_agent": "xiaoman",
                }
            )
        )

        self.assertFalse(payload["success"])
        self.assertEqual(payload["skill"], "qintopia_daily_digest_publish")
        self.assertIn("QINTOPIA_DAILY_DIGEST_PUBLISH_ENABLE", payload["error"])

    def test_daily_digest_publish_returns_narrow_worker_command_when_enabled(self):
        os.environ["QINTOPIA_PROFILE_ID"] = "xiaoman"
        os.environ["QINTOPIA_DAILY_DIGEST_PUBLISH_ENABLE"] = "1"
        os.environ["QINTOPIA_DAILY_DIGEST_PUBLISHER_BIN"] = "/opt/qintopia-agentos-worker"

        payload = json.loads(
            self.module.handle_qintopia_daily_digest_publish(
                {
                    "digest_id": "00000000-0000-0000-0000-000000000001",
                    "actor_agent": "xiaoman",
                    "dry_run": True,
                }
            )
        )

        self.assertTrue(payload["success"])
        action = payload["action"]
        self.assertEqual(action["tool"], "agentos_worker_command")
        self.assertEqual(action["command"][0], "/opt/qintopia-agentos-worker")
        self.assertIn("daily-digest-publish", action["command"])
        self.assertIn("--digest-id", action["command"])
        self.assertIn("--dry-run", action["command"])
        self.assertNotIn("markdown", payload)

    def test_weather_lookup_delegates_to_qintopia_weather_skill(self):
        class FakeWeatherPlugin:
            QINTOPIA_WEATHER_LOOKUP_SCHEMA = {"description": "fake", "parameters": {}}

            def __init__(self) -> None:
                self.calls = []

            def handle_qintopia_weather_lookup(self, args):
                self.calls.append(args)
                return json.dumps({"success": True, "source": "fake-weather-skill"})

            def check_weather_lookup_requirements(self):
                return True

        fake_plugin = FakeWeatherPlugin()
        self.module._QINTOPIA_WEATHER_PLUGIN = fake_plugin

        payload = json.loads(
            self.module.handle_qintopia_weather_lookup(
                {"intent": "umbrella", "hours": 24}
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["source"], "fake-weather-skill")
        self.assertEqual(fake_plugin.calls, [{"intent": "umbrella", "hours": 24}])
        self.assertTrue(self.module.check_weather_lookup_requirements())

    def test_register_exposes_frontline_tools_without_raw_dify_by_default(self):
        class FakeCtx:
            def __init__(self) -> None:
                self.names = []

            def register_tool(self, **kwargs) -> None:
                self.names.append(kwargs["name"])

        os.environ["QINTOPIA_PROFILE_ID"] = "xiaoqin"
        os.environ.pop("QINTOPIA_DIFY_RAW_TOOLS_ENABLE", None)
        ctx = FakeCtx()
        self.module.register(ctx)

        self.assertIn("qintopia_wenyuange_lookup", ctx.names)
        self.assertIn("qintopia_weather_lookup", ctx.names)
        self.assertIn("qintopia_complaint_intake_create", ctx.names)
        self.assertIn("qintopia_complaint_intake_update", ctx.names)
        self.assertIn("qintopia_complaint_followup_send", ctx.names)
        self.assertIn("qintopia_external_product_kb_search", ctx.names)
        self.assertIn("qintopia_public_case_search", ctx.names)
        self.assertIn("qintopia_customer_context_lookup", ctx.names)
        self.assertIn("qintopia_lead_capture", ctx.names)
        self.assertIn("qintopia_proposal_outline_generate", ctx.names)
        self.assertIn("qintopia_demo_script_generate", ctx.names)
        self.assertIn("qintopia_external_disclosure_filter", ctx.names)
        self.assertIn("qintopia_conversation_summary", ctx.names)
        self.assertIn("qintopia_daily_digest_publish", ctx.names)
        self.assertNotIn("qintopia_dify_dataset_list", ctx.names)
        self.assertNotIn("qintopia_dify_knowledge_retrieve", ctx.names)
        self.assertNotIn("qintopia_message_store_search", ctx.names)

    def test_register_exposes_raw_dify_only_for_wenyuange_opt_in(self):
        class FakeCtx:
            def __init__(self) -> None:
                self.names = []

            def register_tool(self, **kwargs) -> None:
                self.names.append(kwargs["name"])

        os.environ["QINTOPIA_PROFILE_ID"] = "wenyuange"
        os.environ["QINTOPIA_DIFY_RAW_TOOLS_ENABLE"] = "1"
        os.environ["QINTOPIA_MESSAGE_STORE_ENABLE"] = "1"
        ctx = FakeCtx()
        self.module.register(ctx)

        self.assertIn("qintopia_wenyuange_lookup", ctx.names)
        self.assertIn("qintopia_dify_dataset_list", ctx.names)
        self.assertIn("qintopia_dify_dataset_get", ctx.names)
        self.assertIn("qintopia_dify_knowledge_retrieve", ctx.names)
        self.assertIn("qintopia_dify_document_list", ctx.names)
        self.assertIn("qintopia_dify_document_get", ctx.names)
        self.assertIn("qintopia_dify_indexing_status_get", ctx.names)
        self.assertIn("qintopia_dify_segment_list", ctx.names)
        self.assertIn("qintopia_dify_segment_get", ctx.names)
        self.assertIn("qintopia_message_store_search", ctx.names)


if __name__ == "__main__":
    unittest.main()
