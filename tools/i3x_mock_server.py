#!/usr/bin/env python3
"""
Mock i3X server for testing the Flux i3X connector.

Implements the minimal i3X API the connector needs:
  GET  /objects
  POST /subscriptions
  POST /subscriptions/{id}/register
  GET  /subscriptions/{id}/stream  (SSE)

Run:
  pip install flask
  python3 tools/i3x_mock_server.py

Then add an i3X source in the Flux UI:
  Base URL:  http://192.168.50.13:5100
  API Key:   test-key (anything works)
"""

import json
import time
import math
import random
import threading
import uuid
from flask import Flask, Response, request, jsonify

app = Flask(__name__)

# Simulated objects — mix of scalar and object values
OBJECTS = [
    {"elementId": "plant.conveyor.speed"},
    {"elementId": "plant.conveyor.status"},
    {"elementId": "plant.boiler.temperature"},
    {"elementId": "plant.boiler.pressure"},
    {"elementId": "plant.pump.flow-rate"},
    {"elementId": "plant.ambient.sensor"},
]

# Active subscriptions: id -> set of registered elementIds
subscriptions: dict[str, set] = {}
subscriptions_lock = threading.Lock()


@app.route("/objects", methods=["GET"])
def get_objects():
    return jsonify(OBJECTS)


@app.route("/subscriptions", methods=["POST"])
def create_subscription():
    sub_id = str(uuid.uuid4())
    with subscriptions_lock:
        subscriptions[sub_id] = set()
    print(f"[i3x] Subscription created: {sub_id}")
    return jsonify({"id": sub_id, "message": "Subscription created"})


@app.route("/subscriptions/<sub_id>/register", methods=["POST"])
def register_objects(sub_id):
    with subscriptions_lock:
        if sub_id not in subscriptions:
            return jsonify({"error": "Subscription not found"}), 404
        body = request.get_json(force=True) or {}
        element_ids = body.get("elementIds", [])
        subscriptions[sub_id].update(element_ids)
    print(f"[i3x] Registered {len(element_ids)} objects to {sub_id}: {element_ids}")
    return jsonify({"registered": len(element_ids)})


@app.route("/subscriptions/<sub_id>/stream", methods=["GET"])
def stream(sub_id):
    with subscriptions_lock:
        if sub_id not in subscriptions:
            return jsonify({"error": "Subscription not found"}), 404

    def generate():
        t = 0
        print(f"[i3x] SSE stream opened for {sub_id}")
        try:
            while True:
                now = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())

                events = [
                    {
                        "elementId": "plant.conveyor.speed",
                        "value": round(50 + 10 * math.sin(t / 5), 2),
                        "quality": "Good",
                        "timestamp": now,
                    },
                    {
                        "elementId": "plant.conveyor.status",
                        "value": "running" if math.sin(t / 20) > -0.5 else "stopped",
                        "quality": "Good",
                        "timestamp": now,
                    },
                    {
                        "elementId": "plant.boiler.temperature",
                        "value": round(180 + 15 * math.cos(t / 8), 2),
                        "quality": "Good",
                        "timestamp": now,
                    },
                    {
                        "elementId": "plant.boiler.pressure",
                        "value": round(14.7 + random.uniform(-0.5, 0.5), 2),
                        "quality": "Good",
                        "timestamp": now,
                    },
                    {
                        "elementId": "plant.pump.flow-rate",
                        "value": round(120 + 20 * math.sin(t / 3 + 1), 2),
                        "quality": "Good" if random.random() > 0.05 else "Bad",
                        "timestamp": now,
                    },
                    {
                        "elementId": "plant.ambient.sensor",
                        "value": {
                            "temperature": round(72 + random.uniform(-2, 2), 1),
                            "humidity": round(45 + random.uniform(-5, 5), 1),
                            "co2_ppm": round(400 + random.uniform(-20, 20)),
                        },
                        "quality": "Good",
                        "timestamp": now,
                    },
                ]

                for event in events:
                    yield f"data: {json.dumps(event)}\n\n"

                t += 1
                time.sleep(2)
        except GeneratorExit:
            print(f"[i3x] SSE stream closed for {sub_id}")

    return Response(generate(), mimetype="text/event-stream")


if __name__ == "__main__":
    print("Mock i3X server starting on http://0.0.0.0:5100")
    print("Objects:", [o["elementId"] for o in OBJECTS])
    app.run(host="0.0.0.0", port=5100, threaded=True)
