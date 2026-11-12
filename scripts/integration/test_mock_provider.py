import json
import threading
import unittest
import urllib.error
import urllib.request
from mock_provider import CLEANED, TEST_KEY, TRANSCRIPT, FixtureServer


class FixtureProviderTests(unittest.TestCase):
    def setUp(self):
        self.server = FixtureServer()
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.base = "http://127.0.0.1:%d" % self.server.server_port

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def request(self, route, body=None, key=TEST_KEY):
        req = urllib.request.Request(self.base + route, data=body,
                                     headers={"Authorization": "Bearer " + key})
        try:
            response = urllib.request.urlopen(req, timeout=3)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            return response.status, json.load(response)

    def test_loopback_only_and_health(self):
        self.assertEqual(self.server.server_address[0], "127.0.0.1")
        self.assertEqual(self.request("/health"), (200, {"fixture": True}))

    def test_transcription_response_and_metadata_only(self):
        self.assertEqual(self.request("/v1/audio/transcriptions", b"synthetic audio"),
                         (200, {"text": TRANSCRIPT}))
        self.assertEqual(self.request("/events")[1], {"events": [
            {"route": "/v1/audio/transcriptions", "bytes": 15}]})

    def test_cleanup_response(self):
        status, body = self.request("/v1/chat/completions", b"{}")
        self.assertEqual(status, 200)
        self.assertEqual(body["choices"][0]["message"]["content"], CLEANED)

    def test_real_or_wrong_keys_are_rejected_without_logging(self):
        self.assertEqual(self.request("/v1/audio/transcriptions", b"x", "not-the-fixture-key")[0], 401)
        self.assertEqual(self.request("/events")[1], {"events": []})

    def test_unknown_routes_are_not_proxied(self):
        self.assertEqual(self.request("/unknown")[0], 404)
        self.assertEqual(self.request("/v1/unknown", b"{}")[0], 404)
        self.assertEqual(self.request("/events")[1], {"events": []})


if __name__ == "__main__":
    unittest.main()
