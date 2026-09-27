#!/usr/bin/env python3
"""Logging forward proxy: CLIENT_PORT -> 127.0.0.1:REAL_PORT.

Appends every request body to OUT_FILE (one body per file rewrite; the
session POST offer is what matters). Forwards bytes identically.
"""
import socket
import sys
import threading

CLIENT_PORT = int(sys.argv[1])
REAL_PORT = int(sys.argv[2])
OUT_FILE = sys.argv[3]


def read_http_message(stream):
    stream.settimeout(10)
    head = b""
    try:
        while not head.endswith(b"\r\n\r\n"):
            chunk = stream.recv(1)
            if not chunk:
                return None, None
            head += chunk
            if len(head) > 65536:
                return None, None
    except (socket.timeout, OSError):
        return None, None
    length = None
    for line in head.decode("latin1").split("\r\n"):
        if ":" in line:
            k, v = line.split(":", 1)
            if k.strip().lower() == "content-length":
                try:
                    length = int(v.strip())
                except ValueError:
                    length = 0
    body = b""
    try:
        if length is not None:
            while len(body) < length:
                chunk = stream.recv(length - len(body))
                if not chunk:
                    break
                body += chunk
        else:
            while True:
                try:
                    chunk = stream.recv(4096)
                except socket.timeout:
                    break
                if not chunk:
                    break
                body += chunk
    except OSError:
        pass
    return head, body


def handle(client):
    try:
        head, body = read_http_message(client)
        if head is None:
            client.close()
            return
        if head.startswith(b"POST") and body:
            with open(OUT_FILE, "wb") as f:
                f.write(body)
        server = socket.create_connection(("127.0.0.1", REAL_PORT), timeout=10)
        server.sendall(head + body)
        rhead, rbody = read_http_message(server)
        if rhead is not None:
            client.sendall(rhead + rbody)
        server.close()
    except OSError:
        pass
    finally:
        client.close()


listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
listener.bind(("127.0.0.1", CLIENT_PORT))
listener.listen(16)
while True:
    conn, _ = listener.accept()
    threading.Thread(target=handle, args=(conn,), daemon=True).start()
