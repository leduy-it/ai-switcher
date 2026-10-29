#!/usr/bin/env python3
"""Exercise real Codex subprocesses with synthetic OAuth and a loopback-only provider.

No real profile, credential, desktop app, or existing process is modified. No inference request
uses a real token. Run with --codex /absolute/path/to/the/installed/codex/binary.
"""
import argparse
import base64
import datetime
import hashlib
import http.server
import json
import os
import pathlib
import selectors
import subprocess
import tempfile
import threading
import time


def jwt(label):
    claims = {"email": "fixture-" + label + "@example.invalid", "exp": 4102444800,
              "https://api.openai.com/auth": {"chatgpt_account_id": "00000000-0000-4000-8000-00000000000" + label,
                                             "chatgpt_user_id": "fixture-user-" + label,
                                             "chatgpt_plan_type": "plus"},
              "https://api.openai.com/profile": {"email": "fixture-" + label + "@example.invalid"}}
    enc = lambda value: base64.urlsafe_b64encode(json.dumps(value).encode()).rstrip(b"=").decode()
    return enc({"alg": "none", "typ": "JWT"}) + "." + enc(claims) + ".synthetic"


class Provider(http.server.BaseHTTPRequestHandler):
    calls = []
    called = threading.Event()
    release = threading.Event()
    tokens = {jwt("a"): "a", jwt("b"): "b"}

    def log_message(self, *_):
        pass

    def do_GET(self):
        body = {"models": [], "data": []}
        if self.path.endswith("/accounts/check"):
            label = self.tokens.get(self.headers.get("Authorization", "").removeprefix("Bearer "), "a")
            account_id = "00000000-0000-4000-8000-00000000000" + label
            body = {"accounts": [{"id": account_id, "workspace_backend_origin": "https://chatgpt.com",
                                  "account_routing_override": "NO_CONSTRAINT"}],
                    "default_account_id": account_id}
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(body).encode())

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))))
        if not self.path.rstrip("/").endswith("/responses"):
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b"{}")
            return
        token = self.headers.get("Authorization", "").removeprefix("Bearer ")
        label = self.tokens.get(token)
        if label is None:
            self.send_error(401, "Only synthetic fixture tokens are accepted")
            return
        self.calls.append({"credential": label, "path": self.path})
        self.called.set()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Connection", "close")
        self.end_headers()
        response_id = "fixture-response-" + str(len(self.calls))
        item = {"id": "fixture-message", "type": "message", "role": "assistant", "status": "completed",
                "content": [{"type": "output_text", "text": "Fixture credential " + label, "annotations": []}]}
        def emit(kind, **values):
            self.wfile.write(("event: " + kind + "\ndata: " + json.dumps({"type": kind, **values}) + "\n\n").encode())
            self.wfile.flush()
        try:
            emit("response.created", response={"id": response_id, "status": "in_progress", "output": []})
            if len(self.calls) == 1:
                self.release.wait(20)
            emit("response.output_item.added", output_index=0, item={**item, "status": "in_progress", "content": []})
            emit("response.output_text.delta", item_id=item["id"], output_index=0, content_index=0,
                 delta=item["content"][0]["text"])
            emit("response.output_item.done", output_index=0, item=item)
            emit("response.completed", response={"id": response_id, "status": "completed", "output": [item],
                 "usage": {"input_tokens": 10, "output_tokens": 5, "total_tokens": 15}})
        except (BrokenPipeError, ConnectionResetError):
            pass


class Backend:
    def __init__(self, binary, home, shared, scratch, log):
        env = {k: v for k, v in os.environ.items()
               if k not in {"OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN", "ACCESS_TOKEN", "CODEX_REMOTE_TOKEN"}}
        env.update(CODEX_HOME=str(home), CODEX_SQLITE_HOME=str(shared))
        self.log = open(log, "wb")
        self.process = subprocess.Popen([binary, "app-server", "--listen", "stdio://"], env=env,
                                        cwd=scratch, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.log, bufsize=0)
        self.pending = b""
        self.events = []
        self.sequence = 0
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        try:
            self.request("initialize", {"clientInfo": {"name": "codex_desktop", "title": "Codex Desktop fixture",
                         "version": "0.1.0"}, "capabilities": {"experimentalApi": True}})
            self.send({"method": "initialized", "params": {}})
        except BaseException:
            self.close()
            raise

    def send(self, message):
        self.process.stdin.write((json.dumps(message) + "\n").encode())

    def receive(self, deadline):
        while b"\n" not in self.pending:
            ready = self.selector.select(max(0, deadline - time.monotonic()))
            if not ready:
                raise TimeoutError("Isolated backend timed out; inspect its fixture log")
            data = os.read(self.process.stdout.fileno(), 65536)
            if not data:
                raise RuntimeError("Isolated backend exited; inspect its fixture log")
            self.pending += data
        line, self.pending = self.pending.split(b"\n", 1)
        return json.loads(line)

    def request(self, method, params):
        self.sequence += 1
        sequence = self.sequence
        self.send({"id": sequence, "method": method, "params": params})
        deadline = time.monotonic() + 20
        while True:
            message = self.receive(deadline)
            if message.get("id") == sequence and "method" not in message:
                if "error" in message:
                    raise RuntimeError(method + ": " + json.dumps(message["error"]))
                return message["result"]
            if "id" in message and "method" in message:
                self.send({"id": message["id"], "error": {"code": -32601, "message": "Fixture does not execute tools"}})
            else:
                self.events.append(message)

    def wait_turn(self, turn_id):
        deadline = time.monotonic() + 20
        while True:
            for index, message in enumerate(self.events):
                if message.get("method") == "turn/completed" and message["params"]["turn"]["id"] == turn_id:
                    return self.events.pop(index)["params"]["turn"]
            self.events.append(self.receive(deadline))

    def close(self):
        self.process.terminate()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.selector.close()
        self.log.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--codex", required=True)
    args = parser.parse_args()
    binary = str(pathlib.Path(args.codex).resolve(strict=True))
    root = pathlib.Path(tempfile.mkdtemp(prefix="michael-profiles-fixture-"))
    os.chmod(root, 0o700)
    print("Isolated fixture directory: " + str(root), flush=True)
    shared, scratch = root / "shared", root / "scratch"
    shared.mkdir(); scratch.mkdir()
    for name in ["sessions", "archived_sessions"]:
        (shared / name).mkdir()
    provider = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    worker = threading.Thread(target=provider.serve_forever, daemon=True)
    worker.start()
    homes = {}
    for label in ["a", "b"]:
        home = root / ("profile-" + label); home.mkdir()
        homes[label] = home
        for name in ["sessions", "archived_sessions"]:
            (home / name).symlink_to(shared / name, target_is_directory=True)
        auth = {"auth_mode": "chatgpt", "tokens": {"access_token": jwt(label), "id_token": jwt(label),
                "refresh_token": "synthetic-never-refresh", "account_id": "00000000-0000-4000-8000-00000000000" + label},
                "last_refresh": datetime.datetime.now(datetime.timezone.utc).isoformat()}
        (home / "auth.json").write_text(json.dumps(auth)); os.chmod(home / "auth.json", 0o600)
        (home / "config.toml").write_text('model = "fixture-model"\nmodel_provider = "fixture"\ncli_auth_credentials_store = "file"\nchatgpt_base_url = "http://127.0.0.1:' + str(provider.server_port) + '"\napproval_policy = "never"\nsandbox_mode = "danger-full-access"\n[model_providers.fixture]\nname = "Synthetic local provider"\nbase_url = "http://127.0.0.1:' + str(provider.server_port) + '/v1"\nwire_api = "responses"\nrequires_openai_auth = true\nsupports_websockets = false\n')
    before = {k: hashlib.sha256((v / "auth.json").read_bytes()).hexdigest() for k, v in homes.items()}
    backend = None
    try:
        backend = Backend(binary, homes["a"], shared, scratch, root / "backend-a.log")
        identity = backend.request("account/read", {"refreshToken": False})
        assert identity["account"]["email"] == "fixture-a@example.invalid"
        assert identity["workspaceRouting"]["chatgptAccountId"] == "00000000-0000-4000-8000-00000000000a"
        assert backend.request("getAuthStatus", {"includeToken": True, "refreshToken": False})["authToken"] == jwt("a")
        thread = backend.request("thread/start", {"cwd": str(scratch), "model": "fixture-model", "modelProvider": "fixture",
                                                  "approvalPolicy": "never", "sandbox": "danger-full-access"})["thread"]
        thread_id = thread["id"]
        turn = backend.request("turn/start", {"threadId": thread_id, "input": [{"type": "text", "text": "scenario-active"}]})["turn"]
        assert Provider.called.wait(15), "Local fixture received no model request"
        assert Provider.calls[-1]["credential"] == "a"
        backend.request("turn/interrupt", {"threadId": thread_id, "turnId": turn["id"]})
        assert backend.wait_turn(turn["id"])["status"] == "interrupted"
        Provider.release.set()
        backend.close(); backend = None
        backend = Backend(binary, homes["b"], shared, scratch, root / "backend-b.log")
        identity = backend.request("account/read", {"refreshToken": False})
        assert identity["account"]["email"] == "fixture-b@example.invalid"
        assert identity["workspaceRouting"]["chatgptAccountId"] == "00000000-0000-4000-8000-00000000000b"
        assert backend.request("getAuthStatus", {"includeToken": True, "refreshToken": False})["authToken"] == jwt("b")
        resumed = backend.request("thread/resume", {"threadId": thread_id, "excludeTurns": True})
        assert resumed["thread"]["id"] == thread_id
        continued = backend.request("turn/start", {"threadId": thread_id,
                       "input": [{"type": "text", "text": "scenario-continue-once"}]})["turn"]
        assert backend.wait_turn(continued["id"])["status"] == "completed"
        assert Provider.calls[-1]["credential"] == "b"
        history = backend.request("thread/read", {"threadId": thread_id, "includeTurns": True})["thread"]
        assert len([t for t in history["turns"] if t["id"] == continued["id"]]) == 1
        # Simulate recovery retry: acknowledge the persisted marker instead of sending a turn again.
        request_count = len(Provider.calls)
        retry_history = backend.request("thread/read", {"threadId": thread_id, "includeTurns": True})["thread"]
        assert "scenario-continue-once" in json.dumps(retry_history)
        assert len(Provider.calls) == request_count
        for key, home in homes.items():
            assert hashlib.sha256((home / "auth.json").read_bytes()).hexdigest() == before[key], "Fixture auth was unexpectedly refreshed"
        report = {"result": "passed", "testRoot": str(root), "threadId": thread_id,
              "scenarios": ["synthetic OAuth identity A", "interrupt active fixture turn", "restart isolated backend with identity B",
                            "resume original thread", "continued inference uses credential B", "recovery retry reads persisted acknowledgement", "credential files unchanged", "native credential cache and workspace match selected auth"]}
        (root / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report, indent=2))
    finally:
        Provider.release.set()
        if backend is not None:
            backend.close()
        provider.shutdown()


if __name__ == "__main__":
    main()
