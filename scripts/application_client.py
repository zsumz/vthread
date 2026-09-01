"""Independent native-socket client and bounded service-process ownership."""

import json
from pathlib import Path
import select
import socket
import struct
import subprocess
import threading
import time

import evidence


def receive(stream, count):
    data = bytearray()
    while len(data) < count:
        part = stream.recv(count - len(data))
        if not part:
            raise EOFError(f"peer closed after {len(data)}/{count} bytes")
        data.extend(part)
    return bytes(data)


def frame(sequence, output=256):
    payload = bytes((sequence + index * 17) & 255 for index in range(64))
    return struct.pack("!IIQ", len(payload), output, sequence) + payload


def expected(sequence, output=256):
    payload = frame(sequence)[16:]
    return struct.pack("!Q", sequence) + bytes(
        payload[index % len(payload)] ^ ((sequence + index) & 255) for index in range(output))


def exchange(stream, sequence, output=256):
    request = frame(sequence, output)
    start = time.perf_counter_ns()
    stream.sendall(request)
    response = receive(stream, output + 8)
    elapsed = time.perf_counter_ns() - start
    assert response == expected(sequence, output), "response payload or sequence mismatch"
    return elapsed, response


class Service:
    def __init__(self, binary, out, carriers, workers, queue, timeout_ms):
        self.out = Path(out)
        self.out.mkdir()
        self.config = dict(carriers=carriers, workers=workers, queue=queue, timeout_ms=timeout_ms)
        self.command = [str(binary), *map(str, self.config.values())]
        self.stderr = (self.out / "server.stderr").open("w")
        self.process = subprocess.Popen(self.command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.stderr, text=True)
        self.events, self.samples = [], []
        self.done = threading.Event()
        self.start = time.monotonic()
        self.sampler = threading.Thread(target=self._sample, daemon=True)
        self.sampler.start()
        self.stopped = None
        try:
            ready = self._read()
            assert ready["event"] == "ready"
            address, port = ready["address"].rsplit(":", 1)
            self.address = (address, int(port))
            self.until(lambda state: state["registered"] >= 1)
            self.baseline = evidence.resources(self.process.pid, self.start)
            self.recovered_resources = []
        except BaseException:
            self.close()
            raise

    def _sample(self):
        while not self.done.is_set():
            self.samples.append(evidence.resources(self.process.pid, self.start))
            self.done.wait(0.25)

    def _read(self):
        assert select.select([self.process.stdout], [], [], 10)[0], "service control timed out"
        line = self.process.stdout.readline()
        assert line, "service exited before its control reply"
        event = json.loads(line)
        self.events.append(event)
        return event

    def control(self, command):
        self.process.stdin.write(command + "\n")
        self.process.stdin.flush()
        return self._read()

    def until(self, predicate, seconds=5):
        deadline = time.monotonic() + seconds
        while True:
            state = self.control("stats")
            if predicate(state):
                return state
            assert time.monotonic() < deadline, f"service condition timed out: {state}"
            time.sleep(0.01)

    def connect(self, greeting=True, small_receive=False):
        stream = socket.socket()
        stream.settimeout(5)
        try:
            if small_receive:
                stream.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024)
            stream.connect(self.address)
            if greeting:
                assert receive(stream, 1) == b"\x01", "healthy client was rejected"
            return stream
        except BaseException:
            stream.close()
            raise

    def stop(self):
        start = time.monotonic()
        self.stopped = self.control("shutdown")
        self.shutdown_seconds = time.monotonic() - start
        assert self.stopped["event"] == "stopped"
        assert self.process.wait(timeout=10) == 0, "service exited unsuccessfully"
        return self.stopped

    def recovered(self):
        state = self.until(lambda value: value["active"] + value["pending"] == 0
                           and value["runtime_active"] == self.config["workers"] + 1)
        # Drops and readiness deregistration follow the ownership counter transition.
        self.until(lambda value: value["readiness"] == value["registered"] == 1)
        sample = evidence.resources(self.process.pid, self.start)
        self.recovered_resources.append(sample)
        if self.baseline["fds"] is not None:
            assert sample["fds"] == self.baseline["fds"], "connection descriptor leak"
        return state

    def close(self):
        try:
            if self.process.poll() is None:
                self.stop()
        finally:
            if self.process.poll() is None:
                self.process.kill()
                self.process.wait(timeout=10)
            self.done.set()
            self.sampler.join(timeout=5)
            assert not self.sampler.is_alive()
            self.stderr.close()
            self.process.stdin.close()
            self.process.stdout.close()
            record = dict(config=self.config, command=self.command, exit=self.process.returncode,
                          events=self.events, resources=self.samples, stopped=self.stopped,
                          shutdown_seconds=getattr(self, "shutdown_seconds", None),
                          baseline=getattr(self, "baseline", None),
                          recovered_resources=getattr(self, "recovered_resources", []),
                          stderr_sha256=evidence.digest(self.out / "server.stderr"))
            (self.out / "server.json").write_text(json.dumps(record, indent=2) + "\n")

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
