#!/usr/bin/env python3
"""Opt-in native GUI fixture. Creates synthetic accounts, isolated desktop data and a loopback
provider. Keeps existing desktop processes and real credentials untouched. Native logs and fake
credentials stay in a private temporary folder; successful output contains metadata only.
"""
import sys, os, json, time, subprocess, tempfile, pathlib, http.server, threading, hashlib, datetime, signal
import argparse
repo = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(repo / 'tests'))
from desktop_switch_scenarios import Backend, Provider, jwt
import socket, struct, base64

class Control(Backend):

    def __init__(self, home):
        self.socket = socket.socket(socket.AF_UNIX)
        self.socket.settimeout(10)
        self.socket.connect(json.loads((home / '.michael-desktop-runtime-v1.json').read_text())['controlSocket'])
        key = base64.b64encode(os.urandom(16)).decode()
        self.socket.sendall(('GET /rpc HTTP/1.1\r\nHost: codex-app-server\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: ' + key + '\r\nSec-WebSocket-Version: 13\r\n\r\n').encode())
        header = b''
        while not header.endswith(b'\r\n\r\n'):
            header += self.socket.recv(1)
            if len(header) > 8192:
                raise RuntimeError('Invalid local websocket header')
        assert header.startswith(b'HTTP/1.1 101')
        self.events = []
        self.sequence = 0
        self.request('initialize', {'clientInfo': {'name': 'profiles_fixture_control', 'version': '0.16.0'}, 'capabilities': {'experimentalApi': True}})
        self.send({'method': 'initialized', 'params': {}})

    def frame(self, body, opcode=1):
        mask = os.urandom(4)
        n = len(body)
        size = bytes([n | 128]) if n < 126 else bytes([126 | 128]) + struct.pack('!H', n) if n < 65536 else bytes([127 | 128]) + struct.pack('!Q', n)
        self.socket.sendall(bytes([128 | opcode]) + size + mask + bytes((v ^ mask[i % 4] for (i, v) in enumerate(body))))

    def send(self, message):
        self.frame(json.dumps(message).encode())

    def exact(self, n):
        out = b''
        while len(out) < n:
            data = self.socket.recv(n - len(out))
            if not data:
                raise RuntimeError('Local websocket closed')
            out += data
        return out

    def receive(self, deadline):
        payload = b''
        while True:
            self.socket.settimeout(max(0.01, deadline - time.monotonic()))
            (flags, size) = self.exact(2)
            opcode = flags & 15
            n = size & 127
            if n == 126:
                n = struct.unpack('!H', self.exact(2))[0]
            elif n == 127:
                n = struct.unpack('!Q', self.exact(8))[0]
            if n > 16 * 1024 * 1024:
                raise RuntimeError('Oversized local websocket response')
            mask = self.exact(4) if size & 128 else None
            body = self.exact(n)
            if mask:
                body = bytes((v ^ mask[i % 4] for (i, v) in enumerate(body)))
            if opcode == 9:
                self.frame(body, 10)
                continue
            if opcode == 8:
                raise RuntimeError('Local websocket closed')
            if opcode not in [0, 1]:
                continue
            payload += body
            if flags & 128:
                return json.loads(payload)

    def close(self):
        self.socket.close()
parser = argparse.ArgumentParser(description='Opt-in isolated native desktop continuation using synthetic OAuth and loopback inference only')
parser.add_argument('--codex', required=True)
parser.add_argument('--app', required=True, help='Native desktop executable, not bundle')
parser.add_argument('--profiles-binary', required=True, help='Built Profiles executable containing the desktop-only transport adapter')
options = parser.parse_args()
binary = str(pathlib.Path(options.codex).resolve())
app = str(pathlib.Path(options.app).resolve())
profiles = str(pathlib.Path(options.profiles_binary).resolve())
root = pathlib.Path(tempfile.mkdtemp(prefix='profiles-native-desktop-fixture-'))
root.chmod(448)
shared = root / 'shared'
scratch = root / 'scratch'
userdata = root / 'desktop-data'
for p in [shared, scratch, userdata]:
    p.mkdir(mode=448)
for name in ['sessions', 'archived_sessions']:
    (shared / name).mkdir()
server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Provider)
threading.Thread(target=server.serve_forever, daemon=True).start()
homes = {}
for label in ['a', 'b']:
    h = root / ('profile-' + label)
    h.mkdir(mode=448)
    homes[label] = h
    for name in ['sessions', 'archived_sessions']:
        (h / name).symlink_to(shared / name, target_is_directory=True)
    (h / 'auth.json').write_text(json.dumps({'auth_mode': 'chatgpt', 'tokens': {'access_token': jwt(label), 'id_token': jwt(label), 'refresh_token': 'synthetic-never-refresh', 'account_id': '00000000-0000-4000-8000-00000000000' + label}, 'last_refresh': datetime.datetime.now(datetime.timezone.utc).isoformat()}))
    (h / 'auth.json').chmod(384)
    (h / 'config.toml').write_text('model="fixture-model"\nmodel_provider="fixture"\ncli_auth_credentials_store="file"\nchatgpt_base_url="http://127.0.0.1:' + str(server.server_port) + '"\napproval_policy="never"\nsandbox_mode="danger-full-access"\n[analytics]\nenabled=false\n[features]\ncode_mode_host=true\n[model_providers.fixture]\nname="Synthetic local provider"\nbase_url="http://127.0.0.1:' + str(server.server_port) + '/v1"\nwire_api="responses"\nrequires_openai_auth=true\nsupports_websockets=false\n')
short = pathlib.Path('/private/tmp/mp-ui-' + os.urandom(4).hex())
short.mkdir(mode=448)
alias = short / 'b'
alias.symlink_to(homes['b'], target_is_directory=True)
env = {k: v for (k, v) in os.environ.items() if not k.startswith(('CODEX_APP_', 'CODEX_ELECTRON_')) and k not in {'OPENAI_API_KEY', 'CODEX_API_KEY', 'CODEX_ACCESS_TOKEN', 'ACCESS_TOKEN', 'CODEX_REMOTE_TOKEN', 'CODEX_CLI_PATH'}}
import shlex
wrapper = root / 'desktop-cli'
wrapper.write_text('#!/bin/sh\nexport CODEX_HOME='+shlex.quote(str(homes['b'].resolve()))+'\nexport CODEX_SQLITE_HOME='+shlex.quote(str(shared.resolve()))+'\nexec '+shlex.quote(profiles)+' --desktop-runtime-proxy '+shlex.quote(binary)+' "$@"\n')
wrapper.chmod(448)
env.update(CODEX_CLI_PATH=str(wrapper), CODEX_HOME=str(homes['b']), CODEX_SQLITE_HOME=str(shared), CODEX_ELECTRON_USER_DATA_PATH=str(userdata), CODEX_APP_SERVER_USE_LOCAL_DAEMON='0', CODEX_APP_SERVER_FORCE_CLI='0', CODEX_REFRESH_TOKEN_URL_OVERRIDE='http://127.0.0.1:' + str(server.server_port) + '/oauth/token')
backend = None
desktop = None
log = None
daemon = False
report = {'root': str(root), 'scenarios': []}
try:
    backend = Backend(binary, homes['a'], shared, scratch, root / 'backend-a.log')
    thread = backend.request('thread/start', {'cwd': str(scratch), 'model': 'fixture-model', 'modelProvider': 'fixture', 'approvalPolicy': 'never', 'sandbox': 'danger-full-access'})['thread']
    thread_id = thread['id']
    turn = backend.request('turn/start', {'threadId': thread_id, 'input': [{'type': 'text', 'text': 'Reply with the fixture credential label only.'}]})['turn']
    assert Provider.called.wait(15)
    backend.request('turn/interrupt', {'threadId': thread_id, 'turnId': turn['id']})
    assert backend.wait_turn(turn['id'])['status'] == 'interrupted'
    Provider.release.set()
    backend.close()
    backend = None
    report['scenarios'].append('original synthetic turn interrupted and durable')
    state = {'electron-persisted-atom-state': {'update-resume-state-v1': {'accountId': '00000000-0000-4000-8000-00000000000b', 'userId': 'fixture-user-b', 'threadIds': [thread_id], 'expiresAtMs': int(time.time() * 1000) + 900000}}}
    statepath = homes['b'] / '.codex-global-state.json'
    statepath.write_text(json.dumps(state))
    statepath.chmod(384)
    log = open(root / 'desktop.log', 'wb')
    os.chmod(root / 'desktop.log', 384)
    desktop = subprocess.Popen([app, '--user-data-dir=' + str(userdata), '--proxy-server=http://127.0.0.1:9', '--proxy-bypass-list=localhost;127.0.0.1;[::1]'], env=env, cwd=scratch, stdout=log, stderr=log)
    deadline = time.monotonic() + 30
    while not (homes['b'] / '.michael-desktop-runtime-v1.json').exists():
        if desktop.poll() is not None or time.monotonic() > deadline:
            raise RuntimeError('Native proxy record missing')
        time.sleep(0.1)
    backend = Control(homes['b'])
    assert backend.request('account/read', {'refreshToken': False})['account']['email'] == 'fixture-b@example.invalid'
    print(json.dumps({'fixtureRoot': str(root), 'fixtureDesktopPid': desktop.pid}), flush=True)
    deadline = time.monotonic() + 50
    consumed = False
    new_turns = []
    while time.monotonic() < deadline:
        if desktop.poll() is not None:
            break
        try:
            live = json.loads(statepath.read_text())
            atoms = live.get('electron-persisted-atom-state', {})
            consumed = 'update-resume-state-v1' in atoms and atoms['update-resume-state-v1'] is None
        except (OSError, ValueError):
            pass
        history = backend.request('thread/read', {'threadId': thread_id, 'includeTurns': True})['thread']
        new_turns = [t for t in history['turns'] if t['id'] != turn['id']]
        if consumed and len(new_turns) == 1 and (new_turns[0]['status'] == 'completed'):
            break
        time.sleep(0.5)
    report.update(markerConsumed=consumed, providerCredentialCalls=[c['credential'] for c in Provider.calls], fixtureDesktopExit=desktop.poll(), sameThreadId=history['id'] == thread_id, nativeNewTurnCount=len(new_turns), nativeTurnCompleted=bool(new_turns and new_turns[0]['status'] == 'completed'), modelPreserved=history.get('model') == 'fixture-model', cwdPreserved=pathlib.Path(history['cwd']).resolve() == scratch.resolve())
    report['result'] = 'passed' if consumed and [c['credential'] for c in Provider.calls] == ['a', 'b'] and (len(new_turns) == 1) and (new_turns[0]['status'] == 'completed') and (history['id'] == thread_id) and (history.get('model') == 'fixture-model') and (pathlib.Path(history['cwd']).resolve() == scratch.resolve()) else 'not_confirmed'
    assert report['result'] == 'passed', 'Native UI did not continue exactly once on the same controlled backend'
    record = json.loads((homes['b'] / '.michael-desktop-runtime-v1.json').read_text())
    assert record['frontendPid'] == desktop.pid
    assert pathlib.Path(record['sharedHome']).resolve()==shared.resolve(), 'Native shell environment overrode the fixture catalog'
    subprocess.run(['cargo', 'test', '--locked', '--manifest-path', str(repo / 'src-tauri/Cargo.toml'), '--lib', 'native_proxy_fixture_ownership', '--', '--ignored', '--nocapture'], env={**os.environ, 'AISW_DESKTOP_FIXTURE_HOME': str(homes['b'])}, check=True, timeout=120)
    report['scenarios'].append('Rust ownership discovery and cached credential/workspace verification on the native GUI backend')
    (root / 'report.json').write_text(json.dumps(report, indent=2))
    (root / 'report.json').chmod(384)
    print(json.dumps(report, indent=2), flush=True)
finally:
    Provider.release.set()
    if backend:
        backend.close()
    if desktop and desktop.poll() is None:
        desktop.terminate()
        try:
            desktop.wait(timeout=10)
        except subprocess.TimeoutExpired:
            desktop.kill()
            desktop.wait()
    if desktop:
        deadline = time.monotonic() + 12
        while (homes['b'] / '.michael-desktop-runtime-v1.json').exists() and time.monotonic() < deadline:
            time.sleep(0.1)
        assert not (homes['b'] / '.michael-desktop-runtime-v1.json').exists(), 'Desktop adapter did not remove its ownership record on GUI exit'
        if 'record' in locals():
            try:
                os.kill(record['backendPid'], 0)
            except ProcessLookupError:
                pass
            else:
                raise AssertionError('Native child survived its fixture GUI')
    if log:
        log.close()
    if daemon:
        subprocess.run([binary, 'app-server', 'daemon', 'stop'], env=env, capture_output=True, timeout=15)
    server.shutdown()
