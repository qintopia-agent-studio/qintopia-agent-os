"""Synthetic facts against the unmodified PMS server; never loads a Profile credential."""
import datetime
import importlib.util
import json
import os
from pathlib import Path
from zoneinfo import ZoneInfo

spec = importlib.util.spec_from_file_location("pms_client", Path(__file__).parents[1] / "client.py")
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)
demo = json.loads(os.environ["ANAN_PMS_TEST_DEMO"])
pms = c.Client(os.environ["ANAN_PMS_TEST_BASE_URL"], demo["writeToken"], local_enabled=True, timeout=30)
prop = demo["propertyId"]
assert pms.me()["propertyAccess"][prop] == "WRITE"
today = datetime.datetime.now(ZoneInfo("Asia/Shanghai")).date()
arrival, departure = today.isoformat(), (today + datetime.timedelta(days=2)).isoformat()
availability = pms.read("availability", prop, filters={"arrivalDate":arrival,"departureDate":departure,"unitKind":"ROOM"})
assert availability["units"]
quote = pms.quote({"propertyId":prop,"inventoryUnitId":demo["roomId"],"stayType":"TRANSIENT",
    "arrivalDate":arrival,"departureDate":departure,"pricingPolicyVersionId":demo["transientPolicyId"]}, key="anan_test_quote",correlation="anan_test_quote")
assert quote["receipt"]["executionStatus"] == "EXECUTED"
payload = {"propertyId":prop,"quoteId":quote["quote"]["quoteId"],"primaryGuest":{"fullName":"Synthetic Anan Guest","nickname":"模拟住客"},"bookingChannelCode":"WECOM"}
preview = pms.preview("CREATE_ORDER",payload,key="anan_test_preview",correlation="anan_test_create")["preview"]
preview["propertyId"] = prop
receipt = pms.confirm(preview,{"code":"CREATE_STANDARD_ORDER","note":""},key="anan_test_create",correlation="anan_test_create")
assert receipt["executionStatus"] == "EXECUTED"
order = receipt["result"].get("order",receipt["result"])
order_id = order.get("orderId",order.get("id"))
assert order_id, list(order)
readback = pms.read("order",prop,resource=order_id)
assert readback["order"]["id"] == order_id
recovered = pms.recover(prop,"CREATE_ORDER","anan_test_create",resolve_key="anan_test_resolve",correlation="anan_test_create")
assert recovered["receiptId"] == receipt["receiptId"]
assert pms.confirm(preview,{"code":"CREATE_STANDARD_ORDER","note":""},key="anan_test_create",correlation="anan_test_create")["receiptId"] == receipt["receiptId"]
print("Real PMS: authorize, availability, quote, preview, confirm, order readback and original-key recovery passed.")
