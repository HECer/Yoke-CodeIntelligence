"""Owned loopback queue observations; never native HTTP acceptance."""
import hashlib
import json
from pathlib import Path
import shutil
import socket
import time


def trial(size):
    with socket.socket() as listener, socket.socket() as client:
        listener.bind(('127.0.0.1', 0))
        listener.listen(1)
        client.connect(listener.getsockname())
        server, _ = listener.accept()
        with server:
            client.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 8192)
            server.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 8192)
            server.setblocking(False)
            filler = b'x' * 65536
            queued = 0
            blocked = False
            while queued < 64 * 1024 * 1024:
                try:
                    count = server.send(filler)
                    if count == 0:
                        break
                    queued += count
                except BlockingIOError:
                    blocked = True
                    break
            if not blocked:
                raise RuntimeError('Actual initial backpressure not observed')
            payload = memoryview(b'x' * size)
            sent = 0
            started = time.monotonic()
            end = started + .250
            outcome = None
            try:
                while sent < size:
                    remaining = end - time.monotonic()
                    if remaining <= 0:
                        outcome = 'deadline'
                        break
                    server.settimeout(remaining)
                    count = server.send(payload[sent:])
                    if count <= 0:
                        raise RuntimeError('Owned socket closed unexpectedly')
                    sent += count
                else:
                    outcome = 'complete'
            except TimeoutError:
                outcome = 'deadline'
            elapsed = time.monotonic() - started
            if elapsed >= 3:
                raise RuntimeError('Owned socket operation exceeded observation bound')
            return {'payloadBytes': size, 'initialWouldBlockObserved': blocked,
                    'queuedBytes': queued, 'clientReceiveBufferBytes': client.getsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF),
                    'serverSendBufferBytes': server.getsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF),
                    'responseBytesWritten': sent, 'outcome': outcome, 'elapsedSeconds': elapsed,
                    'nativeAcceptance': False}


def main():
    if shutil.disk_usage(Path(__file__).resolve().parent).free < 500_000_000:
        raise RuntimeError('Owned socket observation disk reserve unavailable')
    rows = [trial(40), trial(4 * 1024 * 1024)]
    print(json.dumps({'diagnosticOnly': True, 'nativeAcceptance': False,
        'nativeYokeImageReadOrExecuted': False, 'platformScope': 'actual local Python sockets only',
        'sourceSha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'trials': rows}, indent=2))


if __name__ == '__main__':
    main()
