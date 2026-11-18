#!/usr/bin/env python3
"""Exercise the LAN gateway with Nginx, a local TLS certificate and a mock API.

Requires Python 3.9+, nginx and openssl. No provider account is used. The test
starts only loopback listeners and removes its temporary keys/configs/logs.

    python3 tests/nginx_gateway.py --nginx /path/to/nginx --output result.json
"""

import argparse
import hashlib
import http.client
import http.server
import json
from pathlib import Path
import re
import shutil
import socket
import ssl
import subprocess
import tempfile
import threading
import time


PLACEHOLDER = "REPLACE_WITH_64_HEX_SECRET"
# Deliberately public fixture, never a deployment key. Contains letters so a
# case-insensitive comparison cannot pass the regression test accidentally.
TOKEN = "abcdef0123456789" * 4
QUERY_MARKER = "QUERY_MUST_NOT_APPEAR"
BODY_MARKER = "BODY_MUST_NOT_APPEAR"


def require(condition, message):
    if not condition:
        raise AssertionError(message)


class MockServer(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self):
        super().__init__(("127.0.0.1", 0), MockHandler)
        self.requests = []
        self.release_stream = threading.Event()


class MockHandler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        self.server.requests.append({
            "path": self.path,
            "authorization": self.headers.get("Authorization"),
            "x_api_key": self.headers.get("X-Api-Key"),
            "body_length": len(body),
        })
        if self.path.startswith("/v1/stream"):
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            self.wfile.write(b'event: ping\ndata: {"type":"ping"}\n\n')
            self.wfile.flush()
            self.server.release_stream.wait(timeout=5)
            self.wfile.write(b'event: message_stop\ndata: {"type":"message_stop"}\n\n')
            self.wfile.flush()
            return
        status = next((code for code in (401, 403)
                       if self.path.startswith(f"/v1/provider{code}")), 200)
        payload = upstream_payload(status)
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("request-id", f"provider-{status}-request")
        self.send_header("X-Upstream-Marker", "preserve-me")
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, *_):
        pass


def upstream_payload(status):
    if status == 200:
        return b'{"type":"message","id":"msg_mock","content":[]}'
    return json.dumps({
        "type": "error",
        "error": {"type": "upstream-test-error", "message": f"Keep provider {status}"},
        "request_id": f"provider-{status}-request",
    }, separators=(",", ":")).encode()


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def certificate(directory, openssl):
    config = directory / "certificate.conf"
    config.write_text(
        "[req]\nprompt=no\ndistinguished_name=dn\nx509_extensions=v3\n"
        "[dn]\nCN=127.0.0.1\n[v3]\nsubjectAltName=IP:127.0.0.1\n"
        "basicConstraints=critical,CA:TRUE\n"
        "keyUsage=critical,digitalSignature,keyEncipherment,keyCertSign\n"
        "extendedKeyUsage=serverAuth\n", encoding="utf-8")
    cert, key = directory / "tls.crt", directory / "tls.key"
    subprocess.run([
        openssl, "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-sha256",
        "-days", "1", "-config", str(config), "-keyout", str(key), "-out", str(cert),
    ], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return cert, key


def request(port, context, headers, path="/v1/messages"):
    connection = http.client.HTTPSConnection("127.0.0.1", port, context=context, timeout=5)
    payload = json.dumps({"prompt": BODY_MARKER}).encode()
    try:
        connection.putrequest("POST", path + "?audit=" + QUERY_MARKER)
        for name, value in headers:
            connection.putheader(name, value)
        connection.putheader("Content-Length", str(len(payload)))
        connection.endheaders(payload)
        response = connection.getresponse()
        body = response.read()
        return response.status, response.headers, body
    finally:
        connection.close()


def check_local_error(status, headers, body, expected):
    require(status == expected, f"Expected local {expected}, received {status}")
    require(headers.get_content_type() == "application/json", "Local error is not JSON")
    data = json.loads(body)
    kind = "authentication_error" if expected == 401 else "permission_error"
    require(data.get("type") == "error" and data.get("error", {}).get("type") == kind,
            f"Wrong Anthropic error type: {data}")
    request_id = headers.get("request-id", "")
    require(bool(re.fullmatch(r"[0-9a-f]{32}", request_id)), "Missing gateway request-id")
    require(data.get("request_id") == request_id, "Header/body request-id disagree")
    require(headers.get("Cache-Control") == "no-store", "Local error is cacheable")
    return request_id


def run_variant(source, nginx, openssl, directory, variant):
    directory.mkdir()
    (directory / "logs").mkdir()
    cert, key = certificate(directory, openssl)
    frontend = free_port()
    backend = MockServer()
    worker = threading.Thread(target=backend.serve_forever, daemon=True)
    worker.start()
    process = None
    try:
        require(PLACEHOLDER in source, "Expected secret placeholder was not found")
        config = source.replace("192.168.0.29:18766", f"127.0.0.1:{frontend}")
        config = config.replace("192.168.0.0/24", "192.0.2.0/24" if variant == "denied_subnet" else "127.0.0.0/8")
        config = config.replace("/path/to/lan-server.crt", cert.as_posix())
        config = config.replace("/path/to/lan-server.key", key.as_posix())
        config = config.replace("127.0.0.1:18765", f"127.0.0.1:{backend.server_port}")
        if variant != "unconfigured":
            config = config.replace(PLACEHOLDER, "" if variant == "empty_key" else TOKEN)
        # --config is a template override, not permission to launch a listener
        # or make proxy requests outside this self-contained loopback test.
        active = "\n".join(line.split("#", 1)[0] for line in config.splitlines())
        require(re.findall(r"\blisten\s+([^;{}]+);", active) == [f"127.0.0.1:{frontend} ssl"],
                "The test template must render exactly one loopback listener")
        require(re.findall(r"\bproxy_pass\s+([^;{}]+);", active) == [f"http://127.0.0.1:{backend.server_port}"],
                "The test template must proxy only to the loopback mock")
        require(not re.search(r"\binclude\s", active), "External config includes are not supported by this test")
        config = (
            "worker_processes 1;\npid nginx.pid;\nerror_log logs/error.log;\n"
            "events {}\nhttp {\nclient_body_temp_path body;\nproxy_temp_path proxy;\n"
            "fastcgi_temp_path fastcgi;\nuwsgi_temp_path uwsgi;\nscgi_temp_path scgi;\n"
            + config + "\n}\n"
        )
        conf = directory / "nginx.conf"
        conf.write_text(config, encoding="utf-8")
        command = [nginx, "-p", str(directory), "-c", str(conf)]
        checked = subprocess.run(command + ["-t"], capture_output=True, text=True)
        require(checked.returncode == 0, "nginx -t failed: " + checked.stderr)
        process = subprocess.Popen(command + ["-g", "daemon off; master_process off;"],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        deadline = time.monotonic() + 5
        while True:
            require(process.poll() is None, "Nginx exited during startup")
            try:
                with socket.create_connection(("127.0.0.1", frontend), timeout=0.1):
                    break
            except OSError:
                require(time.monotonic() < deadline, "Nginx did not start")
                time.sleep(0.02)
        # Trust this test certificate explicitly; do not disable TLS verification.
        context = ssl.create_default_context(cafile=str(cert))
        auth = [("Authorization", "Bearer " + TOKEN)]
        if variant == "configured":
            cases = [
                ("exact_64_hex_key", auth, "/v1/messages", 200, False),
                ("lowercase_scheme", [("Authorization", "bearer " + TOKEN)], "/v1/messages", 200, False),
                ("mixed_case_scheme", [("Authorization", "bEaReR " + TOKEN)], "/v1/messages", 200, False),
                ("changed_token_case", [("Authorization", "Bearer " + TOKEN.upper())], "/v1/messages", 401, True),
                ("different_64_hex_key", [("Authorization", "Bearer " + "1" * 64)], "/v1/messages", 401, True),
                ("missing_key", [], "/v1/messages", 401, True),
                ("x_api_key_only", [("X-Api-Key", TOKEN)], "/v1/messages", 401, True),
                ("wrong_scheme", [("Authorization", "Basic " + TOKEN)], "/v1/messages", 401, True),
                ("empty_token", [("Authorization", "Bearer")], "/v1/messages", 401, True),
                ("token_prefix", [("Authorization", "Bearer 0" + TOKEN)], "/v1/messages", 401, True),
                ("token_suffix", [("Authorization", "Bearer " + TOKEN + "0")], "/v1/messages", 401, True),
                ("appended_token", [("Authorization", "Bearer " + TOKEN + " extra")], "/v1/messages", 401, True),
                ("comma_separated_token", [("Authorization", "Bearer " + TOKEN + "," + TOKEN)], "/v1/messages", 401, True),
                ("regex_literal_suffix", [("Authorization", "Bearer " + TOKEN + ".*")], "/v1/messages", 401, True),
                ("error_content_type_with_suffix", [], "/v1/messages.html", 401, True),
                ("duplicate_authorization", auth + auth, "/v1/messages", 400, False),
                ("duplicate_authorization_with_wrong", auth + [("Authorization", "Bearer wrong")], "/v1/messages", 400, False),
                ("headers_stripped", auth + [("X-Api-Key", "must-be-stripped")], "/v1/messages", 200, False),
                ("provider_401_preserved", auth, "/v1/provider401", 401, False),
                ("provider_403_preserved", auth, "/v1/provider403", 403, False),
                ("monitor_hidden", auth, "/monitor", 404, False),
                ("health_hidden", auth, "/healthz", 404, False),
            ]
        elif variant == "denied_subnet":
            cases = [
                ("valid_key_outside_lan", auth, "/v1/messages", 403, True),
                ("missing_key_outside_lan", [], "/v1/messages", 401, True),
            ]
        else:
            cases = [
                ("placeholder_does_not_authenticate", [("Authorization", "Bearer " + PLACEHOLDER)], "/v1/messages", 401, True),
                ("hex_key_does_not_authenticate", auth, "/v1/messages", 401, True),
                ("empty_does_not_authenticate", [("Authorization", "Bearer")], "/v1/messages", 401, True),
            ]
        outcomes, local_error_ids = [], []
        for label, headers, path, expected, local in cases:
            before = len(backend.requests)
            status, response_headers, body = request(frontend, context, headers, path)
            require(status == expected, f"{variant}/{label}: expected {expected}, got {status}")
            if local:
                local_error_ids.append(check_local_error(status, response_headers, body, expected))
            forwarded = path.startswith("/v1/provider") or status == 200
            require(len(backend.requests) == before + int(forwarded),
                    f"{variant}/{label}: unexpected upstream access")
            if forwarded:
                require(backend.requests[-1]["authorization"] is None, "Bearer key reached upstream")
                require(backend.requests[-1]["x_api_key"] is None, "X-Api-Key reached upstream")
                require(body == upstream_payload(status), f"{label}: upstream body was rewritten")
                require(response_headers.get("request-id") == f"provider-{status}-request",
                        f"{label}: upstream request-id was rewritten")
                require(response_headers.get("X-Upstream-Marker") == "preserve-me",
                        f"{label}: upstream header was removed")
            outcomes.append({"case": label, "status": status,
                             "content_type": response_headers.get_content_type(),
                             "request_id": response_headers.get("request-id"),
                             "body_sha256": hashlib.sha256(body).hexdigest()})

        if variant == "configured":
            connection = http.client.HTTPSConnection("127.0.0.1", frontend, context=context, timeout=2)
            try:
                connection.request("POST", "/v1/stream", body=b"{}", headers=dict(auth))
                response = connection.getresponse()
                require(response.status == 200, "SSE request failed")
                require(response.readline() == b"event: ping\n", "SSE was buffered before completion")
                require(not backend.release_stream.is_set(), "SSE test released the stream too early")
                backend.release_stream.set()
                require(b"event: message_stop" in response.read(), "SSE termination was lost")
                outcomes.append({"case": "sse_delivered_before_completion", "status": 200})
            finally:
                backend.release_stream.set()
                connection.close()

        log_path = directory / "logs/ccp-auth-fail.log"
        deadline = time.monotonic() + 2
        while True:
            logged = log_path.read_text(encoding="utf-8") if log_path.exists() else ""
            if len(logged.splitlines()) >= len(local_error_ids) or time.monotonic() >= deadline:
                break
            time.sleep(0.02)
        logged_ids = re.findall(r"request_id=([0-9a-f]{32})", logged)
        require(sorted(logged_ids) == sorted(local_error_ids),
                f"{variant}: local failures missing or upstream failures included in auth log")
        require(len(logged.splitlines()) == len(local_error_ids), "Unexpected auth log entries")
        for marker in (TOKEN, TOKEN.upper(), QUERY_MARKER, BODY_MARKER, "must-be-stripped"):
            require(marker not in logged, f"{variant}: sensitive fixture leaked into auth log")
        return {"variant": variant, "config_test": "passed", "cases": outcomes,
                "local_auth_failures_logged": len(logged_ids),
                "only_local_auth_failures_logged": True,
                "credentials_removed_upstream": True,
                "secrets_query_body_absent_from_auth_log": True}
    finally:
        backend.release_stream.set()
        if process is not None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
            if process.stderr is not None:
                process.stderr.close()
        backend.shutdown()
        backend.server_close()
        worker.join(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nginx", default=shutil.which("nginx") or "nginx")
    parser.add_argument("--openssl", default=shutil.which("openssl") or "openssl")
    parser.add_argument("--config", type=Path,
                        default=Path(__file__).resolve().parents[1] / "deploy/nginx/claude-code-proxy-lan.conf")
    parser.add_argument("--output", type=Path, help="Write a JSON result containing no keys or certificates")
    args = parser.parse_args()
    nginx = shutil.which(args.nginx)
    openssl = shutil.which(args.openssl)
    require(nginx is not None and openssl is not None, "nginx and openssl executables are required")
    source = args.config.read_text(encoding="utf-8")
    with tempfile.TemporaryDirectory(prefix="ccp-nginx-test-") as scratch:
        results = [run_variant(source, nginx, openssl, Path(scratch) / name, name)
                   for name in ("configured", "unconfigured", "empty_key", "denied_subnet")]
    result = {"result": "passed", "case_count": sum(len(item["cases"]) for item in results),
              "temporary_files_removed": True, "variants": results}
    text = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text, encoding="utf-8")
    print(text, end="")


if __name__ == "__main__":
    main()
