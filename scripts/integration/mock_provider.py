#!/usr/bin/env python3
"""Loopback-only fixture provider. Never proxies requests or logs keys/audio/text."""
import argparse
import json
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

TEST_KEY = "sk-whispering-local-fixture-only"
TRANSCRIPT = "This is the raw fixture transcript, um, for testing."
CLEANED = "This is the cleaned fixture transcript for testing."
MAX_BODY = 1024 * 1024


class FixtureServer(ThreadingHTTPServer):
    def __init__(self, port=0, delay=0):
        super().__init__(("127.0.0.1", port), Handler)
        self.events = []
        self.event_lock = threading.Lock()
        self.delay = delay


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def reply(self, status, body):
        encoded = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def do_GET(self):
        if self.path == "/health":
            self.reply(200, {"fixture": True})
        elif self.path == "/events":
            with self.server.event_lock:
                events = list(self.server.events)
            self.reply(200, {"events": events})
        else:
            self.reply(404, {"error": "Unknown fixture route"})

    def do_POST(self):
        if self.path not in ("/v1/audio/transcriptions", "/v1/chat/completions"):
            self.reply(404, {"error": "Unknown fixture route"})
            return
        if self.headers.get("Authorization") != "Bearer " + TEST_KEY:
            self.reply(401, {"error": "Use the local-only fixture key"})
            return
        try:
            size = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            size = 0
        if not 0 < size <= MAX_BODY:
            self.reply(413, {"error": "Fixture body must be 1 byte to 1 MiB"})
            return
        # Deliberately discard body; retain metadata only, never audio, prompts or keys.
        data = self.rfile.read(size)
        if len(data) != size:
            self.reply(400, {"error": "Incomplete fixture body"})
            return
        with self.server.event_lock:
            self.server.events.append({"route": self.path, "bytes": size})
        if self.server.delay:
            time.sleep(self.server.delay)
        if self.path.endswith("/transcriptions"):
            self.reply(200, {"text": TRANSCRIPT})
        else:
            self.reply(200, {
                "id": "fixture-completion", "object": "chat.completion",
                "created": 0, "model": "fixture-model",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": CLEANED},
                             "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
            })


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=18763)
    parser.add_argument("--delay", type=float, default=0)
    args = parser.parse_args()
    with FixtureServer(args.port, args.delay) as server:
        print("Fixture provider on 127.0.0.1:%d" % server.server_port, flush=True)
        server.serve_forever()
