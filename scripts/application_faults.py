"""Prove overload, partial clients, read/write deadlines, recovery and loaded shutdown."""

import json
import socket
import struct
from contextlib import ExitStack

from application_client import Service, exchange, frame, receive


def healthy(server):
    with server.connect() as stream:
        exchange(stream, 911)
    return server.recovered()


def stalled_reader(server):
    stream = server.connect(small_receive=True)
    try:
        # 128 requests offer 8 MiB of responses, bounded independently of socket buffers.
        stream.sendall(b"".join(frame(sequence, 65536) for sequence in range(128)))
        return stream
    except BaseException:
        stream.close()
        raise


def run(binary, out, carriers):
    out.mkdir()
    results = {}
    with Service(binary, out / "overload", carriers, 2, 2, 5000) as server, ExitStack() as sockets:
        for _ in range(2):
            sockets.enter_context(server.connect())
        server.until(lambda state: state["active"] == state["io_readers"] == 2)
        for _ in range(2):
            sockets.enter_context(server.connect(greeting=False))
        full = server.until(lambda state: state["pending"] == 2)
        for _ in range(16):
            with server.connect(greeting=False) as rejected:
                assert receive(rejected, 1) == b"\x02", "saturated queue did not reject"
        saturated = server.until(lambda state: state["rejected"] == 16)
        sockets.close()
        server.until(lambda state: state["active"] + state["pending"] == 0)
        results["overload"] = dict(full=full, saturated=saturated, recovered=healthy(server))

    with Service(binary, out / "clients", carriers, 2, 2, 250) as server:
        with server.connect() as slow:
            slow.sendall(b"\x00")
            reading = server.until(lambda state: state["io_readers"] >= 1)
            expired = server.until(lambda state: state["deadlines"] == 1 and state["active"] == 0)
            assert slow.recv(1) == b"", "expired partial writer was retained"
        with server.connect() as malformed:
            malformed.sendall(struct.pack("!IIQ", 4097, 1, 0))
            assert malformed.recv(1) == b""
        with server.connect() as partial:
            partial.sendall(frame(7)[:18])
            partial.shutdown(socket.SHUT_WR)
            assert partial.recv(1) == b""
        settled = server.until(lambda state: state["malformed"] == state["disconnected"] == 1
                               and state["active"] == 0)
        results["clients"] = dict(reading=reading, expired=expired, settled=settled,
                                  recovered=healthy(server))

    with Service(binary, out / "slow-reader", carriers, 2, 2, 1000) as server:
        with stalled_reader(server):
            writing = server.until(lambda state: state["io_writers"] >= 1)
            expired = server.until(lambda state: state["deadlines"] == 1 and state["active"] == 0)
        results["slow_reader"] = dict(writing=writing, expired=expired, recovered=healthy(server))

    with Service(binary, out / "shutdown", carriers, 4, 2, 5000) as server, ExitStack() as sockets:
        for _ in range(2):
            sockets.enter_context(server.connect())
        for _ in range(2):
            sockets.enter_context(stalled_reader(server))
        server.until(lambda state: state["io_readers"] >= 2 and state["io_writers"] >= 2)
        for _ in range(2):
            sockets.enter_context(server.connect(greeting=False))
        before = server.until(lambda state: state["pending"] == 2)
        after = server.stop()
        assert server.shutdown_seconds < 2, "shutdown exceeded the operational gate"
        results["shutdown"] = dict(before=before, after=after, seconds=server.shutdown_seconds)
    (out / "faults.json").write_text(json.dumps(results, indent=2) + "\n")
    return results
