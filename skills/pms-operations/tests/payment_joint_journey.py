"""Opt-in joint acceptance against the sender-owned local fixture; never resets PMS."""
import importlib.util
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace
import uuid

if os.environ.get("QINTOPIA_PAYMENT_JOINT_ENABLE") != "1":
    raise SystemExit("explicit_payment_joint_required")
sys.path.insert(0, os.environ["ANAN_HERMES_SOURCE"])
from gateway.session_context import set_session_vars, clear_session_vars
spec = importlib.util.spec_from_file_location("anan_payment_joint_plugin", Path(__file__).parents[1] / "__init__.py")
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)
fixture = json.loads(Path(os.environ["QINTOPIA_PAYMENT_JOINT_FIXTURE"]).read_text())
agent = json.loads(Path(os.environ["QINTOPIA_PAYMENT_JOINT_CONFIG"]).read_text())
state_file = Path(os.environ["QINTOPIA_PAYMENT_JOINT_RESULT"])
if state_file.exists():
    raise SystemExit("existing_joint_result_preserved; inspect saved action before recovery")
client = p.client.Client("http://127.0.0.1:18448", fixture["writeToken"], local_enabled=True)
ops = p.Operations(client)
run_id = uuid.uuid4().hex
turn = 0


def capture(text):
    global turn
    turn += 1
    event = SimpleNamespace(source=SimpleNamespace(platform="wecom", chat_type="dm", chat_id="synthetic_chat",
        user_id=agent["sender"], is_bot=False, profile="anan"), internal=False,
        message_id=f"joint_{run_id}_{turn}", text=text)
    assert p.host.capture(event, p.transport) is None
    set_session_vars(platform="wecom", chat_type="dm", chat_id="synthetic_chat", user_id=agent["sender"],
        message_id=event.message_id, profile="anan")


def invoke(name, args):
    return ops.invoke(name, args)["result"]


capture("请核对这笔模拟收款，先准备具体方案")
bill = os.environ["QINTOPIA_PAYMENT_JOINT_BILL"]
work = os.environ["QINTOPIA_PAYMENT_JOINT_WORK"]
items = invoke("read", {"binding": agent["binding"], "query": "payments", "filters": {"billId": bill, "kind": "COLLECTION", "status": "ALL"}})["items"]
assert len(items) == 1 and items[0]["id"] == bill and items[0]["status"] == "AVAILABLE"
payment = items[0]
before = client.read("order", fixture["propertyId"], resource=fixture["orderId"])
prepared = invoke("prepare", {"binding": agent["binding"], "operation": "pms.command.RECORD_COLLECTION",
    "work_item": work, "input": {"orderId": fixture["orderId"], "method": "WECOM",
        "transactionReference": payment["reference"], "amountMinor": payment["amountMinor"]},
    "reason": {"code": "AGENT_HTTP_ACCEPTANCE", "note": "模拟有权人员确认具体订单流水金额"}})
assert prepared["phase"] == "awaiting_confirmation"
state = {"action": prepared["action"], "work_item": work, "bill_id": bill, "phase": "prepared"}
state_file.write_text(json.dumps(state))
try:
    invoke("execute", {"action": prepared["action"]})
except ValueError as e:
    assert str(e) == "human_confirmation_required"
else:
    raise AssertionError("payment event executed without human confirmation")
assert client.read("payments", fixture["propertyId"], filters={"billId": bill, "kind": "COLLECTION", "status": "ALL"})["items"][0]["status"] == "AVAILABLE"
# Explicit simulated authorized human turn; event/long-term grant alone never confirms.
capture(f'请为订单「{fixture["orderId"]}」登记企微收款{payment["amountMinor"]/100:.2f}元，流水号「{payment["reference"]}」。')
# The naturally phrased current-scheme confirmation is the explicit final decision.
capture("确认登记这笔收款")


class LoseResponse(p.client.Client):
    def confirm(self, *args, **kwargs):
        super().confirm(*args, **kwargs)
        raise p.client.PmsError("pms_outcome_unknown", outcome_unknown=True)


lost = p.Operations(LoseResponse("http://127.0.0.1:18448", fixture["writeToken"], local_enabled=True))
unknown = lost.invoke("execute", {"action": prepared["action"]})["result"]
assert unknown["phase"] == "unknown"
state["phase"] = "unknown"
state_file.write_text(json.dumps(state))
result = invoke("recover", {"action": prepared["action"]})
assert result["phase"] == "completed" and result["readback"]["order"]["id"] == fixture["orderId"]
matched = client.read("payments", fixture["propertyId"], filters={"billId": bill, "kind": "COLLECTION", "status": "ALL"})["items"][0]
assert matched["status"] == "MATCHED" and matched["orderId"] == fixture["orderId"]
try:
    invoke("execute", {"action": prepared["action"]})
except ValueError:
    pass
else:
    raise AssertionError("completed financial action was executed twice")
state.update(phase="completed", no_confirmation_denied=True, lost_response_recovered=True,
    payment_matched=True, actual_order_readback=True, receipt_id=result["result"]["receiptId"])
state_file.write_text(json.dumps(state))
clear_session_vars([])
print(json.dumps(state))
