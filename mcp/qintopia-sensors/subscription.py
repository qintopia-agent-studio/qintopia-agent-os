"""Read the existing Tuya subscription, preserving its encrypted message and ACK contract."""

import base64
import hashlib
import json
import logging
import os
from pathlib import Path

import sensors


ALLOWED = {"smoke_sensor_state", "co_state", "checking_result", "fault", "battery_state", "link_alarm"}


def fireplace_id():
    devices, _ = sensors.load_registry()
    matches = [d["device_id"] for d in devices
               if d["profile"] == "smoke-co-alarm" and d["area"] == "一楼壁炉区"]
    if len(matches) != 1:
        raise ValueError("expected exactly one registered fireplace smoke/CO detector")
    return matches[0]


def decode(message, key):
    from Crypto.Cipher import AES

    if message.properties().get("em") != "aes_gcm":
        raise ValueError("unexpected encryption mode")
    raw = base64.b64decode(json.loads(message.data())["data"], validate=True)
    cipher = AES.new(key, AES.MODE_GCM, nonce=raw[:12])
    return json.loads(cipher.decrypt_and_verify(raw[12:-16], raw[-16:]))


def properties(event, device_id):
    if event.get("bizCode") != "devicePropertyMessage":
        return None
    data = event["bizData"]
    if data["devId"] != device_id:
        return None
    return [{k: point.get(k) for k in ("code", "value", "time")}
            for point in data["properties"] if isinstance(point, dict) and point.get("code") in ALLOWED]


def process(consumer, message, key, device_id, on_report, decrypt=decode):
    try:
        event = decrypt(message, key)
        points = properties(event, device_id)
    except (KeyError, ValueError, TypeError, AttributeError) as err:
        logging.warning("could not read event: %s", type(err).__name__)
        consumer.negative_acknowledge(message)
        return
    if points is not None:
        on_report(points)
        logging.info("fireplace detector event %s", json.dumps({
            "data_id": event["bizData"].get("dataId"), "ts": event.get("ts"), "properties": points
        }, ensure_ascii=False))
    # A persistence failure propagates before ACK so the broker can redeliver.
    consumer.acknowledge(message)


def consume(device_id, on_report):
    import pulsar

    credentials = json.loads((Path(os.environ["CREDENTIALS_DIRECTORY"]) / "tuya.json").read_text())
    client_id, secret = credentials["client_id"], credentials["client_secret"]
    secret_digest = hashlib.md5(secret.encode()).hexdigest()
    password_digest = hashlib.md5((client_id + secret_digest).encode()).hexdigest()
    # Tuya auth1 receives JSON through Pulsar Basic's username + ':' + password.
    auth = pulsar.AuthenticationBasic(
        '{"username": "' + client_id + '","password"',
        '"' + password_digest[8:24] + '"}', "auth1")
    key = secret[8:24].encode()
    client = pulsar.Client("pulsar+ssl://mqe.tuyacn.com:7285/", authentication=auth,
                           operation_timeout_seconds=15, tls_validate_hostname=True)
    try:
        consumer = client.subscribe(client_id + "/out/event", client_id + "-sub",
                                    consumer_type=pulsar.ConsumerType.Failover)
        logging.info("connected to Tuya production subscription")
        while True:
            process(consumer, consumer.receive(), key, device_id, on_report)
    finally:
        client.close()
